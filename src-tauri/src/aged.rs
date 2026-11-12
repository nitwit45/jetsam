//! Aged view: files over `min_bytes` under ~ that nobody has opened in `min_age_days`.
//! "Opened" is Spotlight's last-used date when it has one, otherwise the later of atime
//! and mtime (atime alone is unreliable: backups and indexers touch it).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::rules::{self, DAY};
use crate::scanner::{self, Meta, Progress, Prune};
use crate::system::run_cmd_lenient;
use crate::types::{flags, Item, Kind};

pub const DEFAULT_MIN_AGE_DAYS: u32 = 90;

/// Paths per `mdls` call: well under the argv limit, and one bad path only costs a batch.
const MDLS_BATCH: usize = 200;

pub fn aged(min_bytes: u64, min_age_days: u32, now: i64, progress: &Progress) -> Vec<Item> {
    let big = big_files(&rules::home(), min_bytes, progress);
    let paths: Vec<PathBuf> = big.iter().map(|(p, _)| p.clone()).collect();
    let spotlight = spotlight_last_used(&paths);
    unused_since(big, spotlight, now - i64::from(min_age_days) * DAY)
}

/// Items for the files last used before `cutoff`, biggest first.
fn unused_since(big: Vec<(PathBuf, Meta)>, spotlight: Vec<Option<i64>>, cutoff: i64) -> Vec<Item> {
    let mut out: Vec<Item> = big
        .into_iter()
        .zip(spotlight)
        .filter_map(|((path, m), seen)| {
            let last_used = seen.unwrap_or(m.atime.max(m.mtime));
            (last_used < cutoff).then_some((path, m, last_used))
        })
        .enumerate()
        .map(|(n, (path, m, last_used))| Item {
            id: format!("aged:{n}"),
            path: path.to_string_lossy().into_owned(),
            bytes: m.bytes,
            mtime: m.mtime,
            kind: Kind::File,
            repo: None,
            repo_last_active: None,
            last_used: Some(last_used),
            flags: vec![rules::aged_hint(&path).to_string(), flags::REVIEW.to_string()],
            apply: vec![],
        })
        .collect();
    out.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    out
}

fn big_files(home: &Path, min_bytes: u64, progress: &Progress) -> Vec<(PathBuf, Meta)> {
    let home_p = Arc::new(home.to_path_buf());
    let mut out = Vec::new();
    scanner::walk(
        home,
        progress,
        Arc::new(move |d| {
            if rules::is_denied(d, &home_p) || rules::is_memory_dir(d, &home_p) {
                Prune::Skip
            } else {
                Prune::Descend
            }
        }),
        |path, m| {
            if !m.is_dir && m.bytes >= min_bytes {
                out.push((path.to_path_buf(), *m));
            }
        },
    );
    out
}

/// `kMDItemLastUsedDate` per path, in order. A batch whose output does not line up with
/// its paths (mdls stops at the first file it cannot find) yields `None` for all of them.
fn spotlight_last_used(paths: &[PathBuf]) -> Vec<Option<i64>> {
    let mut out = Vec::with_capacity(paths.len());
    for batch in paths.chunks(MDLS_BATCH) {
        let mut args = vec!["-name", "kMDItemLastUsedDate", "-raw"];
        args.extend(batch.iter().filter_map(|p| p.to_str()));
        let values = (args.len() == batch.len() + 3)
            .then(|| run_cmd_lenient("mdls", &args))
            .flatten()
            .map(|raw| raw.split('\0').map(parse_mdls_date).collect::<Vec<_>>())
            .filter(|v| v.len() == batch.len());
        out.extend(values.unwrap_or_else(|| vec![None; batch.len()]));
    }
    out
}

/// "2026-08-08 11:33:03 +0000" to unix seconds; anything else, "(null)" included, is None.
pub fn parse_mdls_date(s: &str) -> Option<i64> {
    let mut parts = s.trim().split(' ');
    let (date, time, zone) = (parts.next()?, parts.next()?, parts.next()?);
    let ymd = numbers(date, '-')?;
    let hms = numbers(time, ':')?;
    let offset = zone_offset(zone)?;
    let days = days_from_civil(ymd[0], ymd[1], ymd[2]);
    Some(days * DAY + hms[0] * 3600 + hms[1] * 60 + hms[2] - offset)
}

fn numbers(s: &str, sep: char) -> Option<[i64; 3]> {
    let v: Vec<i64> = s.split(sep).map(|x| x.parse().ok()).collect::<Option<_>>()?;
    v.try_into().ok()
}

/// "+0530" to seconds east of UTC.
fn zone_offset(zone: &str) -> Option<i64> {
    let (sign, digits) = match zone.split_at_checked(1)? {
        ("+", d) => (1, d),
        ("-", d) => (-1, d),
        _ => return None,
    };
    if digits.len() != 4 {
        return None;
    }
    let (h, m): (i64, i64) = (digits[..2].parse().ok()?, digits[2..].parse().ok()?);
    Some(sign * (h * 3600 + m * 60))
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mdls_dates() {
        assert_eq!(parse_mdls_date("1970-01-01 00:00:00 +0000"), Some(0));
        assert_eq!(parse_mdls_date("2026-08-08 11:33:03 +0000"), Some(1_786_188_783));
        assert_eq!(parse_mdls_date("2000-03-01 05:30:00 +0530"), Some(951_868_800));
        assert_eq!(parse_mdls_date("(null)"), None);
        assert_eq!(parse_mdls_date("/x: could not find /x."), None);
    }

    #[test]
    fn spotlight_batch_that_does_not_line_up_falls_back() {
        let missing = [PathBuf::from("/nonexistent/jetsam-a"), PathBuf::from("/nonexistent/jetsam-b")];
        assert_eq!(spotlight_last_used(&missing), vec![None, None]);
    }

    fn meta(atime: i64, mtime: i64) -> Meta {
        Meta { bytes: 1 << 20, mtime, atime, is_dir: false, is_symlink: false, dev: 1 }
    }

    #[test]
    fn only_files_unused_for_min_age_days_are_listed() {
        let now = 1_800_000_000;
        let ago = |days: i64| now - days * DAY;
        let big = vec![
            (PathBuf::from("/u/old.mov"), meta(ago(200), ago(300))),
            (PathBuf::from("/u/opened.mov"), meta(ago(5), ago(300))),
            (PathBuf::from("/u/edited.mov"), meta(ago(300), ago(10))),
            (PathBuf::from("/u/spotlight-old.mov"), meta(ago(1), ago(1))),
            (PathBuf::from("/u/spotlight-new.mov"), meta(ago(400), ago(400))),
        ];
        let spotlight = vec![None, None, None, Some(ago(120)), Some(ago(3))];
        let names = |cutoff: i64| -> Vec<String> {
            unused_since(big.clone(), spotlight.clone(), cutoff).into_iter().map(|i| i.path).collect()
        };
        assert_eq!(names(ago(90)), vec!["/u/old.mov", "/u/spotlight-old.mov"]);
        assert_eq!(names(ago(150)), vec!["/u/old.mov"]);
        assert!(names(ago(365)).is_empty());

        let items = unused_since(big.clone(), spotlight.clone(), ago(90));
        assert_eq!(items[0].last_used, Some(ago(200)));
        assert_eq!(items[1].last_used, Some(ago(120)), "Spotlight wins over atime");
        assert!(items.iter().all(|i| i.has_flag(flags::REVIEW)));
    }
}
