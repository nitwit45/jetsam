//! The only module that removes anything. Every item is re-verified against the live
//! filesystem immediately before it goes (flags, deny-list, type, mtime, size, and for a
//! worktree its git state and the processes inside it), command items are checked against
//! an allow-list, and each outcome is appended to a JSONL ops log that powers undo of the
//! last batch.

use std::collections::HashMap;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use trash::macos::{DeleteMethod, TrashContextExtMacos};

use crate::rules;
use crate::scanner;
use crate::system::run_cmd;
use crate::types::{flags, CleanFailure, CleanProgress, CleanResult, Item, Kind, ScanResult, UndoResult};
use crate::worktrees::{self, ProcessCwds};

/// The scan id the Aged view's results are stored under, alongside the last full scan.
pub const AGED_SCAN_ID: &str = "aged";

/// Failure reason when macOS will not move an item to the Trash. The UI offers an explicit
/// permanent delete for exactly these; nothing is ever removed permanently on its own.
pub const TRASH_REFUSED: &str = "trash-refused";

/// Scan results kept server-side so `clean` only ever acts on paths this process produced:
/// the latest scan, plus the latest Aged listing.
#[derive(Default)]
pub struct ScanStore {
    scans: Mutex<HashMap<String, Vec<Item>>>,
}

impl ScanStore {
    pub fn put(&self, result: &ScanResult) {
        let items: Vec<Item> = result.categories.iter().flat_map(|c| c.items.iter().cloned()).collect();
        if let Ok(mut s) = self.scans.lock() {
            s.retain(|id, _| id == AGED_SCAN_ID);
            s.insert(result.id.clone(), items);
        }
    }

    pub fn put_aged(&self, items: &[Item]) {
        if let Ok(mut s) = self.scans.lock() {
            s.insert(AGED_SCAN_ID.to_string(), items.to_vec());
        }
    }

    pub fn select(&self, scan_id: &str, ids: &[String]) -> Result<Vec<Item>, String> {
        let s = self.scans.lock().map_err(|_| "store poisoned".to_string())?;
        let items = s.get(scan_id).ok_or_else(|| format!("unknown scan {scan_id}; rescan first"))?;
        let by_id: HashMap<&str, &Item> = items.iter().map(|i| (i.id.as_str(), i)).collect();
        Ok(ids.iter().filter_map(|id| by_id.get(id.as_str()).map(|i| (*i).clone())).collect())
    }
}

/// Identifies a file across a rename: a moved item keeps its device and inode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileId {
    pub dev: u64,
    pub ino: u64,
}

fn file_id(path: &Path) -> Option<FileId> {
    std::fs::symlink_metadata(path).ok().map(|m| FileId { dev: m.dev(), ino: m.ino() })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpsEntry {
    pub ts: i64,
    pub batch: String,
    pub id: String,
    pub path: String,
    pub bytes: u64,
    pub kind: Kind,
    pub permanent: bool,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The underlying error behind `reason`, for the log only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Where the item landed in the Trash. Undo restores from here and nowhere else.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trashed_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trashed_id: Option<FileId>,
}

/// Binaries a `command` item may invoke. Anything else is refused before it runs.
const COMMAND_BINARIES: [&str; 2] = ["brew", "docker"];

/// The docker subcommands a command item may run, after `docker --context <name>`.
/// `container prune` and `system prune` are absent on purpose: a stopped container can
/// hold data in its writable layer.
const DOCKER_ACTIONS: [[&str; 2]; 2] = [["image", "prune"], ["builder", "prune"]];

/// NSFileManager trash: no Finder, no AppleScript, no automation permission prompt.
pub fn trash_context() -> trash::TrashContext {
    let mut ctx = trash::TrashContext::default();
    ctx.set_delete_method(DeleteMethod::NsFileManager);
    ctx
}

/// `~/Library/Application Support/Jetsam`: the ops log lives here.
pub fn support_dir(home: &Path) -> PathBuf {
    home.join("Library/Application Support/Jetsam")
}

pub fn ops_log_path(home: &Path) -> PathBuf {
    support_dir(home).join("ops.log")
}

/// Builds before the rename kept their data in `Application Support/fresh`. Move it across
/// once so the ops log, and with it undo, survives the upgrade. Never overwrites: if the
/// new folder already exists the old one is left where it is.
pub fn migrate_legacy_support_dir(home: &Path) {
    let old = home.join("Library/Application Support/fresh");
    let new = support_dir(home);
    if old.is_dir() && !new.exists() {
        let _ = std::fs::rename(&old, &new);
    }
}

fn append_ops(home: &Path, entries: &[OpsEntry]) {
    let path = ops_log_path(home);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) else { return };
    for e in entries {
        if let Ok(line) = serde_json::to_string(e) {
            let _ = writeln!(f, "{line}");
        }
    }
}

