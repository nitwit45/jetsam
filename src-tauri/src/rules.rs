//! Category definitions, the deny-list and the pure matching predicates every scan and
//! every delete consult. The only I/O here is the look at a candidate directory and its
//! siblings that decides whether it is really a build artefact.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::types::Mode;

pub const DAY: i64 = 86_400;
pub const INSTALLER_MAX_AGE: i64 = 14 * DAY;
pub const AI_SESSION_MAX_AGE: i64 = 45 * DAY;
pub const AVD_MAX_AGE: i64 = 60 * DAY;
pub const DEFAULT_RECENT_DAYS: u32 = 14;

pub fn home() -> PathBuf {
    dirs::home_dir()
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Absolute roots that are never a target and never descended into: credentials,
/// mail and messages, iCloud and device backups, and the OS itself.
pub fn deny_roots(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join("Library/Keychains"),
        home.join("Library/Mail"),
        home.join("Library/Messages"),
        home.join("Library/Mobile Documents"),
        home.join("Library/Application Support/MobileSync"),
        home.join(".ssh"),
        home.join(".gnupg"),
        home.join(".aws"),
        home.join(".kube"),
        home.join(".docker"),
        PathBuf::from("/System"),
        PathBuf::from("/usr"),
        PathBuf::from("/bin"),
        PathBuf::from("/sbin"),
        PathBuf::from("/private/var/db"),
    ]
}

/// Roots that may be scanned beneath but can never themselves be a delete target.
pub fn protected_roots(home: &Path) -> Vec<PathBuf> {
    vec![
        PathBuf::from("/"),
        PathBuf::from("/Users"),
        PathBuf::from("/Applications"),
        PathBuf::from("/Library"),
        PathBuf::from("/private"),
        PathBuf::from("/private/var"),
        PathBuf::from("/private/tmp"),
        PathBuf::from("/Volumes"),
        home.to_path_buf(),
        home.join("Library"),
        home.join("Library/Caches"),
        home.join("Library/Logs"),
        home.join("Library/Application Support"),
        home.join("Library/Developer"),
        home.join("Desktop"),
        home.join("Documents"),
        home.join("Downloads"),
        home.join("Pictures"),
        home.join("Movies"),
        home.join("Music"),
        home.join(".Trash"),
        home.join(".claude"),
        home.join(".claude/projects"),
        home.join(".codex"),
        home.join(".cursor"),
    ]
}

pub fn is_protected_root(path: &Path, home: &Path) -> bool {
    protected_roots(home).iter().any(|r| r == path)
}

/// Hard refusal. Applies in every mode, including permanent deletes.
pub fn is_denied(path: &Path, home: &Path) -> bool {
    if deny_roots(home).iter().any(|r| path.starts_with(r)) {
        return true;
    }
    let comps: Vec<&OsStr> = path.iter().collect();
    for c in &comps {
        if *c == OsStr::new(".git") {
            return true;
        }
        if c.to_string_lossy().ends_with(".photoslibrary") {
            return true;
        }
    }
    if let Ok(rel) = path.strip_prefix(home.join(".claude/projects")) {
        let rel: Vec<&OsStr> = rel.iter().collect();
        if rel.len() >= 2 && rel[1] == OsStr::new("memory") {
            return true;
        }
    }
    false
}

/// Directory names a filesystem walk must never descend into, beyond the deny-list.
pub fn is_memory_dir(path: &Path, home: &Path) -> bool {
    path.starts_with(home.join(".claude/projects")) && path.file_name() == Some(OsStr::new("memory"))
}

pub struct CategoryDef {
    pub id: &'static str,
    pub label: &'static str,
    pub mode: Mode,
}

pub const CATEGORIES: &[CategoryDef] = &[
    CategoryDef { id: "app_caches", label: "App caches & logs", mode: Mode::Light },
    CategoryDef { id: "trash", label: "Trash", mode: Mode::Light },
    CategoryDef { id: "installers", label: "Old installers", mode: Mode::Light },
    CategoryDef { id: "homebrew", label: "Homebrew", mode: Mode::Light },
    CategoryDef { id: "docker", label: "Docker", mode: Mode::Light },
    CategoryDef { id: "pkg_caches", label: "Package manager caches", mode: Mode::Standard },
    CategoryDef { id: "xcode_android", label: "Xcode & Android", mode: Mode::Standard },
    CategoryDef { id: "ai_sessions", label: "AI session logs", mode: Mode::Deep },
    CategoryDef { id: "project_artifacts", label: "Project build artifacts", mode: Mode::Deep },
    CategoryDef { id: "worktrees", label: "Stale worktrees", mode: Mode::Deep },
];

