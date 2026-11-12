//! Parallel filesystem walking (jwalk). Sizes are allocated blocks, symlinks are
//! never followed, and the walk never crosses onto another volume.

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jwalk::WalkDirGeneric;

use crate::types::ScanProgress;

pub type ProgressSink = Arc<dyn Fn(ScanProgress) + Send + Sync>;

const EMIT_INTERVAL: Duration = Duration::from_millis(200);

pub struct Progress {
    scanned: AtomicU64,
    bytes: AtomicU64,
    current: Mutex<String>,
    last_emit: Mutex<Instant>,
    sink: Option<ProgressSink>,
}

impl Progress {
    pub fn new(sink: ProgressSink) -> Arc<Self> {
        Arc::new(Self {
            scanned: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
            current: Mutex::new(String::new()),
            last_emit: Mutex::new(Instant::now()),
            sink: Some(sink),
        })
    }

    pub fn noop() -> Arc<Self> {
        Arc::new(Self {
            scanned: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
            current: Mutex::new(String::new()),
            last_emit: Mutex::new(Instant::now()),
            sink: None,
        })
    }

    pub fn tick(&self, path: &Path, bytes: u64) {
        self.scanned.fetch_add(1, Ordering::Relaxed);
        self.bytes.fetch_add(bytes, Ordering::Relaxed);
        let Some(sink) = &self.sink else { return };
        let Ok(mut last) = self.last_emit.try_lock() else { return };
        if last.elapsed() < EMIT_INTERVAL {
            return;
        }
        *last = Instant::now();
        let current = path.to_string_lossy().into_owned();
        if let Ok(mut c) = self.current.lock() {
            *c = current.clone();
        }
        sink(ScanProgress {
            scanned: self.scanned.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
            current,
        });
    }

    pub fn note(&self, label: &str) {
        self.tick(Path::new(label), 0);
    }

    pub fn flush(&self) {
        if let Some(sink) = &self.sink {
            let current = self.current.lock().map(|c| c.clone()).unwrap_or_default();
            sink(ScanProgress {
                scanned: self.scanned.load(Ordering::Relaxed),
                bytes: self.bytes.load(Ordering::Relaxed),
                current,
            });
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Meta {
    pub bytes: u64,
    pub mtime: i64,
    pub atime: i64,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub dev: u64,
}

/// lstat: never follows symlinks. Bytes are allocated blocks * 512.
pub fn lstat(path: &Path) -> Option<Meta> {
    let m = std::fs::symlink_metadata(path).ok()?;
    Some(Meta {
        bytes: m.blocks() * 512,
        mtime: m.mtime(),
        atime: m.atime(),
        is_dir: m.is_dir(),
        is_symlink: m.file_type().is_symlink(),
        dev: m.dev(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prune {
    /// Yield the directory and walk its children.
    Descend,
    /// Yield the directory but do not walk into it.
    Stop,
    /// Neither yield nor walk.
    Skip,
}

pub type PruneFn = Arc<dyn Fn(&Path) -> Prune + Send + Sync>;

/// Walk `root`, calling `prune` for each child directory and `visit` for every
/// yielded entry (files and directories, root excluded). Symlinks are skipped,
/// and directories on another device than `root` are skipped.
pub fn walk<F>(root: &Path, progress: &Progress, prune: PruneFn, mut visit: F)
where
    F: FnMut(&Path, &Meta),
{
    let Some(root_meta) = lstat(root) else { return };
    if !root_meta.is_dir || root_meta.is_symlink {
        return;
    }
    let root_dev = root_meta.dev;
    let walker = WalkDirGeneric::<((), ())>::new(root)
        .follow_links(false)
        .skip_hidden(false)
        .process_read_dir(move |_depth, _dir, _state, children| {
            let mut keep = Vec::with_capacity(children.len());
            for child in children.iter_mut() {
                let k = match child {
                    Ok(e) if e.file_type.is_symlink() => false,
                    Ok(e) if e.file_type.is_dir() => {
                        let p = e.path();
                        match lstat(&p) {
                            Some(m) if m.dev == root_dev && !m.is_symlink => match prune(&p) {
                                Prune::Descend => true,
                                Prune::Stop => {
                                    e.read_children_path = None;
                                    true
                                }
                                Prune::Skip => false,
                            },
                            _ => false,
                        }
                    }
                    Ok(_) => true,
                    Err(_) => false,
                };
                keep.push(k);
            }
            let mut i = 0;
            children.retain(|_| {
                let k = keep[i];
                i += 1;
                k
            });
        });
    for entry in walker.into_iter().flatten() {
        if entry.depth == 0 {
            continue;
        }
        let path = entry.path();
        let Some(meta) = lstat(&path) else { continue };
        if meta.is_symlink {
            continue;
        }
        progress.tick(&path, if meta.is_dir { 0 } else { meta.bytes });
        visit(&path, &meta);
    }
}

/// Total allocated bytes under `path` (file or directory). Directory entries' own blocks count.
pub fn size_of(path: &Path, progress: &Progress) -> u64 {
    let Some(meta) = lstat(path) else { return 0 };
    if meta.is_symlink {
        return 0;
    }
    if !meta.is_dir {
        progress.tick(path, meta.bytes);
        return meta.bytes;
    }
    let mut total = meta.bytes;
    walk(path, progress, Arc::new(|_| Prune::Descend), |_, m| total += m.bytes);
    total
}

/// Direct children of a directory (non-symlink), with metadata.
pub fn children(dir: &Path) -> Vec<(PathBuf, Meta)> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<(PathBuf, Meta)> = rd
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let m = lstat(&p)?;
            if m.is_symlink {
                return None;
            }
            Some((p, m))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// `(total, free)` bytes of the volume holding `path`, via `df -k`.
pub fn disk_usage(path: &Path) -> (u64, u64) {
    let out = std::process::Command::new("df").arg("-k").arg(path).output();
    let Ok(out) = out else { return (0, 0) };
    let text = String::from_utf8_lossy(&out.stdout);
    let Some(line) = text.lines().nth(1) else { return (0, 0) };
    let cols: Vec<&str> = line.split_whitespace().collect();
    if cols.len() < 4 {
        return (0, 0);
    }
    let total = cols[1].parse::<u64>().unwrap_or(0) * 1024;
    let free = cols[3].parse::<u64>().unwrap_or(0) * 1024;
    (total, free)
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_skips_symlinks_and_counts_blocks() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path().join("r");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("a.bin"), vec![0u8; 100_000]).unwrap();
        std::fs::write(root.join("sub/b.bin"), vec![1u8; 50_000]).unwrap();
        std::os::unix::fs::symlink(root.join("a.bin"), root.join("link")).unwrap();
        let p = Progress::noop();
        let size = size_of(&root, &p);
        assert!(size >= 150_000, "size {size}");
        assert!(size < 300_000, "size {size}");
    }

    #[test]
    fn walk_prunes() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path();
        std::fs::create_dir_all(root.join("keep/x")).unwrap();
        std::fs::create_dir_all(root.join("stop/y")).unwrap();
        std::fs::create_dir_all(root.join("skip/z")).unwrap();
        let mut seen = Vec::new();
        let p = Progress::noop();
        walk(
            root,
            &p,
            Arc::new(|d: &Path| match d.file_name().and_then(|n| n.to_str()) {
                Some("stop") => Prune::Stop,
                Some("skip") => Prune::Skip,
                _ => Prune::Descend,
            }),
            |path, _| seen.push(path.strip_prefix(root).unwrap().to_string_lossy().into_owned()),
        );
        seen.sort();
        assert_eq!(seen, vec!["keep", "keep/x", "stop"]);
    }
}
