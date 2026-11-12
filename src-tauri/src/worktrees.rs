//! Stale `.claude/worktrees` checkouts per repo, and the live checks that decide whether
//! one may go: nothing uncommitted or ignored-but-precious, merged into the main branch,
//! untouched by git for a day, and no process working inside it.

use std::path::{Path, PathBuf};

use crate::rules::{self, DAY};
use crate::scanner::{self, Progress};
use crate::system::{run_cmd, run_cmd_lenient};
use crate::types::{flags, Item, Kind};

/// A worktree git touched more recently than this may belong to an agent that is mid-task.
pub const MIN_IDLE: i64 = DAY;

/// Worktree paths from `git worktree list --porcelain`, main worktree first.
pub fn parse_porcelain(out: &str) -> Vec<PathBuf> {
    out.lines().filter_map(|l| l.strip_prefix("worktree ")).map(PathBuf::from).collect()
}

pub fn is_candidate(path: &Path) -> bool {
    path.to_string_lossy().contains("/.claude/worktrees/")
}

/// Working directories of every process the user can see, from one `lsof` call.
pub struct ProcessCwds(Option<Vec<PathBuf>>);

impl ProcessCwds {
    /// An unreadable or empty listing (this process always has a cwd) is kept as unknown,
    /// and unknown means every worktree counts as in use.
    pub fn load() -> Self {
        let cwds = run_cmd_lenient("lsof", &["-a", "-d", "cwd", "-Fn"]).map(|out| parse_lsof_cwds(&out));
        Self(cwds.filter(|c| !c.is_empty()))
    }

    pub fn unknown() -> Self {
        Self(None)
    }

    #[cfg(test)]
    pub fn known(paths: &[&str]) -> Self {
        Self(Some(paths.iter().map(PathBuf::from).collect()))
    }

    pub fn any_inside(&self, dir: &Path) -> bool {
        let Some(cwds) = &self.0 else { return true };
        let resolved = std::fs::canonicalize(dir).ok();
        cwds.iter().any(|c| c.starts_with(dir) || resolved.as_deref().is_some_and(|r| c.starts_with(r)))
    }
}

/// `lsof -F n` prints one field per line; `n` lines carry the path.
pub fn parse_lsof_cwds(out: &str) -> Vec<PathBuf> {
    out.lines().filter_map(|l| l.strip_prefix('n')).map(PathBuf::from).collect()
}

/// `git status --porcelain --ignored` output, run in the worktree `wt`, is clean only when
/// every entry is an ignored build artefact or Finder metadata. An ignored `.env` or local
/// config is someone's data.
pub fn status_is_dirty(out: &str, wt: &Path) -> bool {
    out.lines().filter(|l| !l.is_empty()).any(|l| l.strip_prefix("!! ").is_none_or(|p| !is_disposable(wt, p)))
}

/// An ignored entry is disposable when it is Finder metadata or lies inside a folder that
/// passes the deep scan's own build-artefact check, `rules::is_artifact_dir`: `target` needs
/// a `Cargo.toml` beside it, `bin` and `obj` a .NET project, and a folder holding its own
/// `.git` never counts. The worktree is itself the repository, so names that only need a
/// repo or a manifest pass.
fn is_disposable(wt: &Path, entry: &str) -> bool {
    let entry = entry.trim_matches('"');
    let parts: Vec<&str> = entry.trim_end_matches('/').split('/').collect();
    if parts.last() == Some(&".DS_Store") {
        return true;
    }
    let dirs = if entry.ends_with('/') { &parts[..] } else { &parts[..parts.len().saturating_sub(1)] };
    let mut dir = wt.to_path_buf();
    dirs.iter().any(|d| {
        dir.push(d);
        rules::is_artifact_dir(&dir, || true)
    })
}

fn base_branch(repo: &str) -> Option<&'static str> {
    ["main", "master"]
        .into_iter()
        .find(|&b| run_cmd("git", &["-C", repo, "rev-parse", "--verify", "--quiet", b]).is_some())
}

fn is_dirty(wt: &str) -> bool {
    run_cmd("git", &["-C", wt, "status", "--porcelain", "--ignored"])
        .is_none_or(|s| status_is_dirty(&s, Path::new(wt)))
}

