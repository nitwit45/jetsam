//! Builds the per-category item lists for a scan: which paths and commands each
//! category proposes, sized and flagged, for the mode requested. Nothing here deletes;
//! `cleaner` re-verifies every item before it acts.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use jwalk::rayon::prelude::*;

use crate::rules::{self, DAY};
use crate::scanner::{self, Meta, Progress, Prune};
use crate::system::{has_cmd, run_cmd, run_cmd_lenient};
use crate::types::{flags, Category, Item, Kind, Mode, ScanResult};
use crate::worktrees;

pub struct Ctx {
    pub home: PathBuf,
    pub now: i64,
    pub mode: Mode,
    pub recent_days: u32,
    pub progress: Arc<Progress>,
    ids: AtomicU64,
}

impl Ctx {
    /// Full mode has no recency window, whatever the caller asked for.
    fn new(home: PathBuf, mode: Mode, recent_days: u32, progress: Arc<Progress>) -> Self {
        let recent_days = if mode == Mode::Full { 0 } else { recent_days };
        Ctx { home, now: scanner::now(), mode, recent_days, progress, ids: AtomicU64::new(1) }
    }

    fn next_id(&self, cat: &str) -> String {
        format!("{cat}:{}", self.ids.fetch_add(1, Ordering::Relaxed))
    }

    fn item(&self, cat: &str, path: &Path, meta: &Meta) -> Item {
        let bytes = if meta.is_dir { scanner::size_of(path, &self.progress) } else { meta.bytes };
        Item {
            id: self.next_id(cat),
            path: path.to_string_lossy().into_owned(),
            bytes,
            mtime: meta.mtime,
            kind: if meta.is_dir { Kind::Dir } else { Kind::File },
            repo: None,
            repo_last_active: None,
            last_used: None,
            flags: vec![],
            apply: vec![],
        }
    }

    fn item_at(&self, cat: &str, path: &Path) -> Option<Item> {
        let m = scanner::lstat(path)?;
        if m.is_symlink || rules::is_denied(path, &self.home) {
            return None;
        }
        Some(self.item(cat, path, &m))
    }

    fn command_item(&self, cat: &str, label: &str, bytes: u64, apply: Vec<Vec<String>>) -> Item {
        Item {
            id: self.next_id(cat),
            path: label.to_string(),
            bytes,
            mtime: self.now,
            kind: Kind::Command,
            repo: None,
            repo_last_active: None,
            last_used: None,
            flags: vec![],
            apply,
        }
    }
}

fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

/// "1.2GB", "345.6MB", "5.6kB", "0B" -> bytes (1024-based).
pub fn parse_human_size(s: &str) -> u64 {
    let s = s.trim();
    let idx = s.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(s.len());
    let n: f64 = s[..idx].parse().unwrap_or(0.0);
    let unit = s[idx..].trim().to_ascii_uppercase();
    let mult: f64 = match unit.as_str() {
        "B" | "" => 1.0,
        "K" | "KB" | "KIB" => 1024.0,
        "M" | "MB" | "MIB" => 1024f64.powi(2),
        "G" | "GB" | "GIB" => 1024f64.powi(3),
        "T" | "TB" | "TIB" => 1024f64.powi(4),
        _ => 1.0,
    };
    (n * mult) as u64
}

/// Sum of `brew cleanup -n` output: prefers the summary line, falls back to per-line sizes.
pub fn parse_brew_cleanup(out: &str) -> u64 {
    if let Some(line) = out.lines().find(|l| l.contains("would free approximately")) {
        if let Some(rest) = line.split("approximately ").nth(1) {
            return parse_human_size(rest.split(' ').next().unwrap_or("0"));
        }
    }
    out.lines()
        .filter(|l| l.starts_with("Would remove:"))
        .filter_map(|l| l.rsplit('(').next())
        .map(|s| parse_human_size(s.trim_end_matches(')')))
        .sum()
}

/// Sum "Reclaimable" for images and build cache across `docker system df --format
/// '{{json .}}'` lines. Containers and volumes are never pruned, so they do not count.
pub fn parse_docker_df(out: &str) -> u64 {
    out.lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|v| matches!(v.get("Type").and_then(|t| t.as_str()), Some("Images" | "Build Cache")))
        .filter_map(|v| v.get("Reclaimable").and_then(|r| r.as_str()).map(|s| s.to_string()))
        .map(|r| parse_human_size(r.split(' ').next().unwrap_or("0")))
        .sum()
}