pub fn category(id: &str) -> Option<&'static CategoryDef> {
    CATEGORIES.iter().find(|c| c.id == id)
}

pub fn category_enabled(id: &str, mode: Mode) -> bool {
    category(id).map(|c| c.mode <= mode).unwrap_or(false)
}

/// `~/Library/Caches/<name>`: skip CloudKit and Apple-owned caches except helpd.
pub fn app_cache_skip(name: &str) -> bool {
    name == "CloudKit" || (name.starts_with("com.apple.") && name != "com.apple.helpd")
}

pub fn is_installer(name: &str, mtime: i64, now: i64) -> bool {
    let lower = name.to_ascii_lowercase();
    let ext_ok = lower.ends_with(".dmg") || lower.ends_with(".pkg");
    ext_ok && now - mtime > INSTALLER_MAX_AGE
}

/// `(path, report_only)` for the pkg_caches category.
pub fn pkg_cache_paths(home: &Path) -> Vec<(PathBuf, bool)> {
    vec![
        (home.join(".npm/_cacache"), false),
        (home.join("Library/pnpm/store"), false),
        (home.join(".bun/install/cache"), false),
        (home.join("Library/Caches/pip"), false),
        (home.join(".cache/pip"), false),
        (home.join(".cargo/registry/cache"), false),
        (home.join(".gradle/caches"), false),
        (home.join(".nuget/packages"), true),
        (home.join("Library/Caches/ms-playwright"), false),
        (home.join("Library/Caches/com.microsoft.VSCode.ShipIt"), false),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactRule {
    /// Unambiguous build output, but only inside a repo or next to a project manifest:
    /// `build` or `dist` anywhere else is just as likely a folder of someone's work.
    Always,
    NeedsCargoToml,
    NeedsDotnetProject,
}

const MANIFEST_FILES: [&str; 11] = [
    "package.json",
    "pyproject.toml",
    "setup.py",
    "requirements.txt",
    "Cargo.toml",
    "go.mod",
    "pom.xml",
    "Gemfile",
    "composer.json",
    "Package.swift",
    "build.gradle",
];

pub fn artifact_rule(name: &str) -> Option<ArtifactRule> {
    Some(match name {
        "node_modules" | ".next" | ".nuxt" | "dist" | "build" | ".turbo" | "__pycache__" | ".venv"
        | "venv" | ".pytest_cache" | ".gradle" | "coverage" | ".parcel-cache" | ".cache" => {
            ArtifactRule::Always
        }
        "target" => ArtifactRule::NeedsCargoToml,
        "bin" | "obj" => ArtifactRule::NeedsDotnetProject,
        _ => return None,
    })
}

fn is_dotnet_project_file(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.ends_with(".csproj") || n.ends_with(".sln") || n.ends_with(".fsproj")
}

fn is_manifest_file(name: &str) -> bool {
    MANIFEST_FILES.contains(&name) || name.starts_with("build.gradle") || is_dotnet_project_file(name)
}

fn any_sibling(dir: &Path, matches: fn(&str) -> bool) -> bool {
    std::fs::read_dir(dir)
        .map(|rd| rd.flatten().any(|e| e.file_name().to_str().is_some_and(matches)))
        .unwrap_or(false)
}

/// True when `dir` holds a `.git` file or directory, which makes it a repository or a
/// worktree rather than an artefact. An unreadable entry counts as present.
pub fn contains_git_entry(dir: &Path) -> bool {
    match std::fs::symlink_metadata(dir.join(".git")) {
        Ok(_) => true,
        Err(e) => e.kind() != std::io::ErrorKind::NotFound,
    }
}

/// True when `dir` is a build-artifact directory per its name and sibling context.
/// `in_repo` reports whether a parent directory is a git repository; it only runs for names
/// that need it, because the walk calls this for every directory under home.
pub fn is_artifact_dir(dir: &Path, in_repo: impl FnOnce() -> bool) -> bool {
    let Some(name) = dir.file_name().and_then(|n| n.to_str()) else { return false };
    let Some(rule) = artifact_rule(name) else { return false };
    let Some(parent) = dir.parent() else { return false };
    if contains_git_entry(dir) {
        return false;
    }
    match rule {
        ArtifactRule::Always => any_sibling(parent, is_manifest_file) || in_repo(),
        ArtifactRule::NeedsCargoToml => parent.join("Cargo.toml").is_file(),
        ArtifactRule::NeedsDotnetProject => any_sibling(parent, is_dotnet_project_file),
    }
}

pub fn aged_hint(path: &Path) -> &'static str {
    let ext = path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "mp4" | "mov" | "mkv" | "avi" | "m4v" | "webm" | "wmv" | "flv" => "video",
        "zip" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "7z" | "rar" | "zst" => "archive",
        "dmg" | "iso" | "img" | "pkg" | "vdi" | "vmdk" | "qcow2" | "sparseimage" | "sparsebundle" => {
            "disk-image"
        }
        "gguf" | "safetensors" | "ckpt" | "pt" | "pth" | "bin" | "onnx" | "mlmodel" | "mlpackage" => "model",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h() -> PathBuf {
        PathBuf::from("/Users/t")
    }

    #[test]
    fn deny_list_roots() {
        let home = h();
        for p in [
            "/Users/t/Library/Keychains/login.keychain-db",
            "/Users/t/Library/Mail/V10",
            "/Users/t/Library/Mobile Documents/x",
            "/Users/t/.ssh/id_ed25519",
            "/Users/t/.gnupg",
            "/Users/t/.aws/credentials",
            "/Users/t/.kube/config",
            "/Users/t/.docker/config.json",
            "/Users/t/Library/Messages/chat.db",
            "/Users/t/Library/Application Support/MobileSync/Backup/abc",
            "/System/Library",
            "/usr/bin/ls",
            "/bin",
            "/sbin/x",
            "/private/var/db/dyld",
        ] {
            assert!(is_denied(Path::new(p), &home), "{p}");
        }
        for p in ["/Users/t/Library/Caches/foo", "/Users/t/Downloads/x.dmg", "/private/var/folders/x"] {
            assert!(!is_denied(Path::new(p), &home), "{p}");
        }
    }

    #[test]
    fn deny_list_patterns() {
        let home = h();
        assert!(is_denied(Path::new("/Users/t/code/app/.git"), &home));
        assert!(is_denied(Path::new("/Users/t/code/app/.git/objects/pack"), &home));
        assert!(!is_denied(Path::new("/Users/t/code/app/.github"), &home));
        assert!(!is_denied(Path::new("/Users/t/code/app/node_modules"), &home));
        assert!(is_denied(Path::new("/Users/t/Pictures/Photos Library.photoslibrary"), &home));
        assert!(is_denied(
            Path::new("/Users/t/Pictures/Photos Library.photoslibrary/originals/a.jpg"),
            &home
        ));
        assert!(is_denied(Path::new("/Users/t/.claude/projects/-Users-t/memory"), &home));
        assert!(is_denied(Path::new("/Users/t/.claude/projects/-Users-t/memory/MEMORY.md"), &home));
        assert!(!is_denied(Path::new("/Users/t/.claude/projects/-Users-t/abc.jsonl"), &home));
        assert!(!is_denied(Path::new("/Users/t/.claude/projects/-Users-t/memory.jsonl"), &home));
    }

    #[test]
    fn protected_roots_block_targets() {
        let home = h();
        assert!(is_protected_root(&home, &home));
        assert!(is_protected_root(Path::new("/"), &home));
        assert!(is_protected_root(Path::new("/Users/t/Library/Caches"), &home));
        assert!(!is_protected_root(Path::new("/Users/t/Library/Caches/foo"), &home));
    }

    #[test]
    fn category_mode_gating() {
        assert!(category_enabled("trash", Mode::Light));
        assert!(!category_enabled("pkg_caches", Mode::Light));
        assert!(category_enabled("pkg_caches", Mode::Standard));
        assert!(!category_enabled("project_artifacts", Mode::Standard));
        assert!(category_enabled("project_artifacts", Mode::Deep));
        assert!(category_enabled("worktrees", Mode::Full));
        assert!(!category_enabled("nope", Mode::Full));
    }

    #[test]
    fn app_cache_skip_rules() {
        assert!(app_cache_skip("CloudKit"));
        assert!(app_cache_skip("com.apple.Safari"));
        assert!(!app_cache_skip("com.apple.helpd"));
        assert!(!app_cache_skip("com.google.Chrome"));
    }

    #[test]
    fn installer_age() {
        let now = 1_000_000_000;
        assert!(is_installer("Foo.dmg", now - 15 * DAY, now));
        assert!(is_installer("foo.PKG", now - 15 * DAY, now));
        assert!(!is_installer("Foo.dmg", now - 13 * DAY, now));
        assert!(!is_installer("a.zip", now - 100 * DAY, now), "archives may be someone's only copy");
        assert!(!is_installer("a.txt", now - 100 * DAY, now));
    }

    #[test]
    fn artifact_sibling_rules() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path();
        std::fs::create_dir_all(root.join("a/node_modules")).unwrap();
        std::fs::create_dir_all(root.join("a/target")).unwrap();
        std::fs::create_dir_all(root.join("b/target")).unwrap();
        std::fs::write(root.join("b/Cargo.toml"), "").unwrap();
        std::fs::create_dir_all(root.join("c/bin")).unwrap();
        std::fs::create_dir_all(root.join("d/obj")).unwrap();
        std::fs::write(root.join("d/App.csproj"), "").unwrap();
        std::fs::write(root.join("a/package.json"), "{}").unwrap();
        assert!(is_artifact_dir(&root.join("a/node_modules"), || false));
        assert!(!is_artifact_dir(&root.join("a/target"), || true));
        assert!(is_artifact_dir(&root.join("b/target"), || false));
        assert!(!is_artifact_dir(&root.join("c/bin"), || true));
        assert!(is_artifact_dir(&root.join("d/obj"), || false));
        assert!(!is_artifact_dir(&root.join("a"), || true));
    }

    #[test]
    fn always_names_need_a_repo_or_a_manifest() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path();
        for name in ["build", "dist", "coverage", "venv", ".cache"] {
            let loose = root.join("Documents").join(name);
            std::fs::create_dir_all(&loose).unwrap();
            assert!(!is_artifact_dir(&loose, || false), "{name} with no repo and no manifest");
            assert!(is_artifact_dir(&loose, || true), "{name} inside a repo");
        }
        for manifest in ["pyproject.toml", "go.mod", "App.sln", "build.gradle.kts", "Package.swift"] {
            let proj = root.join(format!("proj-{manifest}"));
            std::fs::create_dir_all(proj.join("dist")).unwrap();
            std::fs::write(proj.join(manifest), "").unwrap();
            assert!(is_artifact_dir(&proj.join("dist"), || false), "dist next to {manifest}");
        }
    }

    #[test]
    fn a_candidate_holding_git_is_never_an_artifact() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path();
        std::fs::write(root.join("package.json"), "{}").unwrap();
        std::fs::create_dir_all(root.join("dist/.git")).unwrap();
        std::fs::create_dir_all(root.join("build")).unwrap();
        std::fs::write(root.join("build/.git"), "gitdir: /elsewhere").unwrap();
        std::fs::create_dir_all(root.join("coverage")).unwrap();
        assert!(contains_git_entry(&root.join("dist")));
        assert!(!is_artifact_dir(&root.join("dist"), || true), "its own clone");
        assert!(!is_artifact_dir(&root.join("build"), || true), "a worktree or submodule");
        assert!(is_artifact_dir(&root.join("coverage"), || true));
    }

    #[test]
    fn aged_hints() {
        assert_eq!(aged_hint(Path::new("/x/a.MOV")), "video");
        assert_eq!(aged_hint(Path::new("/x/a.tar.gz")), "archive");
        assert_eq!(aged_hint(Path::new("/x/a.dmg")), "disk-image");
        assert_eq!(aged_hint(Path::new("/x/a.gguf")), "model");
        assert_eq!(aged_hint(Path::new("/x/a")), "other");
    }
}