fn is_unmerged(wt: &str, base: Option<&str>) -> bool {
    let Some(base) = base else { return true };
    let range = format!("{base}..HEAD");
    run_cmd("git", &["-C", wt, "log", "--oneline", &range]).is_none_or(|s| !s.trim().is_empty())
}

/// Newest write to the worktree's HEAD, index or reflog. `None` when its git dir cannot be
/// found, which callers treat as active.
fn last_git_activity(wt: &str) -> Option<i64> {
    let git_dir = run_cmd("git", &["-C", wt, "rev-parse", "--absolute-git-dir"])?;
    let git_dir = Path::new(git_dir.trim());
    ["HEAD", "index", "logs/HEAD"]
        .iter()
        .filter_map(|f| scanner::lstat(&git_dir.join(f)))
        .map(|m| m.mtime)
        .max()
}

fn is_recently_active(wt: &str, now: i64) -> bool {
    last_git_activity(wt).is_none_or(|t| now - t < MIN_IDLE)
}

pub fn scan(
    repos: &[PathBuf],
    now: i64,
    progress: &Progress,
    mut next_id: impl FnMut() -> String,
) -> Vec<Item> {
    let mut cwds: Option<ProcessCwds> = None;
    let mut out = Vec::new();
    for repo in repos {
        let repo_s = repo.to_string_lossy().into_owned();
        let Some(list) = run_cmd("git", &["-C", &repo_s, "worktree", "list", "--porcelain"]) else {
            continue;
        };
        let paths = parse_porcelain(&list);
        let base = base_branch(&repo_s);
        for wt in paths.iter().skip(1).filter(|p| is_candidate(p)) {
            let Some(meta) = scanner::lstat(wt) else { continue };
            if !meta.is_dir || meta.is_symlink {
                continue;
            }
            progress.note(&wt.to_string_lossy());
            let wt_s = wt.to_string_lossy().into_owned();
            let mut f = vec![flags::WORKTREE.to_string()];
            if is_dirty(&wt_s) {
                f.push(flags::DIRTY.into());
            }
            if is_unmerged(&wt_s, base) {
                f.push(flags::UNMERGED.into());
            }
            if is_recently_active(&wt_s, now) || cwds.get_or_insert_with(ProcessCwds::load).any_inside(wt) {
                f.push(flags::RECENT.into());
            }
            out.push(Item {
                id: next_id(),
                path: wt_s,
                bytes: scanner::size_of(wt, progress),
                mtime: meta.mtime,
                kind: Kind::Dir,
                repo: Some(repo_s.clone()),
                repo_last_active: None,
                last_used: None,
                flags: f,
                apply: vec![],
            });
        }
    }
    out
}