/// True for an engine on this machine: a `unix://` socket path, as OrbStack and Docker
/// Desktop use. `tcp://` and `ssh://` engines can be someone's server.
pub fn is_local_docker_endpoint(host: &str) -> bool {
    host.strip_prefix("unix://").is_some_and(|path| path.starts_with('/'))
}

/// The active docker context, when its engine is local. `docker context show` already
/// accounts for `DOCKER_HOST` and `DOCKER_CONTEXT`, so this is the engine a bare `docker`
/// call would reach.
fn local_docker_context() -> Option<String> {
    let name = run_cmd("docker", &["context", "show"])?.trim().to_string();
    if name.is_empty() {
        return None;
    }
    let host = run_cmd("docker", &["context", "inspect", &name, "--format", "{{.Endpoints.docker.Host}}"])?;
    is_local_docker_endpoint(host.trim()).then_some(name)
}

fn docker_argv(context: &str, args: &[&str]) -> Vec<String> {
    let mut v = argv(&["docker", "--context", context]);
    v.extend(argv(args));
    v
}

/// `~/Library/Caches` and `~/Library/Logs` entries, plus the per-user temp caches (report
/// only). Caches that `pkg_caches` lists (pip, Playwright, VS Code's updater) are left to it,
/// so each path is offered once, in the category its mode belongs to.
fn app_caches(ctx: &Ctx) -> Vec<Item> {
    let pkg_paths: Vec<PathBuf> = rules::pkg_cache_paths(&ctx.home).into_iter().map(|(p, _)| p).collect();
    let mut items = Vec::new();
    for (p, m) in scanner::children(&ctx.home.join("Library/Caches")) {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if rules::app_cache_skip(name) || pkg_paths.iter().any(|k| k.starts_with(&p)) {
            continue;
        }
        items.push(ctx.item("app_caches", &p, &m));
    }
    for (p, m) in scanner::children(&ctx.home.join("Library/Logs")) {
        items.push(ctx.item("app_caches", &p, &m));
    }
    for (user_dir, _) in scanner::children(Path::new("/private/var/folders")) {
        for (hash_dir, _) in scanner::children(&user_dir) {
            for (p, m) in scanner::children(&hash_dir.join("C")) {
                let mut it = ctx.item("app_caches", &p, &m);
                it.flags.push(flags::REPORT_ONLY.into());
                items.push(it);
            }
        }
    }
    items
}

fn trash(ctx: &Ctx) -> Vec<Item> {
    scanner::children(&ctx.home.join(".Trash")).iter().map(|(p, m)| ctx.item("trash", p, m)).collect()
}

fn installers(ctx: &Ctx) -> Vec<Item> {
    scanner::children(&ctx.home.join("Downloads"))
        .iter()
        .filter(|(p, m)| {
            !m.is_dir
                && rules::is_installer(p.file_name().and_then(|n| n.to_str()).unwrap_or(""), m.mtime, ctx.now)
        })
        .map(|(p, m)| ctx.item("installers", p, m))
        .collect()
}

fn homebrew(ctx: &Ctx) -> Vec<Item> {
    if !has_cmd("brew") {
        return vec![];
    }
    ctx.progress.note("brew cleanup -n");
    let Some(out) = run_cmd_lenient("brew", &["cleanup", "-n"]) else { return vec![] };
    let bytes = parse_brew_cleanup(&out);
    if bytes == 0 {
        return vec![];
    }
    vec![ctx.command_item(
        "homebrew",
        "brew cleanup --prune=all -s",
        bytes,
        vec![argv(&["brew", "cleanup", "--prune=all", "-s"])],
    )]
}