/// Docker argv must pin the engine the scan checked was local, and do only what is allowed.
fn verify_docker_argv(argv: &[String]) -> Result<(), String> {
    let [_, flag, context, rest @ ..] = argv else { return Err("docker command without --context".into()) };
    if flag != "--context" || context.is_empty() || context.starts_with('-') {
        return Err("docker command without --context".into());
    }
    if !DOCKER_ACTIONS.iter().any(|action| rest.starts_with(&action.map(String::from))) {
        return Err(format!("docker {} is not allowed", rest.join(" ")));
    }
    Ok(())
}

/// Command items run external tools, so the checks are on the argv rather than a path.
/// Docker volumes hold real data (databases, uploads), so any argument mentioning
/// `volume` is refused outright, whatever the subcommand around it.
fn verify_command(item: &Item) -> Result<(), String> {
    if item.apply.is_empty() {
        return Err("command has nothing to run".into());
    }
    for argv in &item.apply {
        let Some(bin) = argv.first() else { return Err("empty command".into()) };
        if !COMMAND_BINARIES.contains(&bin.as_str()) {
            return Err(format!("command {bin} is not allowed"));
        }
        if argv.iter().any(|a| a.to_ascii_lowercase().contains("volume")) {
            return Err("refused: command touches volumes".into());
        }
        if bin == "docker" {
            verify_docker_argv(argv)?;
        }
    }
    Ok(())
}

/// Re-check an item immediately before deletion. Any drift since the scan refuses.
pub fn verify(item: &Item, home: &Path, cwds: &ProcessCwds) -> Result<(), String> {
    for f in [flags::REPORT_ONLY, flags::DIRTY, flags::UNMERGED, flags::GIT_TRACKED, flags::RECENT] {
        if item.has_flag(f) {
            return Err(format!("flagged {f}"));
        }
    }
    if item.kind == Kind::Command {
        return verify_command(item);
    }
    let path = Path::new(&item.path);
    if !path.is_absolute() {
        return Err("relative path".into());
    }
    if path.components().any(|c| matches!(c, Component::ParentDir | Component::CurDir)) {
        return Err("path is not normalised".into());
    }
    if rules::is_protected_root(path, home) {
        return Err("protected root".into());
    }
    if rules::is_denied(path, home) {
        return Err("deny-listed".into());
    }
    let meta = scanner::lstat(path).ok_or("no longer exists")?;
    if meta.is_symlink {
        return Err("is a symlink".into());
    }
    match item.kind {
        Kind::Dir if !meta.is_dir => return Err("not a directory any more".into()),
        Kind::File if meta.is_dir => return Err("not a file any more".into()),
        _ => {}
    }
    if meta.mtime != item.mtime {
        return Err("modified since scan".into());
    }
    if item.kind == Kind::File && meta.bytes != item.bytes {
        return Err("size changed since scan".into());
    }
    if item.kind == Kind::Dir {
        if item.has_flag(flags::WORKTREE) {
            worktrees::verify_removable(path, item.repo.as_deref(), cwds)?;
        } else if rules::contains_git_entry(path) {
            return Err("contains a git repository".into());
        }
    }
    Ok(())
}

/// Why an item was not removed: `reason` goes to the UI, `detail` only to the ops log.
struct Refusal {
    reason: String,
    detail: Option<String>,
}

impl Refusal {
    fn trash_refused(detail: impl Into<String>) -> Self {
        Self { reason: TRASH_REFUSED.into(), detail: Some(detail.into()) }
    }
}

