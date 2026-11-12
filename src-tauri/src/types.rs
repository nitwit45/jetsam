//! Shared data types: the scan modes, the item and category shapes the UI renders, and
//! the result structs every Tauri command returns. Serialised field names are the UI
//! contract, so a rename here is a frontend change too.

use serde::{Deserialize, Serialize};

/// How far a scan reaches. Modes are ordered and each includes everything below it.
///
/// - `Light`: app caches and logs, Trash, old installers, Homebrew and Docker leftovers.
/// - `Standard`: plus package-manager caches and Xcode/Android artefacts.
/// - `Deep`: plus build artefacts and stale worktrees, and old AI session logs for review.
///   Artefacts in repos active within the recency window are listed but not selectable.
/// - `Full`: `Deep` with no recency window, so artefacts in active repos count as well.
///   Still never anything git-tracked and never user data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Light,
    Standard,
    Deep,
    #[serde(alias = "nuke")]
    Full,
}

impl Mode {
    /// Parses the wire name. "nuke" is the pre-rename spelling of `Full` and still accepted.
    pub fn parse(s: &str) -> Option<Mode> {
        match s {
            "light" => Some(Mode::Light),
            "standard" => Some(Mode::Standard),
            "deep" => Some(Mode::Deep),
            "full" | "nuke" => Some(Mode::Full),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Light => "light",
            Mode::Standard => "standard",
            Mode::Deep => "deep",
            Mode::Full => "full",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Dir,
    File,
    Command,
}

pub mod flags {
    pub const DIRTY: &str = "dirty";
    pub const UNMERGED: &str = "unmerged";
    /// In a repo or worktree that is in use right now or was within the recency window.
    pub const RECENT: &str = "recent";
    pub const GIT_TRACKED: &str = "git-tracked";
    pub const REPORT_ONLY: &str = "report-only";
    pub const WORKTREE: &str = "worktree";
    /// Not regenerable. Selectable, but the UI never preselects it and a person decides.
    pub const REVIEW: &str = "review";
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub path: String,
    pub bytes: u64,
    pub mtime: i64,
    pub kind: Kind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_last_active: Option<i64>,
    /// When the file was last opened (Aged view only): Spotlight's last-used date when it
    /// has one, otherwise the later of atime and mtime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used: Option<i64>,
    #[serde(default)]
    pub flags: Vec<String>,
    /// Shell argv lists executed when a `command` item is applied. Never sent to the UI,
    /// so the frontend cannot inject a command; see `cleaner::verify_command` for the
    /// checks they still go through.
    #[serde(skip)]
    pub apply: Vec<Vec<String>>,
}

impl Item {
    pub fn has_flag(&self, f: &str) -> bool {
        self.flags.iter().any(|x| x == f)
    }

    /// Items the UI may select. `review` items stay selectable; the UI just never picks them
    /// for the user.
    pub fn selectable(&self) -> bool {
        !(self.has_flag(flags::REPORT_ONLY)
            || self.has_flag(flags::DIRTY)
            || self.has_flag(flags::UNMERGED)
            || self.has_flag(flags::GIT_TRACKED)
            || self.has_flag(flags::RECENT))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: String,
    pub label: String,
    pub mode: Mode,
    pub total_bytes: u64,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub id: String,
    pub disk_total: u64,
    pub disk_free: u64,
    pub categories: Vec<Category>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub scanned: u64,
    pub bytes: u64,
    pub current: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanProgress {
    pub done: usize,
    pub total: usize,
    pub freed_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanFailure {
    pub id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanResult {
    pub freed_bytes: u64,
    pub failed: Vec<CleanFailure>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopProc {
    pub pid: u32,
    pub rss: u64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryInfo {
    pub total_bytes: u64,
    pub free_pct: f64,
    pub swap_used: u64,
    pub swap_total: u64,
    pub compressed_bytes: u64,
    pub pageouts: u64,
    pub pressure: String,
    pub top: Vec<TopProc>,
    pub uptime_sec: u64,
    pub history: Vec<HistPoint>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FreshenStatus {
    pub hibernate_mode: i64,
    pub spotlight_dev_volume: bool,
    pub chrome_running: bool,
    pub orbstack_running: bool,
    pub vscode_excludes: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FreshenResult {
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoResult {
    pub restored: usize,
    /// Entries of the last batch that could not be put back: no longer in the Trash where
    /// they landed, or something now sits at the original path.
    pub skipped: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FdaStatus {
    pub granted: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistPoint {
    pub ts: i64,
    pub free_pct: f64,
    pub swap_used: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcRow {
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
    pub app: String,
    pub rss: u64,
    pub cpu_pct: f64,
    pub elapsed_sec: u64,
    pub reasons: Vec<String>,
    /// For a Claude Code process: the editor hosting it ("vscode" | "cursor").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
}

impl ProcRow {
    pub fn has_reason(&self, reason: &str) -> bool {
        self.reasons.iter().any(|r| r == reason)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReclaimAction {
    pub label: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReclaimResult {
    pub freed_bytes: u64,
    pub free_pct_delta: f64,
    pub swap_delta: i64,
    pub before: MemoryInfo,
    pub after: MemoryInfo,
    pub actions: Vec<ReclaimAction>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_parses_wire_names_and_legacy_alias() {
        assert_eq!(Mode::parse("light"), Some(Mode::Light));
        assert_eq!(Mode::parse("standard"), Some(Mode::Standard));
        assert_eq!(Mode::parse("deep"), Some(Mode::Deep));
        assert_eq!(Mode::parse("full"), Some(Mode::Full));
        assert_eq!(Mode::parse("nuke"), Some(Mode::Full), "pre-rename spelling still works");
        assert_eq!(Mode::parse("Full"), None, "names are case-sensitive");
        assert_eq!(Mode::parse(""), None);
    }

    #[test]
    fn mode_round_trips_and_orders() {
        for m in [Mode::Light, Mode::Standard, Mode::Deep, Mode::Full] {
            assert_eq!(Mode::parse(m.as_str()), Some(m));
        }
        assert_eq!(Mode::Full.as_str(), "full", "the wire name is never the alias");
        assert!(Mode::Light < Mode::Standard && Mode::Standard < Mode::Deep && Mode::Deep < Mode::Full);
        assert_eq!(serde_json::to_string(&Mode::Full).unwrap(), "\"full\"");
        assert_eq!(serde_json::from_str::<Mode>("\"nuke\"").unwrap(), Mode::Full);
    }

    #[test]
    fn selectable_excludes_every_blocking_flag() {
        let mut it = Item {
            id: "x:1".into(),
            path: "/tmp/x".into(),
            bytes: 1,
            mtime: 0,
            kind: Kind::File,
            repo: None,
            repo_last_active: None,
            last_used: None,
            flags: vec![],
            apply: vec![],
        };
        assert!(it.selectable());
        it.flags.push(flags::WORKTREE.into());
        assert!(it.selectable(), "worktree alone is informational");
        it.flags.push(flags::REVIEW.into());
        assert!(it.selectable(), "review items are chosen by hand, not blocked");
        for f in [flags::REPORT_ONLY, flags::DIRTY, flags::UNMERGED, flags::GIT_TRACKED, flags::RECENT] {
            let mut blocked = it.clone();
            blocked.flags.push(f.into());
            assert!(!blocked.selectable(), "{f}");
        }
    }
}