/// Images and build cache only. Stopped containers can hold data in their writable layer
/// and volumes hold real data, so neither is pruned; `cleaner::verify_command` enforces it.
fn docker(ctx: &Ctx) -> Vec<Item> {
    if !has_cmd("docker") {
        return vec![];
    }
    let Some(context) = local_docker_context() else { return vec![] };
    ctx.progress.note("docker system df");
    let Some(out) = run_cmd("docker", &["--context", &context, "system", "df", "--format", "{{json .}}"])
    else {
        return vec![];
    };
    let bytes = parse_docker_df(&out);
    if bytes == 0 {
        return vec![];
    }
    let mut apply = vec![
        docker_argv(&context, &["image", "prune", "-f"]),
        docker_argv(&context, &["builder", "prune", "-af"]),
    ];
    let label = if ctx.mode >= Mode::Deep {
        apply.push(docker_argv(&context, &["image", "prune", "-af"]));
        "docker prune (all unused images, build cache)"
    } else {
        "docker prune (dangling images, build cache)"
    };
    vec![ctx.command_item("docker", label, bytes, apply)]
}

fn pkg_caches(ctx: &Ctx) -> Vec<Item> {
    rules::pkg_cache_paths(&ctx.home)
        .into_iter()
        .filter_map(|(p, report_only)| {
            let mut it = ctx.item_at("pkg_caches", &p)?;
            if report_only {
                it.flags.push(flags::REPORT_ONLY.into());
            }
            Some(it)
        })
        .collect()
}

fn xcode_android(ctx: &Ctx) -> Vec<Item> {
    let cat = "xcode_android";
    let mut items = Vec::new();
    let dev = ctx.home.join("Library/Developer");
    for (p, m) in scanner::children(&dev.join("Xcode/DerivedData")) {
        items.push(ctx.item(cat, &p, &m));
    }
    let mut support = scanner::children(&dev.join("Xcode/iOS DeviceSupport"));
    support.retain(|(_, m)| m.is_dir);
    support.sort_by_key(|(_, m)| m.mtime);
    if !support.is_empty() {
        for (p, m) in &support[..support.len() - 1] {
            items.push(ctx.item(cat, p, m));
        }
    }
    items.extend(ctx.item_at(cat, &dev.join("CoreSimulator/Caches")));
    for (p, m) in scanner::children(&ctx.home.join(".android/avd")) {
        if m.is_dir && ctx.now - m.mtime > rules::AVD_MAX_AGE {
            let mut it = ctx.item(cat, &p, &m);
            it.flags.push(flags::REVIEW.into());
            items.push(it);
        }
    }
    items.extend(ctx.item_at(cat, &ctx.home.join(".gradle/daemon")));
    items
}

fn old_files_under(ctx: &Ctx, cat: &str, root: &Path, max_age: i64, ext: Option<&str>) -> Vec<Item> {
    let home = Arc::new(ctx.home.clone());
    let mut out = Vec::new();
    scanner::walk(
        root,
        &ctx.progress,
        Arc::new(move |d| {
            if rules::is_denied(d, &home) || rules::is_memory_dir(d, &home) {
                Prune::Skip
            } else {
                Prune::Descend
            }
        }),
        |p, m| {
            if m.is_dir || ctx.now - m.mtime <= max_age {
                return;
            }
            if let Some(ext) = ext {
                if p.extension().and_then(|e| e.to_str()) != Some(ext) {
                    return;
                }
            }
            if rules::is_denied(p, &ctx.home) {
                return;
            }
            out.push(ctx.item(cat, p, m));
        },
    );
    out
}

fn ai_sessions(ctx: &Ctx) -> Vec<Item> {
    let cat = "ai_sessions";
    let mut items = old_files_under(
        ctx,
        cat,
        &ctx.home.join(".claude/projects"),
        rules::AI_SESSION_MAX_AGE,
        Some("jsonl"),
    );
    items.extend(old_files_under(
        ctx,
        cat,
        &ctx.home.join(".codex/sessions"),
        rules::AI_SESSION_MAX_AGE,
        None,
    ));
    items.extend(old_files_under(ctx, cat, &ctx.home.join(".cursor/logs"), rules::AI_SESSION_MAX_AGE, None));
    items.extend(old_files_under(
        ctx,
        cat,
        &ctx.home.join("Library/Application Support/Cursor/logs"),
        rules::AI_SESSION_MAX_AGE,
        None,
    ));
    for it in &mut items {
        it.flags.push(flags::REVIEW.into());
    }
    items
}