impl From<String> for Refusal {
    fn from(reason: String) -> Self {
        Self { reason, detail: None }
    }
}

struct Landed {
    path: PathBuf,
    id: FileId,
}

/// The entry in `trash_dir` that is the file we just trashed. The plain name is tried
/// first; on a name clash macOS appends a timestamp, so fall back to matching the id.
fn locate_in_trash(trash_dir: &Path, name: &std::ffi::OsStr, id: FileId) -> Option<Landed> {
    let plain = trash_dir.join(name);
    if file_id(&plain) == Some(id) {
        return Some(Landed { path: plain, id });
    }
    std::fs::read_dir(trash_dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| file_id(p) == Some(id))
        .map(|path| Landed { path, id })
}

/// Moves `path` to the Trash and reports where it landed when that can be found (an item
/// on another volume goes to that volume's Trash, which undo does not cover).
fn move_to_trash(ctx: &trash::TrashContext, path: &Path, home: &Path) -> Result<Option<Landed>, Refusal> {
    let trash_dir = home.join(".Trash");
    if path.starts_with(&trash_dir) {
        return Err(Refusal::trash_refused("already in the Trash"));
    }
    let id = file_id(path);
    ctx.delete(path).map_err(|e| Refusal::trash_refused(e.to_string()))?;
    Ok(id.zip(path.file_name()).and_then(|(id, name)| locate_in_trash(&trash_dir, name, id)))
}

fn remove_permanent(path: &Path, is_dir: bool) -> Result<(), String> {
    if is_dir {
        std::fs::remove_dir_all(path).map_err(|e| e.to_string())
    } else {
        std::fs::remove_file(path).map_err(|e| e.to_string())
    }
}

fn run_apply(item: &Item) -> Result<(), String> {
    for argv in &item.apply {
        let Some((cmd, args)) = argv.split_first() else { continue };
        let out = std::process::Command::new(cmd).args(args).output().map_err(|e| format!("{cmd}: {e}"))?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            let tail: String = err
                .lines()
                .rev()
                .take(3)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join(" | ");
            return Err(format!("{} failed: {tail}", argv.join(" ")));
        }
    }
    Ok(())
}

fn remove(
    ctx: &trash::TrashContext,
    item: &Item,
    permanent: bool,
    home: &Path,
) -> Result<Option<Landed>, Refusal> {
    let path = Path::new(&item.path);
    match item.kind {
        Kind::Command => run_apply(item).map(|_| None).map_err(Refusal::from),
        _ if permanent => remove_permanent(path, item.kind == Kind::Dir).map(|_| None).map_err(Refusal::from),
        _ => move_to_trash(ctx, path, home),
    }
}

pub fn clean<F>(items: &[Item], permanent: bool, home: &Path, batch: &str, progress: F) -> CleanResult
where
    F: Fn(CleanProgress),
{
    let total = items.len();
    let mut freed = 0u64;
    let mut failed = Vec::new();
    let mut log = Vec::with_capacity(total);
    let mut pruned_repos: Vec<String> = Vec::new();
    let ctx = trash_context();
    // Only worktree items consult the process list, so skip the lsof call without them.
    let cwds = if items.iter().any(|i| i.has_flag(flags::WORKTREE)) {
        ProcessCwds::load()
    } else {
        ProcessCwds::unknown()
    };
    for (i, item) in items.iter().enumerate() {
        let outcome = verify(item, home, &cwds)
            .map_err(Refusal::from)
            .and_then(|_| remove(&ctx, item, permanent, home));
        let mut entry = OpsEntry {
            ts: scanner::now(),
            batch: batch.to_string(),
            id: item.id.clone(),
            path: item.path.clone(),
            bytes: item.bytes,
            kind: item.kind,
            permanent: permanent && item.kind != Kind::Command,
            ok: outcome.is_ok(),
            reason: None,
            note: None,
            trashed_path: None,
            trashed_id: None,
        };
        match outcome {
            Ok(landed) => {
                freed += item.bytes;
                if let Some(l) = landed {
                    entry.trashed_path = Some(l.path.to_string_lossy().into_owned());
                    entry.trashed_id = Some(l.id);
                }
                if item.has_flag(flags::WORKTREE) {
                    if let Some(repo) = &item.repo {
                        if !pruned_repos.contains(repo) {
                            let _ = run_cmd("git", &["-C", repo, "worktree", "prune"]);
                            pruned_repos.push(repo.clone());
                        }
                    }
                }
            }
            Err(refusal) => {
                failed.push(CleanFailure { id: item.id.clone(), reason: refusal.reason.clone() });
                entry.reason = Some(refusal.reason);
                entry.note = refusal.detail;
            }
        }
        log.push(entry);
        if (i + 1) % 20 == 0 || i + 1 == total {
            progress(CleanProgress { done: i + 1, total, freed_bytes: freed });
        }
    }
    append_ops(home, &log);
    CleanResult { freed_bytes: freed, failed }
}