/// Live re-check right before a worktree is removed. The scan may be minutes old and an
/// agent can have started work in it since.
pub fn verify_removable(wt: &Path, repo: Option<&str>, cwds: &ProcessCwds) -> Result<(), String> {
    if cwds.any_inside(wt) {
        return Err("a running process is working in it".into());
    }
    let wt_s = wt.to_string_lossy();
    if is_dirty(&wt_s) {
        return Err("has uncommitted or ignored files".into());
    }
    if is_unmerged(&wt_s, repo.and_then(base_branch)) {
        return Err("has commits not merged into main".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "init.defaultBranch=main"])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    }

    #[test]
    fn porcelain_and_candidates() {
        let out = "worktree /Users/t/app\nHEAD abc\nbranch refs/heads/main\n\nworktree /Users/t/app/.claude/worktrees/feat-x\nHEAD def\nbranch refs/heads/feat-x\n\nworktree /Users/t/app-wt2\nHEAD 123\ndetached\n";
        let p = parse_porcelain(out);
        assert_eq!(p.len(), 3);
        assert!(!is_candidate(&p[0]));
        assert!(is_candidate(&p[1]));
        assert!(!is_candidate(&p[2]));
    }

    #[test]
    fn ignored_artifacts_are_clean_but_ignored_data_is_not() {
        let td = tempfile::tempdir().unwrap();
        let wt = td.path();
        for dir in ["node_modules", "web/dist", "target", "build", "src/__pycache__"] {
            std::fs::create_dir_all(wt.join(dir)).unwrap();
        }
        std::fs::write(wt.join("Cargo.toml"), "").unwrap();
        let dirty = |out: &str| status_is_dirty(out, wt);
        assert!(!dirty(""));
        assert!(!dirty("!! node_modules/\n!! web/dist/\n!! target/\n!! .DS_Store\n"));
        assert!(!dirty("!! build/out.o\n!! src/__pycache__/\n"));
        assert!(dirty("!! .env\n"));
        assert!(dirty("!! .claude/settings.local.json\n"));
        assert!(dirty("!! dist\n"), "a file named dist is not a build dir");
        assert!(dirty(" M src/main.rs\n"));
        assert!(dirty("?? notes.md\n!! node_modules/\n"));
    }

    #[test]
    fn ignored_folders_need_the_same_context_as_the_deep_scan() {
        let td = tempfile::tempdir().unwrap();
        let wt = td.path();
        for dir in ["tools/bin", "dotnet/bin", "crate/target", "lib/target", "vendor/dist/.git"] {
            std::fs::create_dir_all(wt.join(dir)).unwrap();
        }
        std::fs::write(wt.join("dotnet/App.csproj"), "").unwrap();
        std::fs::write(wt.join("crate/Cargo.toml"), "").unwrap();
        let dirty = |out: &str| status_is_dirty(out, wt);
        assert!(dirty("!! tools/bin/.env\n"), "a scripts folder named bin with no .NET project");
        assert!(!dirty("!! dotnet/bin/\n"), "bin next to a .csproj is build output");
        assert!(!dirty("!! crate/target/\n"), "target next to Cargo.toml is build output");
        assert!(dirty("!! lib/target/notes.txt\n"), "target with no Cargo.toml is someone's folder");
        assert!(dirty("!! vendor/dist/\n"), "a folder holding its own clone is never an artefact");
    }

    #[test]
    fn process_cwds_match_by_path_prefix() {
        let out = "p123\nfcwd\nn/Users/t/app/.claude/worktrees/x/src\np456\nfcwd\nn/Users/t\n";
        assert_eq!(parse_lsof_cwds(out).len(), 2);
        let cwds = ProcessCwds::known(&["/Users/t/app/.claude/worktrees/x/src", "/Users/t"]);
        assert!(cwds.any_inside(Path::new("/Users/t/app/.claude/worktrees/x")));
        assert!(!cwds.any_inside(Path::new("/Users/t/app/.claude/worktrees/x-2")));
        assert!(ProcessCwds::unknown().any_inside(Path::new("/anything")), "unknown means in use");
    }

    #[test]
    fn git_failure_counts_as_dirty_unmerged_and_active() {
        let td = tempfile::tempdir().unwrap();
        let not_a_repo = td.path().to_string_lossy().into_owned();
        assert!(is_dirty(&not_a_repo));
        assert!(is_unmerged(&not_a_repo, Some("main")));
        assert!(is_recently_active(&not_a_repo, scanner::now()));
    }

    #[test]
    fn live_worktree_checks() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("app");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q"]);
        std::fs::write(repo.join(".gitignore"), "node_modules/\n.env\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-qm", "init"]);
        let wt = repo.join(".claude/worktrees/feat");
        git(&repo, &["worktree", "add", "-q", "-b", "feat", &wt.to_string_lossy()]);
        let repo_s = repo.to_string_lossy().into_owned();
        let nobody = ProcessCwds::known(&[]);

        std::fs::create_dir_all(wt.join("node_modules/x")).unwrap();
        assert_eq!(verify_removable(&wt, Some(&repo_s), &nobody), Ok(()));
        assert!(is_recently_active(&wt.to_string_lossy(), scanner::now()), "just created");
        assert!(!is_recently_active(&wt.to_string_lossy(), scanner::now() + 2 * MIN_IDLE));

        let inside = wt.join("src").to_string_lossy().into_owned();
        assert!(verify_removable(&wt, Some(&repo_s), &ProcessCwds::known(&[&inside])).is_err());

        std::fs::write(wt.join(".env"), "SECRET=1").unwrap();
        assert!(verify_removable(&wt, Some(&repo_s), &nobody).is_err(), "ignored .env is data");
        std::fs::remove_file(wt.join(".env")).unwrap();

        std::fs::write(wt.join("a.txt"), "a").unwrap();
        git(&wt, &["add", "a.txt"]);
        git(&wt, &["commit", "-qm", "work"]);
        assert!(verify_removable(&wt, Some(&repo_s), &nobody).is_err(), "unmerged commit");
    }
}