/// Nearest ancestor (stopping at home) containing `.git`.
fn owning_repo(path: &Path, home: &Path) -> Option<PathBuf> {
    let mut cur = path.parent();
    while let Some(d) = cur {
        if d == home || !d.starts_with(home) {
            return None;
        }
        if d.join(".git").exists() {
            return Some(d.to_path_buf());
        }
        cur = d.parent();
    }
    None
}

/// Last time the repo was worked on: its newest commit or, for uncommitted work, the last
/// index write (editors refresh it on every `git status`). `None` when git cannot say.
fn repo_last_active(repo: &str) -> Option<i64> {
    let commit: i64 = run_cmd("git", &["-C", repo, "log", "-1", "--format=%ct"])?.trim().parse().ok()?;
    let index = scanner::lstat(&Path::new(repo).join(".git/index")).map_or(commit, |m| m.mtime);
    Some(commit.max(index))
}

/// A repo whose activity git cannot report counts as recent.
fn is_recent(last_active: Option<i64>, now: i64, window: i64) -> bool {
    window > 0 && last_active.is_none_or(|t| now - t < window)
}

/// True when git tracks anything under `path`, or when git cannot answer.
fn git_tracked(repo: &str, path: &Path) -> bool {
    let rel = path.strip_prefix(repo).unwrap_or(path).to_string_lossy().into_owned();
    run_cmd("git", &["-C", repo, "ls-files", "--", &rel]).is_none_or(|s| !s.trim().is_empty())
}

struct DeepWalk {
    candidates: Vec<PathBuf>,
    repos: Vec<PathBuf>,
}

fn deep_walk(ctx: &Ctx) -> DeepWalk {
    let home = Arc::new(ctx.home.clone());
    let found: Arc<Mutex<DeepWalk>> = Arc::new(Mutex::new(DeepWalk { candidates: vec![], repos: vec![] }));
    let found2 = found.clone();
    scanner::walk(
        &ctx.home,
        &ctx.progress,
        Arc::new(move |d| {
            let name = d.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == ".git" {
                if let (Some(parent), Ok(mut f)) = (d.parent(), found2.lock()) {
                    f.repos.push(parent.to_path_buf());
                }
                return Prune::Skip;
            }
            if d.parent() == Some(home.as_path()) && (name.starts_with('.') || name == "Library") {
                return Prune::Skip;
            }
            if rules::is_denied(d, &home) {
                return Prune::Skip;
            }
            if rules::is_artifact_dir(d, || owning_repo(d, &home).is_some()) {
                if let Ok(mut f) = found2.lock() {
                    f.candidates.push(d.to_path_buf());
                }
                return Prune::Skip;
            }
            Prune::Descend
        }),
        |_, _| {},
    );
    let mut out = Arc::try_unwrap(found)
        .map(|m| m.into_inner().unwrap_or_else(|e| e.into_inner()))
        .unwrap_or_else(|arc| {
            let g = arc.lock().unwrap_or_else(|e| e.into_inner());
            DeepWalk { candidates: g.candidates.clone(), repos: g.repos.clone() }
        });
    out.candidates.sort();
    out.repos.sort();
    out.repos.dedup();
    out
}

struct Candidate {
    path: PathBuf,
    meta: Meta,
    repo: Option<String>,
    last_active: Option<i64>,
    flags: Vec<String>,
}

/// Artefacts in recently active repos and anything git tracks stay listed, flagged so they
/// cannot be selected, rather than vanishing from the results.
fn project_artifacts(ctx: &Ctx, walk: &DeepWalk) -> Vec<Item> {
    let cat = "project_artifacts";
    let recent_window = ctx.recent_days as i64 * DAY;
    let mut activity: HashMap<String, Option<i64>> = HashMap::new();
    let mut kept = Vec::new();
    for c in &walk.candidates {
        let Some(meta) = scanner::lstat(c) else { continue };
        let repo = owning_repo(c, &ctx.home).map(|r| r.to_string_lossy().into_owned());
        let mut candidate = Candidate { path: c.clone(), meta, repo, last_active: None, flags: vec![] };
        if let Some(r) = &candidate.repo {
            candidate.last_active = *activity.entry(r.clone()).or_insert_with(|| repo_last_active(r));
            if is_recent(candidate.last_active, ctx.now, recent_window) {
                candidate.flags.push(flags::RECENT.into());
            }
            if git_tracked(r, c) {
                candidate.flags.push(flags::GIT_TRACKED.into());
            }
        }
        kept.push(candidate);
    }
    let progress = ctx.progress.clone();
    let sizes: Vec<u64> = kept.par_iter().map(|c| scanner::size_of(&c.path, &progress)).collect();
    kept.into_iter()
        .zip(sizes)
        .map(|(c, bytes)| Item {
            id: ctx.next_id(cat),
            path: c.path.to_string_lossy().into_owned(),
            bytes,
            mtime: c.meta.mtime,
            kind: Kind::Dir,
            repo: c.repo,
            repo_last_active: c.last_active,
            last_used: None,
            flags: c.flags,
            apply: vec![],
        })
        .collect()
}