fn read_last_batch(home: &Path) -> Vec<OpsEntry> {
    let Ok(text) = std::fs::read_to_string(ops_log_path(home)) else { return Vec::new() };
    let entries: Vec<OpsEntry> = text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    let Some(last) = entries.last() else { return Vec::new() };
    let batch = last.batch.clone();
    entries.into_iter().filter(|e| e.batch == batch).collect()
}

/// Puts one trashed entry back. Only the exact Trash entry recorded at delete time is
/// moved, and never over something that now sits at the original path.
fn restore(e: &OpsEntry) -> bool {
    let (Some(from), Some(id)) = (e.trashed_path.as_deref(), e.trashed_id) else { return false };
    let (from, to) = (Path::new(from), Path::new(&e.path));
    if file_id(from) != Some(id) || std::fs::symlink_metadata(to).is_ok() {
        return false;
    }
    if !to.parent().is_some_and(Path::is_dir) {
        return false;
    }
    std::fs::rename(from, to).is_ok()
}

/// Restores what the last batch sent to the Trash.
pub fn undo_last(home: &Path) -> UndoResult {
    let trashed: Vec<OpsEntry> = read_last_batch(home)
        .into_iter()
        .filter(|e| e.ok && !e.permanent && e.kind != Kind::Command)
        .collect();
    let restored = trashed.iter().filter(|e| restore(e)).count();
    UndoResult { restored, skipped: trashed.len() - restored }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Kind;

    fn item(path: &Path, kind: Kind) -> Item {
        let m = scanner::lstat(path).unwrap();
        Item {
            id: "t".into(),
            path: path.to_string_lossy().into_owned(),
            bytes: m.bytes,
            mtime: m.mtime,
            kind,
            repo: None,
            repo_last_active: None,
            last_used: None,
            flags: vec![],
            apply: vec![],
        }
    }

    fn check(it: &Item, home: &Path) -> Result<(), String> {
        verify(it, home, &ProcessCwds::unknown())
    }

    #[test]
    fn verify_accepts_unchanged_file_and_dir() {
        let td = tempfile::tempdir().unwrap();
        let home = td.path();
        let f = home.join("Library/Caches/app/blob");
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, b"hello").unwrap();
        assert_eq!(check(&item(&f, Kind::File), home), Ok(()));
        assert_eq!(check(&item(f.parent().unwrap(), Kind::Dir), home), Ok(()));
    }

    #[test]
    fn verify_refuses_drift() {
        let td = tempfile::tempdir().unwrap();
        let home = td.path();
        let f = home.join("Downloads/x.dmg");
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, b"hello").unwrap();
        let mut it = item(&f, Kind::File);
        it.mtime -= 1;
        assert_eq!(check(&it, home), Err("modified since scan".to_string()));
        let mut it = item(&f, Kind::File);
        it.bytes += 512;
        assert_eq!(check(&it, home), Err("size changed since scan".to_string()));
        let it = item(&f, Kind::File);
        std::fs::remove_file(&f).unwrap();
        assert_eq!(check(&it, home), Err("no longer exists".to_string()));
    }

    #[test]
    fn verify_refuses_symlink_kind_mismatch_flags() {
        let td = tempfile::tempdir().unwrap();
        let home = td.path();
        let real = home.join("real");
        std::fs::write(&real, b"x").unwrap();
        let link = home.join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert_eq!(check(&item(&link, Kind::File), home), Err("is a symlink".to_string()));
        assert!(check(&item(&real, Kind::Dir), home).is_err());
        let mut it = item(&real, Kind::File);
        it.flags.push(flags::REPORT_ONLY.into());
        assert_eq!(check(&it, home), Err("flagged report-only".to_string()));
        let mut it = item(&real, Kind::File);
        it.flags.push(flags::DIRTY.into());
        assert!(check(&it, home).is_err());
    }

    #[test]
    fn verify_refuses_denied_and_protected() {
        let td = tempfile::tempdir().unwrap();
        let home = td.path();
        let key = home.join(".ssh/id_rsa");
        std::fs::create_dir_all(key.parent().unwrap()).unwrap();
        std::fs::write(&key, b"k").unwrap();
        assert_eq!(check(&item(&key, Kind::File), home), Err("deny-listed".to_string()));
        let git = home.join("proj/.git/HEAD");
        std::fs::create_dir_all(git.parent().unwrap()).unwrap();
        std::fs::write(&git, b"ref").unwrap();
        assert_eq!(check(&item(&git, Kind::File), home), Err("deny-listed".to_string()));
        assert_eq!(check(&item(home, Kind::Dir), home), Err("protected root".to_string()));
        let caches = home.join("Library/Caches");
        std::fs::create_dir_all(&caches).unwrap();
        assert_eq!(check(&item(&caches, Kind::Dir), home), Err("protected root".to_string()));
    }

    fn command(apply: &[Vec<&str>]) -> Item {
        Item {
            id: "docker:1".into(),
            path: "docker prune".into(),
            bytes: 1,
            mtime: 0,
            kind: Kind::Command,
            repo: None,
            repo_last_active: None,
            last_used: None,
            flags: vec![],
            apply: apply.iter().map(|argv| argv.iter().map(|s| s.to_string()).collect()).collect(),
        }
    }

    #[test]
    fn verify_refuses_volume_commands_and_unknown_binaries() {
        let home = Path::new("/Users/t");
        let prune = vec!["docker", "--context", "orbstack", "image", "prune", "-af"];
        assert_eq!(
            check(
                &command(&[
                    prune.clone(),
                    vec!["docker", "--context", "orbstack", "builder", "prune", "-af"]
                ]),
                home
            ),
            Ok(())
        );
        assert_eq!(check(&command(&[vec!["brew", "cleanup", "--prune=all", "-s"]]), home), Ok(()));
        for argv in [
            vec!["docker", "volume", "prune", "-f"],
            vec!["docker", "system", "prune", "--volumes"],
            vec!["docker", "VOLUME", "rm", "data"],
            vec!["docker", "run", "-v", "named-volume:/data", "alpine"],
        ] {
            assert_eq!(
                check(&command(&[prune.clone(), argv.clone()]), home),
                Err("refused: command touches volumes".to_string()),
                "{argv:?}"
            );
        }
        assert_eq!(
            check(&command(&[vec!["rm", "-rf", "/"]]), home),
            Err("command rm is not allowed".to_string())
        );
        assert_eq!(
            check(&command(&[vec!["image", "prune"]]), home),
            Err("command image is not allowed".to_string())
        );
        assert_eq!(check(&command(&[vec![]]), home), Err("empty command".to_string()));
        assert_eq!(
            check(&command(&[vec!["docker", "image", "prune", "-f"]]), home),
            Err("docker command without --context".to_string()),
            "an unpinned engine could be a remote host"
        );
        for argv in [
            vec!["docker", "--context", "orbstack", "container", "prune", "-f"],
            vec!["docker", "--context", "orbstack", "system", "prune", "-af"],
            vec!["docker", "--context", "orbstack", "rmi", "postgres"],
        ] {
            assert!(check(&command(std::slice::from_ref(&argv)), home).is_err(), "{argv:?}");
        }
        assert_eq!(check(&command(&[]), home), Err("command has nothing to run".to_string()));
        let mut flagged = command(&[prune]);
        flagged.flags.push(flags::REPORT_ONLY.into());
        assert_eq!(check(&flagged, home), Err("flagged report-only".to_string()));
    }

    #[test]
    fn verify_refuses_unnormalised_paths() {
        let td = tempfile::tempdir().unwrap();
        let home = td.path();
        let f = home.join("Library/Caches/app/blob");
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, b"hello").unwrap();
        let mut it = item(&f, Kind::File);
        it.path = home.join("Library/Caches/app/../app/blob").to_string_lossy().into_owned();
        assert_eq!(check(&it, home), Err("path is not normalised".to_string()));
        let mut it = item(&f, Kind::File);
        it.path = "Library/Caches/app/blob".into();
        assert_eq!(check(&it, home), Err("relative path".to_string()));
    }

    #[test]
    fn verify_refuses_a_directory_holding_git() {
        let td = tempfile::tempdir().unwrap();
        let home = td.path();
        let dist = home.join("code/dist");
        std::fs::create_dir_all(dist.join(".git")).unwrap();
        assert_eq!(check(&item(&dist, Kind::Dir), home), Err("contains a git repository".to_string()));
        let build = home.join("code/app/build");
        std::fs::create_dir_all(&build).unwrap();
        std::fs::write(build.join(".git"), "gitdir: ../.git/modules/build").unwrap();
        assert_eq!(check(&item(&build, Kind::Dir), home), Err("contains a git repository".to_string()));
    }

    #[test]
    fn verify_rechecks_worktrees_live() {
        let td = tempfile::tempdir().unwrap();
        let home = td.path();
        let wt = home.join("app/.claude/worktrees/feat");
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::write(wt.join(".git"), "gitdir: /nowhere").unwrap();
        let mut it = item(&wt, Kind::Dir);
        it.flags.push(flags::WORKTREE.into());
        it.repo = Some(home.join("app").to_string_lossy().into_owned());
        assert_eq!(
            verify(&it, home, &ProcessCwds::unknown()),
            Err("a running process is working in it".to_string()),
            "unknown cwds fail closed"
        );
        assert_eq!(
            verify(&it, home, &ProcessCwds::known(&[])),
            Err("has uncommitted or ignored files".to_string()),
            "a git error counts as dirty"
        );
    }

    #[test]
    fn store_keeps_aged_results_beside_the_latest_scan() {
        let td = tempfile::tempdir().unwrap();
        let f = td.path().join("big.mov");
        std::fs::write(&f, b"x").unwrap();
        let mut aged = item(&f, Kind::File);
        aged.id = "aged:0".into();
        let scan = |id: &str| ScanResult { id: id.into(), disk_total: 0, disk_free: 0, categories: vec![] };
        let store = ScanStore::default();
        store.put_aged(&[aged]);
        store.put(&scan("scan-light-1"));
        store.put(&scan("scan-light-2"));
        assert_eq!(store.select(AGED_SCAN_ID, &["aged:0".into()]).unwrap().len(), 1);
        assert!(store.select("scan-light-2", &[]).is_ok());
        assert!(store.select("scan-light-1", &[]).is_err(), "only the latest scan is kept");
    }

    #[test]
    fn items_already_in_the_trash_are_refused_not_removed() {
        let td = tempfile::tempdir().unwrap();
        let home = td.path();
        let f = home.join(".Trash/old.zip");
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, b"zip").unwrap();
        let res = clean(&[item(&f, Kind::File)], false, home, "b1", |_| {});
        assert!(f.exists());
        assert_eq!(res.freed_bytes, 0);
        assert_eq!(res.failed.len(), 1);
        assert_eq!(res.failed[0].reason, TRASH_REFUSED);
        let last = read_last_batch(home);
        assert!(!last[0].ok && !last[0].permanent);
        assert_eq!(last[0].note.as_deref(), Some("already in the Trash"));
    }

    fn trashed_entry(batch: &str, orig: &Path, landed: &Path) -> OpsEntry {
        OpsEntry {
            ts: 0,
            batch: batch.into(),
            id: "project_artifacts:1".into(),
            path: orig.to_string_lossy().into_owned(),
            bytes: 1,
            kind: Kind::Dir,
            permanent: false,
            ok: true,
            reason: None,
            note: None,
            trashed_path: Some(landed.to_string_lossy().into_owned()),
            trashed_id: file_id(landed),
        }
    }

    #[test]
    fn undo_restores_the_recorded_trash_entry_and_skips_conflicts() {
        let td = tempfile::tempdir().unwrap();
        let home = td.path();
        let trash = home.join(".Trash");
        let unrelated = trash.join("node_modules");
        let ours = trash.join("node_modules 15-21-25-376");
        let conflicting = trash.join("node_modules 15-21-25-377");
        for d in [&unrelated, &ours, &conflicting] {
            std::fs::create_dir_all(d).unwrap();
        }
        let a = home.join("code/a/node_modules");
        let b = home.join("code/b/node_modules");
        let c = home.join("code/c/node_modules");
        std::fs::create_dir_all(a.parent().unwrap()).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::create_dir_all(c.parent().unwrap()).unwrap();
        let mut gone = trashed_entry("b1", &c, &trash.join("node_modules 9"));
        gone.trashed_id = Some(FileId { dev: 0, ino: 0 });
        append_ops(home, &[trashed_entry("b1", &a, &ours), trashed_entry("b1", &b, &conflicting), gone]);

        let res = undo_last(home);
        assert_eq!((res.restored, res.skipped), (1, 2));
        assert!(a.is_dir() && !ours.exists(), "restored from the recorded entry");
        assert!(unrelated.is_dir(), "a same-named entry already in the Trash is left alone");
        assert!(conflicting.is_dir(), "nothing is moved over a path that exists again");
        assert!(!c.exists());
    }

    #[test]
    fn clean_permanent_deletes_and_logs() {
        let td = tempfile::tempdir().unwrap();
        let home = td.path();
        let d = home.join("Library/Caches/app");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("a"), vec![0u8; 10_000]).unwrap();
        let dir_item = item(&d, Kind::Dir);
        let mut gone = item(&d, Kind::Dir);
        gone.id = "gone".into();
        gone.path = home.join("Library/Caches/nope").to_string_lossy().into_owned();
        let res = clean(&[dir_item.clone(), gone], true, home, "b1", |_| {});
        assert!(!d.exists());
        assert_eq!(res.freed_bytes, dir_item.bytes);
        assert_eq!(res.failed.len(), 1);
        assert_eq!(res.failed[0].id, "gone");
        let log = std::fs::read_to_string(ops_log_path(home)).unwrap();
        assert_eq!(log.lines().count(), 2);
        let last = read_last_batch(home);
        assert_eq!(last.len(), 2);
        assert!(last[0].ok && !last[1].ok);
    }

    /// Uses a tempdir in $HOME because the system tempdir may sit on another volume.
    #[test]
    #[ignore = "moves a probe file through the real ~/.Trash; run with `cargo test -- --ignored`"]
    fn nsfilemanager_trash_records_where_the_file_landed() {
        let home = rules::home();
        let dir = tempfile::tempdir_in(&home).unwrap();
        let f = dir.path().join(format!("jetsam-trash-probe-{}", scanner::now()));
        std::fs::write(&f, b"probe").unwrap();
        let landed = move_to_trash(&trash_context(), &f, &home).ok().flatten().expect("located in ~/.Trash");
        assert!(!f.exists());
        assert!(landed.path.starts_with(home.join(".Trash")));
        std::fs::rename(&landed.path, &f).unwrap();
    }

    #[test]
    fn legacy_support_dir_moves_once_and_never_overwrites() {
        let td = tempfile::tempdir().unwrap();
        let home = td.path();
        let old = home.join("Library/Application Support/fresh");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("ops.log"), "old\n").unwrap();

        migrate_legacy_support_dir(home);
        assert!(!old.exists());
        assert_eq!(std::fs::read_to_string(ops_log_path(home)).unwrap(), "old\n");

        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("ops.log"), "stale\n").unwrap();
        migrate_legacy_support_dir(home);
        assert!(old.exists(), "an existing Jetsam folder is never replaced");
        assert_eq!(std::fs::read_to_string(ops_log_path(home)).unwrap(), "old\n");
    }
}