fn category_items(ctx: &Ctx, id: &str, deep: Option<&DeepWalk>) -> Vec<Item> {
    match id {
        "app_caches" => app_caches(ctx),
        "trash" => trash(ctx),
        "installers" => installers(ctx),
        "homebrew" => homebrew(ctx),
        "docker" => docker(ctx),
        "pkg_caches" => pkg_caches(ctx),
        "xcode_android" => xcode_android(ctx),
        "ai_sessions" => ai_sessions(ctx),
        "project_artifacts" => deep.map(|w| project_artifacts(ctx, w)).unwrap_or_default(),
        "worktrees" => deep
            .map(|w| worktrees::scan(&w.repos, ctx.now, &ctx.progress, || ctx.next_id("worktrees")))
            .unwrap_or_default(),
        _ => vec![],
    }
}

pub fn run_scan(mode: Mode, recent_days: u32, progress: Arc<Progress>) -> ScanResult {
    let ctx = Ctx::new(rules::home(), mode, recent_days, progress);
    let deep = if mode >= Mode::Deep { Some(deep_walk(&ctx)) } else { None };
    let mut categories = Vec::new();
    for def in rules::CATEGORIES {
        if !rules::category_enabled(def.id, mode) {
            continue;
        }
        ctx.progress.note(def.label);
        let items = category_items(&ctx, def.id, deep.as_ref());
        let total_bytes = items.iter().map(|i| i.bytes).sum();
        let items: Vec<Item> = items.into_iter().filter(|i| i.bytes > 0).collect();
        categories.push(Category {
            id: def.id.to_string(),
            label: def.label.to_string(),
            mode: def.mode,
            total_bytes,
            items,
        });
    }
    ctx.progress.flush();
    let (disk_total, disk_free) = scanner::disk_usage(&ctx.home);
    ScanResult { id: format!("scan-{}-{}", mode.as_str(), ctx.now), disk_total, disk_free, categories }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_sizes() {
        assert_eq!(parse_human_size("0B"), 0);
        assert_eq!(parse_human_size("1KB"), 1024);
        assert_eq!(parse_human_size("5.6kB"), (5.6 * 1024.0) as u64);
        assert_eq!(parse_human_size("1.5GB"), (1.5 * 1024f64.powi(3)) as u64);
    }

    #[test]
    fn brew_and_docker_parsers() {
        let brew = "Would remove: /opt/homebrew/Cellar/x (12.3MB)\nWould remove: /y (1KB)\n==> This operation would free approximately 2.5GB of disk space.\n";
        assert_eq!(parse_brew_cleanup(brew), (2.5 * 1024f64.powi(3)) as u64);
        let brew2 = "Would remove: /a (1KB)\nWould remove: /b (1KB)\n";
        assert_eq!(parse_brew_cleanup(brew2), 2048);
        let docker = "{\"Type\":\"Images\",\"Reclaimable\":\"1GB (50%)\"}\n{\"Type\":\"Build Cache\",\"Reclaimable\":\"512MB\"}\n";
        assert_eq!(parse_docker_df(docker), 1024u64.pow(3) + 512 * 1024 * 1024);
    }

    #[test]
    fn owning_repo_stops_at_home() {
        let td = tempfile::tempdir().unwrap();
        let home = td.path();
        std::fs::create_dir_all(home.join(".git")).unwrap();
        std::fs::create_dir_all(home.join("code/app/.git")).unwrap();
        std::fs::create_dir_all(home.join("code/app/web/node_modules")).unwrap();
        std::fs::create_dir_all(home.join("loose/node_modules")).unwrap();
        assert_eq!(owning_repo(&home.join("code/app/web/node_modules"), home), Some(home.join("code/app")));
        assert_eq!(owning_repo(&home.join("loose/node_modules"), home), None);
    }

    fn ctx(home: &Path, mode: Mode, recent_days: u32) -> Ctx {
        Ctx::new(home.to_path_buf(), mode, recent_days, Progress::noop())
    }

    fn git(dir: &Path, args: &[&str]) {
        let ok = std::process::Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    }

    #[test]
    fn docker_acts_only_on_a_local_socket() {
        assert!(is_local_docker_endpoint("unix:///Users/t/.orbstack/run/docker.sock"));
        assert!(is_local_docker_endpoint("unix:///var/run/docker.sock"));
        assert!(!is_local_docker_endpoint("tcp://10.0.0.5:2376"));
        assert!(!is_local_docker_endpoint("ssh://deploy@prod.example.com"));
        assert!(!is_local_docker_endpoint("unix://relative.sock"));
        assert!(!is_local_docker_endpoint(""));
    }

    #[test]
    fn docker_df_counts_only_what_is_pruned() {
        let df = "{\"Type\":\"Images\",\"Reclaimable\":\"1GB (50%)\"}\n{\"Type\":\"Containers\",\"Reclaimable\":\"2GB (90%)\"}\n{\"Type\":\"Local Volumes\",\"Reclaimable\":\"5GB (100%)\"}\n{\"Type\":\"Build Cache\",\"Reclaimable\":\"1GB\"}\n";
        assert_eq!(parse_docker_df(df), 2 * 1024u64.pow(3));
    }

    #[test]
    fn git_failure_fails_closed() {
        let td = tempfile::tempdir().unwrap();
        let not_a_repo = td.path().to_string_lossy().into_owned();
        std::fs::create_dir_all(td.path().join("dist")).unwrap();
        assert!(git_tracked(&not_a_repo, &td.path().join("dist")), "unknown counts as tracked");
        assert_eq!(repo_last_active(&not_a_repo), None);
        assert!(is_recent(None, 1_000_000, 14 * DAY), "unknown activity counts as recent");
        assert!(!is_recent(None, 1_000_000, 0), "Full mode has no recency window");
        assert!(!is_recent(Some(0), 1_000_000, 10));
    }

    #[test]
    fn deep_walk_skips_repos_and_loose_folders_named_like_artifacts() {
        let td = tempfile::tempdir().unwrap();
        let home = td.path();
        std::fs::create_dir_all(home.join("code/dist/.git")).unwrap();
        std::fs::create_dir_all(home.join("code/dist/node_modules")).unwrap();
        std::fs::write(home.join("code/dist/package.json"), "{}").unwrap();
        std::fs::create_dir_all(home.join("Documents/build")).unwrap();
        std::fs::write(home.join("Documents/build/report.key"), "slides").unwrap();
        std::fs::create_dir_all(home.join("code/app/.git")).unwrap();
        std::fs::create_dir_all(home.join("code/app/web/coverage")).unwrap();
        let walk = deep_walk(&ctx(home, Mode::Deep, 14));
        assert_eq!(
            walk.candidates,
            vec![home.join("code/app/web/coverage"), home.join("code/dist/node_modules")]
        );
        assert_eq!(walk.repos, vec![home.join("code/app"), home.join("code/dist")]);
    }

    #[test]
    fn recent_and_tracked_artifacts_are_listed_but_flagged() {
        let td = tempfile::tempdir().unwrap();
        let home = td.path();
        let repo = home.join("code/app");
        std::fs::create_dir_all(repo.join("node_modules/x")).unwrap();
        std::fs::create_dir_all(repo.join("dist")).unwrap();
        std::fs::write(repo.join("dist/index.js"), "tracked").unwrap();
        std::fs::write(repo.join(".gitignore"), "node_modules/\n").unwrap();
        git(&repo, &["init", "-q"]);
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-qm", "init"]);
        let flags_of = |mode: Mode| -> Vec<(String, Vec<String>)> {
            let c = ctx(home, mode, 14);
            let walk = deep_walk(&c);
            project_artifacts(&c, &walk)
                .into_iter()
                .map(|i| (i.path.rsplit('/').next().unwrap_or("").to_string(), i.flags))
                .collect()
        };
        assert_eq!(
            flags_of(Mode::Deep),
            vec![
                ("dist".to_string(), vec![flags::RECENT.to_string(), flags::GIT_TRACKED.to_string()]),
                ("node_modules".to_string(), vec![flags::RECENT.to_string()]),
            ]
        );
        assert_eq!(
            flags_of(Mode::Full),
            vec![
                ("dist".to_string(), vec![flags::GIT_TRACKED.to_string()]),
                ("node_modules".to_string(), vec![]),
            ]
        );
    }

    #[test]
    fn no_path_is_listed_in_two_categories() {
        let td = tempfile::tempdir().unwrap();
        let home = td.path();
        for dir in [
            "Library/Caches/pip/http",
            "Library/Caches/ms-playwright/chromium-1140",
            "Library/Caches/com.microsoft.VSCode.ShipIt/update",
            "Library/Caches/com.spotify.client/Data",
            "Library/Logs/SomeApp",
            ".cache/pip/wheels",
            ".npm/_cacache/index-v5",
            "Library/Developer/Xcode/DerivedData/App-abc",
            ".gradle/caches/modules-2",
            ".gradle/daemon/8.5",
            ".Trash/old",
        ] {
            std::fs::create_dir_all(home.join(dir)).unwrap();
            std::fs::write(home.join(dir).join("blob"), "x").unwrap();
        }
        let repo = home.join("code/app");
        std::fs::create_dir_all(repo.join("node_modules/x")).unwrap();
        std::fs::write(repo.join("node_modules/x/index.js"), "x").unwrap();
        git(&repo, &["init", "-q"]);

        // Brew and Docker propose commands, not paths, and read this machine's state.
        let path_categories = || rules::CATEGORIES.iter().filter(|c| !matches!(c.id, "homebrew" | "docker"));
        for mode in [Mode::Standard, Mode::Full] {
            let c = ctx(home, mode, 14);
            let deep = deep_walk(&c);
            let mut owner: Vec<(PathBuf, &str)> = Vec::new();
            for def in path_categories().filter(|d| rules::category_enabled(d.id, mode)) {
                for item in category_items(&c, def.id, Some(&deep)) {
                    owner.push((PathBuf::from(&item.path), def.id));
                }
            }
            for (a, cat_a) in &owner {
                for (b, cat_b) in &owner {
                    assert!(
                        cat_a == cat_b || !b.starts_with(a),
                        "{} is in {cat_a} and {} in {cat_b}",
                        a.display(),
                        b.display()
                    );
                }
            }
            let in_cat = |cat: &str, rel: &str| {
                owner.iter().filter(|(p, c)| *c == cat && *p == home.join(rel)).count()
            };
            for rel in [
                "Library/Caches/pip",
                "Library/Caches/ms-playwright",
                "Library/Caches/com.microsoft.VSCode.ShipIt",
            ] {
                assert_eq!(in_cat("pkg_caches", rel), 1, "{rel} once, as a package-manager cache");
                assert_eq!(in_cat("app_caches", rel), 0, "{rel} not as an app cache");
            }
            assert_eq!(in_cat("app_caches", "Library/Caches/com.spotify.client"), 1);
        }
        assert!(!rules::category_enabled("pkg_caches", Mode::Light), "and so Light offers none of them");
    }

    #[test]
    fn ai_sessions_need_review_and_start_at_deep() {
        assert!(!rules::category_enabled("ai_sessions", Mode::Standard));
        assert!(rules::category_enabled("ai_sessions", Mode::Deep));
        let td = tempfile::tempdir().unwrap();
        let home = td.path();
        let log = home.join(".claude/projects/-Users-t-app/session.jsonl");
        std::fs::create_dir_all(log.parent().unwrap()).unwrap();
        std::fs::write(&log, "{}").unwrap();
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(60 * DAY as u64);
        std::fs::File::options().write(true).open(&log).unwrap().set_modified(old).unwrap();
        let items = ai_sessions(&ctx(home, Mode::Deep, 14));
        assert_eq!(items.len(), 1);
        assert!(items[0].has_flag(flags::REVIEW));
        assert!(items[0].selectable(), "chosen by hand, never preselected");
    }
}
