//! RAM reclaim: a 10s process sampler, reason tagging (protected, orphan, idle, leak,
//! headless, vm), ending orphans plus a file-cache purge, and soft restarts of Chrome
//! and the editors.
//!
//! Claude Code agents, everything they start and every process hosting them (the
//! terminal, the editor's extension host) are protected: ending any of them ends an
//! agent's work. A process is only called an orphan when a sample saw it with a real
//! parent and it has since been handed to launchd; launchd's own jobs and anything
//! daemonised at birth never are. Virtual machines are only ever labelled; OrbStack
//! hands idle memory back to macOS on its own, so Reclaim never stops or signals a VM.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde_json::Value;

use crate::memory as memory_mod;
use crate::scanner::now;
use crate::system::{self, admin_shell, pgrep, run_cmd};
use crate::types::{FreshenResult, HistPoint, ProcRow, ReclaimAction, ReclaimResult};

pub const SAMPLE_INTERVAL: Duration = Duration::from_secs(10);
const MAX_SAMPLES: usize = 60;
const IDLE_WINDOW_SEC: i64 = 15 * 60;
const IDLE_MIN_SAMPLES: usize = 6;
const IDLE_CPU_PCT: f64 = 0.5;
const IDLE_MIN_RSS: u64 = 200 * 1024 * 1024;
const LEAK_MIN_GROWTH: u64 = 150 * 1024 * 1024;
const LEAK_MIN_RATIO: f64 = 1.3;
/// How long a process stays protected after its Claude agent exits, and how young a
/// process must be to be claimed by a Claude process group or session.
const CLAUDE_GRACE_SEC: i64 = 120;
/// `etime` is whole seconds, so a start time recomputed per sample drifts by one or two.
const START_SLACK_SEC: i64 = 2;
const TERM_GRACE: Duration = Duration::from_secs(3);
const SETTLE: Duration = Duration::from_secs(3);
const QUIT_WAIT: Duration = Duration::from_secs(20);

const ORPHAN_NAMES: [&str; 11] = [
    "node",
    "tsserver",
    "esbuild",
    "vite",
    "next-server",
    "bun",
    "python",
    "ruby",
    "java",
    "code helper",
    "cursor helper",
];
const PROTECTED_NAMES: [&str; 4] = ["WindowServer", "loginwindow", "kernel_task", "launchd"];
/// Executables macOS ships: ending one can take down the session or a system service.
const SYSTEM_PREFIXES: [&str; 7] =
    ["/System/", "/usr/libexec/", "/usr/sbin/", "/usr/bin/", "/sbin/", "/bin/", "/Library/Apple/"];
/// launchd labels of Apple's own agents and daemons.
const SYSTEM_JOB_PREFIX: &str = "com.apple.";
/// Apple's Virtualization.framework host process; OrbStack, UTM and Docker Desktop all use it.
const ORBSTACK_VM: &str = "com.apple.Virtualization.VirtualMachine";
const VM_MARKERS: [&str; 2] = [ORBSTACK_VM, "qemu"];
const CLAUDE_CODE_PACKAGE: &str = "@anthropic-ai/claude-code/";
/// The native installer keeps each release here and links `~/.local/bin/claude` to it.
const CLAUDE_VERSIONS_DIR: &str = "/.local/share/claude/versions/";

#[derive(Debug, Clone, PartialEq)]
pub struct RawProc {
    pub pid: u32,
    pub ppid: u32,
    pub pgid: u32,
    pub rss: u64,
    pub cpu_pct: f64,
    pub elapsed_sec: u64,
    pub comm: String,
    /// Command-line flags after the executable (from a second `ps ... args=` pass).
    pub args: String,
}

#[derive(Debug, Clone, Copy)]
pub struct Sample {
    pub ts: i64,
    pub rss: u64,
    pub cpu_pct: f64,
}

/// What the sampler knows about one live process across samples.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lineage {
    /// Sample time minus elapsed time; tells a reused pid apart from the original.
    pub started: i64,
    /// Some sample saw it with a parent other than launchd.
    pub had_parent: bool,
}

/// The Claude agent a process was last attributed to, and when.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClaudeMark {
    pub agent: u32,
    pub ts: i64,
}

#[derive(Default)]
pub struct Sampler {
    pub samples: HashMap<u32, Vec<Sample>>,
    pub history: Vec<HistPoint>,
    pub latest: Vec<RawProc>,
    pub lineage: HashMap<u32, Lineage>,
    /// pid -> its Claude agent. Kept for the grace window after the pid exits, so a
    /// short-lived agent shell can still vouch for a server it started in the background.
    pub claude_seen: HashMap<u32, ClaudeMark>,
    /// The last app other than Jetsam a sample saw in front. Clicking Reclaim brings
    /// Jetsam forward, so this is the app the user was actually working in.
    pub last_front: Option<LastFront>,
}

/// A frontmost app remembered across samples, with its main process's start time so a
/// reused pid is not protected in its place.
#[derive(Debug, Clone, PartialEq)]
pub struct LastFront {
    pub front: Frontmost,
    pub started: Option<i64>,
}

pub type SharedSampler = Arc<Mutex<Sampler>>;

fn lock(s: &SharedSampler) -> MutexGuard<'_, Sampler> {
    s.lock().unwrap_or_else(|e| e.into_inner())
}

// ---------- ps parsing ----------

/// `[[dd-]hh:]mm:ss` -> seconds.
pub fn parse_etime(s: &str) -> u64 {
    let (days, rest) = match s.split_once('-') {
        Some((d, r)) => (d.parse().unwrap_or(0u64), r),
        None => (0, s),
    };
    let parts: Vec<u64> = rest.split(':').map(|p| p.parse().unwrap_or(0)).collect();
    let secs = match parts.as_slice() {
        [h, m, s] => h * 3600 + m * 60 + s,
        [m, s] => m * 60 + s,
        [s] => *s,
        _ => 0,
    };
    days * 86400 + secs
}

fn take_token(s: &str) -> Option<(&str, &str)> {
    let s = s.trim_start();
    if s.is_empty() {
        return None;
    }
    let end = s.find(char::is_whitespace).unwrap_or(s.len());
    Some((&s[..end], &s[end..]))
}

/// One line of `ps -axo pid=,ppid=,pgid=,rss=,%cpu=,etime=,comm=`; comm may contain spaces.
pub fn parse_ps_line(line: &str) -> Option<RawProc> {
    let (pid, r) = take_token(line)?;
    let (ppid, r) = take_token(r)?;
    let (pgid, r) = take_token(r)?;
    let (rss, r) = take_token(r)?;
    let (cpu, r) = take_token(r)?;
    let (etime, r) = take_token(r)?;
    let comm = r.trim();
    if comm.is_empty() {
        return None;
    }
    Some(RawProc {
        pid: pid.parse().ok()?,
        ppid: ppid.parse().ok()?,
        pgid: pgid.parse().ok()?,
        rss: rss.parse::<u64>().ok()? * 1024,
        cpu_pct: cpu.parse().unwrap_or(0.0),
        elapsed_sec: parse_etime(etime),
        comm: comm.to_string(),
        args: String::new(),
    })
}

pub fn parse_ps(s: &str) -> Vec<RawProc> {
    s.lines().filter_map(parse_ps_line).collect()
}

/// Fill `args` from `ps -axwwo pid=,args=` output: everything after the executable path.
pub fn attach_args(procs: &mut [RawProc], ps_args: &str) {
    let by_pid: HashMap<u32, &str> = ps_args
        .lines()
        .filter_map(|l| {
            let (pid, rest) = take_token(l)?;
            Some((pid.parse().ok()?, rest.trim()))
        })
        .collect();
    for p in procs.iter_mut() {
        if let Some(full) = by_pid.get(&p.pid) {
            // argv[0] is usually the full path, sometimes just the basename; otherwise drop the first token.
            let rest = full
                .strip_prefix(p.comm.as_str())
                .or_else(|| full.strip_prefix(basename(&p.comm)))
                .or_else(|| take_token(full).map(|(_, r)| r))
                .unwrap_or("");
            p.args = rest.trim().to_string();
        }
    }
}

pub fn list_processes() -> Vec<RawProc> {
    let mut procs = run_cmd("ps", &["-axo", "pid=,ppid=,pgid=,rss=,%cpu=,etime=,comm="])
        .map(|s| parse_ps(&s))
        .unwrap_or_default();
    if let Some(args) = run_cmd("ps", &["-axwwo", "pid=,args="]) {
        attach_args(&mut procs, &args);
    }
    procs
}

/// Running jobs in `launchctl list` output (`PID\tStatus\tLabel`, `-` when idle): pid -> label.
pub fn parse_launchctl_list(s: &str) -> HashMap<u32, String> {
    s.lines()
        .filter_map(|l| {
            let mut cols = l.split('\t');
            let pid = cols.next()?.trim().parse().ok()?;
            Some((pid, cols.nth(1)?.trim().to_string()))
        })
        .collect()
}

fn launchd_jobs() -> HashMap<u32, String> {
    run_cmd("launchctl", &["list"]).map(|s| parse_launchctl_list(&s)).unwrap_or_default()
}

fn is_system_job(label: &str) -> bool {
    label.starts_with(SYSTEM_JOB_PREFIX)
}

// ---------- naming ----------

pub fn basename(comm: &str) -> &str {
    comm.rsplit('/').next().unwrap_or(comm)
}

/// Grouping label: the outermost `.app` bundle name, else the binary basename.
pub fn app_label(comm: &str) -> String {
    if let Some(i) = comm.find(".app/") {
        let head = &comm[..i];
        return head.rsplit('/').next().unwrap_or(head).to_string();
    }
    basename(comm).to_string()
}

/// Claude Code itself: the native binary under whatever path launched it, or the npm
/// package running under node.
pub fn is_claude(p: &RawProc) -> bool {
    let native = basename(&p.comm) == "claude" || p.comm.contains(CLAUDE_VERSIONS_DIR);
    native || (basename(&p.comm) == "node" && p.args.split_whitespace().any(is_claude_script))
}

/// A node entry point for Claude Code: the package's `cli.js` / `cli.mjs`, or a `bin/claude`
/// link to it (what `#!/usr/bin/env node` leaves in argv).
fn is_claude_script(arg: &str) -> bool {
    let name = basename(arg);
    let package_cli = arg.contains(CLAUDE_CODE_PACKAGE) && (name == "cli.js" || name == "cli.mjs");
    package_cli || (arg.contains('/') && name == "claude")
}

const HEADLESS_BINARIES: [&str; 3] = ["Google Chrome", "Chromium", "chrome-headless-shell"];
const HEADLESS_FLAGS: [&str; 3] = ["--remote-debugging-pipe", "--remote-debugging-port", "--headless"];

/// A browser root driven by automation (Playwright/Puppeteer): headless or debugging flags, not a `--type=` child.
pub fn is_headless_root(p: &RawProc) -> bool {
    let name = basename(&p.comm);
    let browser = HEADLESS_BINARIES.contains(&name) || p.comm.contains("MacOS/Google Chrome");
    browser
        && !p.args.contains("--type=")
        && HEADLESS_FLAGS
            .iter()
            .any(|f| p.args.split_whitespace().any(|a| a == *f || a.starts_with(&format!("{f}="))))
}

fn is_system_path(comm: &str) -> bool {
    SYSTEM_PREFIXES.iter().any(|p| comm.starts_with(p))
}

fn is_vm(comm: &str) -> bool {
    VM_MARKERS.iter().any(|m| comm.contains(m))
}

/// Group label for a VM process. The Virtualization.framework host is anonymous, so it
/// is attributed to OrbStack only while the OrbStack app itself is alive.
fn vm_label(comm: &str, orbstack_running: bool) -> &'static str {
    if orbstack_running && comm.contains(ORBSTACK_VM) {
        "OrbStack VM"
    } else {
        "Virtual machine"
    }
}

fn is_orphan_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    ORPHAN_NAMES.iter().any(|o| n.starts_with(o))
}

// ---------- process tree ----------

struct Tree<'a> {
    parent: HashMap<u32, u32>,
    comm: HashMap<u32, &'a str>,
    by_pid: HashMap<u32, &'a RawProc>,
}

impl<'a> Tree<'a> {
    fn new(procs: &'a [RawProc]) -> Self {
        Tree {
            parent: procs.iter().map(|p| (p.pid, p.ppid)).collect(),
            comm: procs.iter().map(|p| (p.pid, p.comm.as_str())).collect(),
            by_pid: procs.iter().map(|p| (p.pid, p)).collect(),
        }
    }

    fn ancestors(&self, pid: u32) -> impl Iterator<Item = u32> + '_ {
        let mut cur = pid;
        let mut steps = 0;
        std::iter::from_fn(move || {
            let p = *self.parent.get(&cur)?;
            if p == 0 || p == cur || steps > 128 {
                return None;
            }
            steps += 1;
            cur = p;
            Some(p)
        })
    }

    /// Editor hosting this process, by walking up to an editor bundle.
    fn host_editor(&self, pid: u32) -> Option<&'static str> {
        self.ancestors(pid).find_map(|a| {
            let c = self.comm.get(&a)?;
            if c.contains("Visual Studio Code.app/") {
                Some("vscode")
            } else if c.contains("/Cursor.app/") {
                Some("cursor")
            } else {
                None
            }
        })
    }
}

/// pid -> the running Claude agent it belongs to: each agent, its descendants, and
/// anything already attributed to a still-running agent (a dev server whose launching
/// shell has exited), with their descendants.
fn agents_of(procs: &[RawProc], tree: &Tree, seen: &HashMap<u32, ClaudeMark>) -> HashMap<u32, u32> {
    let agents: HashSet<u32> = procs.iter().filter(|p| is_claude(p)).map(|p| p.pid).collect();
    let anchor = |pid: u32| {
        if agents.contains(&pid) {
            return Some(pid);
        }
        seen.get(&pid).map(|m| m.agent).filter(|a| agents.contains(a))
    };
    procs
        .iter()
        .filter_map(|p| {
            let agent = std::iter::once(p.pid).chain(tree.ancestors(p.pid)).find_map(anchor)?;
            Some((p.pid, agent))
        })
        .collect()
}

/// Headless browser trees: pid -> (group label, driver state).
#[derive(Clone)]
struct HeadlessGroup {
    label: String,
    driver_alive: bool,
    orphaned: bool,
}

fn headless_groups(
    procs: &[RawProc],
    tree: &Tree,
    agents: &HashMap<u32, u32>,
    orphaned: &HashSet<u32>,
) -> HashMap<u32, HeadlessGroup> {
    let roots: Vec<&RawProc> = procs.iter().filter(|p| is_headless_root(p)).collect();
    if roots.is_empty() {
        return HashMap::new();
    }
    let groups: HashMap<u32, HeadlessGroup> = roots
        .iter()
        .map(|r| {
            let driver = if r.ppid > 1 { tree.by_pid.get(&r.ppid) } else { None };
            let is_orphan = orphaned.contains(&r.pid);
            let label = match driver {
                None if is_orphan => "Headless browser · orphaned".to_string(),
                None => "Headless browser".to_string(),
                Some(par) => {
                    let script = par
                        .args
                        .split_whitespace()
                        .next()
                        .map(basename)
                        .filter(|s| !s.starts_with('-') && !s.is_empty())
                        .unwrap_or(basename(&par.comm));
                    match agents.get(&r.pid) {
                        Some(agent) => format!("Headless browser · {script} · agent {agent}"),
                        None => format!("Headless browser · {script}"),
                    }
                }
            };
            (r.pid, HeadlessGroup { label, driver_alive: driver.is_some(), orphaned: is_orphan })
        })
        .collect();
    procs
        .iter()
        .filter_map(|p| {
            let root = if groups.contains_key(&p.pid) {
                Some(p.pid)
            } else {
                tree.ancestors(p.pid).find(|a| groups.contains_key(a))
            }?;
            Some((p.pid, groups[&root].clone()))
        })
        .collect()
}

// ---------- tagging ----------

/// The frontmost app as lsappinfo reports it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Frontmost {
    pub pid: Option<u32>,
    pub bundle_path: Option<String>,
}

/// True when `exe` lives inside the `.app` bundle at `bundle`.
fn in_bundle(bundle: &str, exe: &str) -> bool {
    exe.strip_prefix(bundle.trim_end_matches('/')).is_some_and(|rest| rest.starts_with('/'))
}

impl Frontmost {
    /// The app's main process or any helper whose executable lives inside its bundle.
    fn owns(&self, p: &RawProc) -> bool {
        self.pid == Some(p.pid) || self.bundle_path.as_deref().is_some_and(|b| in_bundle(b, &p.comm))
    }
}

/// This running Jetsam, so the frontmost-app rule can tell it apart from the user's app.
pub struct SelfApp {
    pub pid: u32,
    pub exe: String,
}

impl SelfApp {
    fn current() -> Self {
        let exe = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        SelfApp { pid: std::process::id(), exe }
    }

    fn is(&self, front: &Frontmost) -> bool {
        let own_bundle =
            !self.exe.is_empty() && front.bundle_path.as_deref().is_some_and(|b| in_bundle(b, &self.exe));
        front.pid == Some(self.pid) || own_bundle
    }
}

/// `lsappinfo info -only bundlepath -only pid <asn>` output: `"LSBundlePath"="/Applications/X.app"`
/// and `"pid"=123`, one per line.
pub fn parse_lsappinfo(info: &str) -> Frontmost {
    let mut front = Frontmost::default();
    for line in info.lines() {
        let Some((key, value)) = line.split_once('=') else { continue };
        let value = value.trim().trim_matches('"');
        match key.trim().trim_matches('"') {
            "pid" => front.pid = value.parse().ok(),
            "LSBundlePath" if value.ends_with(".app") => front.bundle_path = Some(value.to_string()),
            _ => {}
        }
    }
    front
}

pub struct TagCtx {
    pub self_pid: u32,
    pub frontmost: Option<Frontmost>,
    /// Jobs launchd runs (LaunchAgents, brew services), pid -> label: never orphans, and
    /// Apple's own are protected.
    pub launchd_jobs: HashMap<u32, String>,
    pub orbstack_running: bool,
    pub now: i64,
}

fn is_idle(p: &RawProc, samples: Option<&Vec<Sample>>, now: i64) -> bool {
    if p.rss < IDLE_MIN_RSS {
        return false;
    }
    let window: Vec<&Sample> =
        samples.map(|v| v.iter().filter(|s| now - s.ts <= IDLE_WINDOW_SEC).collect()).unwrap_or_default();
    if window.len() >= IDLE_MIN_SAMPLES {
        window.iter().all(|s| s.cpu_pct < IDLE_CPU_PCT)
    } else {
        p.elapsed_sec as i64 > IDLE_WINDOW_SEC && p.cpu_pct < IDLE_CPU_PCT
    }
}

fn is_leak(samples: Option<&Vec<Sample>>) -> bool {
    let Some(v) = samples else { return false };
    let (Some(first), Some(last)) = (v.first(), v.last()) else { return false };
    v.len() >= 2
        && last.rss >= first.rss + LEAK_MIN_GROWTH
        && last.rss as f64 >= first.rss as f64 * LEAK_MIN_RATIO
}

/// Handed to launchd after a sample saw it with a real parent, and not a launchd job.
/// A parent of 1 alone proves nothing: LaunchAgents and anything daemonised at birth
/// look exactly the same.
fn was_orphaned(p: &RawProc, seen: &Sampler, launchd_jobs: &HashMap<u32, String>) -> bool {
    p.ppid == 1
        && !launchd_jobs.contains_key(&p.pid)
        && seen.lineage.get(&p.pid).is_some_and(|l| l.had_parent)
}

pub fn tag(procs: &[RawProc], seen: &Sampler, ctx: &TagCtx) -> Vec<ProcRow> {
    let tree = Tree::new(procs);
    let agents = agents_of(procs, &tree, &seen.claude_seen);
    let in_grace = seen.claude_seen.iter().filter(|(_, m)| ctx.now - m.ts <= CLAUDE_GRACE_SEC);
    let claude: HashSet<u32> = agents.keys().copied().chain(in_grace.map(|(pid, _)| *pid)).collect();
    // The terminal or editor extension host running an agent: ending it ends the agent.
    let agent_hosts: HashSet<u32> =
        agents.values().collect::<HashSet<_>>().into_iter().flat_map(|a| tree.ancestors(*a)).collect();
    // The terminal or launcher we were started from: ending it would end us too.
    let own_ancestors: HashSet<u32> = tree.ancestors(ctx.self_pid).collect();
    let orphaned: HashSet<u32> =
        procs.iter().filter(|p| was_orphaned(p, seen, &ctx.launchd_jobs)).map(|p| p.pid).collect();
    let headless = headless_groups(procs, &tree, &agents, &orphaned);

    let mut rows: Vec<ProcRow> = procs
        .iter()
        .map(|p| {
            let name = basename(&p.comm).to_string();
            let under_claude = claude.contains(&p.pid);
            let hl = headless.get(&p.pid);
            let app = match hl {
                Some(g) => g.label.clone(),
                None if under_claude => "Claude Code".to_string(),
                None if is_vm(&p.comm) => vm_label(&p.comm, ctx.orbstack_running).to_string(),
                None => app_label(&p.comm),
            };
            let protected = p.pid <= 1
                || p.pid == ctx.self_pid
                || own_ancestors.contains(&p.pid)
                || under_claude
                || agent_hosts.contains(&p.pid)
                || is_system_path(&p.comm)
                || ctx.launchd_jobs.get(&p.pid).is_some_and(|label| is_system_job(label))
                || PROTECTED_NAMES.contains(&name.as_str())
                || ctx.frontmost.as_ref().is_some_and(|f| f.owns(p));
            let s = seen.samples.get(&p.pid);
            let mut reasons = Vec::new();
            match hl {
                Some(g) => {
                    if protected || g.driver_alive {
                        reasons.push("protected".into());
                    } else if g.orphaned {
                        reasons.push("orphan".into());
                    }
                    reasons.push("headless".into());
                }
                None if protected => reasons.push("protected".into()),
                None if orphaned.contains(&p.pid) && is_orphan_name(&name) => reasons.push("orphan".into()),
                None => {}
            }
            if is_idle(p, s, ctx.now) {
                reasons.push("idle".into());
            }
            if is_leak(s) {
                reasons.push("leak".into());
            }
            if is_vm(&p.comm) {
                reasons.push("vm".into());
            }
            let host = if is_claude(p) { tree.host_editor(p.pid).map(String::from) } else { None };
            ProcRow {
                pid: p.pid,
                ppid: p.ppid,
                name,
                app,
                rss: p.rss,
                cpu_pct: p.cpu_pct,
                elapsed_sec: p.elapsed_sec,
                reasons,
                host,
            }
        })
        .collect();
    rows.sort_by(|a, b| b.rss.cmp(&a.rss).then(a.pid.cmp(&b.pid)));
    rows
}

// ---------- sampler ----------

impl Sampler {
    /// Fold in one process table taken at `ts`. `session_id` looks up a live pid's session.
    pub fn record(&mut self, procs: &[RawProc], ts: i64, session_id: impl Fn(u32) -> Option<u32>) {
        let newcomers = self.update_lineage(procs, ts);
        let tree = Tree::new(procs);
        for (pid, agent) in agents_of(procs, &tree, &self.claude_seen) {
            self.claude_seen.insert(pid, ClaudeMark { agent, ts });
        }
        self.adopt(procs, &newcomers, ts, session_id);
        self.claude_seen.retain(|_, m| ts - m.ts <= CLAUDE_GRACE_SEC);
        self.push_samples(procs, ts);
    }

    /// Track each live pid's start time and whether it ever had a real parent. Returns the
    /// pids seen for the first time, including reused ones, whose old state is dropped.
    fn update_lineage(&mut self, procs: &[RawProc], ts: i64) -> HashSet<u32> {
        let live: HashSet<u32> = procs.iter().map(|p| p.pid).collect();
        self.lineage.retain(|pid, _| live.contains(pid));
        let mut newcomers = HashSet::new();
        for p in procs {
            let started = ts - p.elapsed_sec as i64;
            match self.lineage.get_mut(&p.pid) {
                Some(l) if (l.started - started).abs() <= START_SLACK_SEC => l.had_parent |= p.ppid > 1,
                _ => {
                    self.samples.remove(&p.pid);
                    self.claude_seen.remove(&p.pid);
                    self.lineage.insert(p.pid, Lineage { started, had_parent: p.ppid > 1 });
                    newcomers.insert(p.pid);
                }
            }
        }
        newcomers
    }

    /// A young process whose process group or session is led by a Claude process belongs
    /// to that agent, even if it reached launchd before any sample saw its parent
    /// (`nohup node server.js &` from an agent's shell).
    fn adopt(
        &mut self,
        procs: &[RawProc],
        newcomers: &HashSet<u32>,
        ts: i64,
        session_id: impl Fn(u32) -> Option<u32>,
    ) {
        let young = procs
            .iter()
            .filter(|p| newcomers.contains(&p.pid) && !self.claude_seen.contains_key(&p.pid))
            .filter(|p| p.elapsed_sec as i64 <= CLAUDE_GRACE_SEC);
        let claimed: Vec<(u32, u32)> = young
            .filter_map(|p| {
                let leaders = [Some(p.pgid), session_id(p.pid)].into_iter().flatten();
                let agent =
                    leaders.filter(|id| *id > 1).find_map(|id| self.claude_seen.get(&id).map(|m| m.agent))?;
                Some((p.pid, agent))
            })
            .collect();
        for (pid, agent) in claimed {
            self.claude_seen.insert(pid, ClaudeMark { agent, ts });
        }
    }

    /// Remember the frontmost app from a sample, unless it is Jetsam itself.
    pub fn note_frontmost(&mut self, front: Option<Frontmost>, me: &SelfApp) {
        let Some(front) = front.filter(|f| !me.is(f)) else { return };
        let started = front.pid.and_then(|pid| self.lineage.get(&pid)).map(|l| l.started);
        self.last_front = Some(LastFront { front, started });
    }

    /// The app the frontmost rule protects: the one in front now, or, while Jetsam is in
    /// front (or lsappinfo could not say), the last other app a sample saw there. Its whole
    /// bundle is protected; its pid only while the same process still holds it.
    pub fn frontmost_to_protect(&self, current: Option<Frontmost>, me: &SelfApp) -> Option<Frontmost> {
        if let Some(front) = current.filter(|f| !me.is(f)) {
            return Some(front);
        }
        let last = self.last_front.as_ref()?;
        let same_process = |pid: &u32| {
            last.started.is_some_and(|st| {
                self.lineage.get(pid).is_some_and(|l| (l.started - st).abs() <= START_SLACK_SEC)
            })
        };
        Some(Frontmost {
            pid: last.front.pid.filter(same_process),
            bundle_path: last.front.bundle_path.clone(),
        })
    }

    fn push_samples(&mut self, procs: &[RawProc], ts: i64) {
        let live: HashSet<u32> = procs.iter().map(|p| p.pid).collect();
        self.samples.retain(|pid, _| live.contains(pid));
        for p in procs {
            let v = self.samples.entry(p.pid).or_default();
            v.push(Sample { ts, rss: p.rss, cpu_pct: p.cpu_pct });
            if v.len() > MAX_SAMPLES {
                let extra = v.len() - MAX_SAMPLES;
                v.drain(..extra);
            }
        }
    }
}

/// Take one sample: process table, memory point, lineage, Claude attribution and the
/// frontmost app. Called every 10s and on demand.
pub fn sample(shared: &SharedSampler) {
    let procs = list_processes();
    let ts = now();
    let point = memory_mod::quick_point(ts);
    let front = frontmost_app();
    let mut s = lock(shared);
    s.record(&procs, ts, system::session_id);
    s.note_frontmost(front, &SelfApp::current());
    s.history.push(point);
    if s.history.len() > MAX_SAMPLES {
        let extra = s.history.len() - MAX_SAMPLES;
        s.history.drain(..extra);
    }
    s.latest = procs;
}

pub fn history(shared: &SharedSampler) -> Vec<HistPoint> {
    lock(shared).history.clone()
}

fn frontmost_app() -> Option<Frontmost> {
    // lsappinfo needs no Automation permission, unlike System Events via osascript.
    let asn = run_cmd("lsappinfo", &["front"])?;
    let info = run_cmd("lsappinfo", &["info", "-only", "bundlepath", "-only", "pid", asn.trim()])?;
    Some(parse_lsappinfo(&info))
}

/// Tagged rows from the latest sample (takes one if the sampler has not run yet).
pub fn rows(shared: &SharedSampler) -> Vec<ProcRow> {
    if lock(shared).latest.is_empty() {
        sample(shared);
    }
    let current = frontmost_app();
    let launchd_jobs = launchd_jobs();
    let me = SelfApp::current();
    let s = lock(shared);
    let has_vm = s.latest.iter().any(|p| is_vm(&p.comm));
    let ctx = TagCtx {
        self_pid: me.pid,
        frontmost: s.frontmost_to_protect(current, &me),
        launchd_jobs,
        orbstack_running: has_vm && system::orbstack_running(),
        now: now(),
    };
    tag(&s.latest, &s, &ctx)
}

// ---------- reclaim ----------

fn alive(pid: u32) -> bool {
    run_cmd("kill", &["-0", &pid.to_string()]).is_some()
}

fn signal(pid: u32, sig: &str) -> bool {
    run_cmd("kill", &[sig, &pid.to_string()]).is_some()
}

/// SIGTERM, 3s grace, SIGKILL leftovers. One action per process.
fn end_processes(targets: &[&ProcRow], actions: &mut Vec<ReclaimAction>) {
    if targets.is_empty() {
        return;
    }
    let mut termed = Vec::new();
    for r in targets {
        if signal(r.pid, "-TERM") {
            termed.push(*r);
        } else {
            actions.push(ReclaimAction {
                label: format!("End {} ({})", r.name, r.pid),
                ok: false,
                detail: "SIGTERM failed".into(),
            });
        }
    }
    if termed.is_empty() {
        return;
    }
    std::thread::sleep(TERM_GRACE);
    for r in termed {
        let label = format!("End {} ({})", r.name, r.pid);
        if !alive(r.pid) {
            actions.push(ReclaimAction { label, ok: true, detail: "Ended cleanly".into() });
        } else if signal(r.pid, "-KILL") {
            actions.push(ReclaimAction { label, ok: true, detail: "Did not exit in 3s, force-ended".into() });
        } else {
            actions.push(ReclaimAction { label, ok: false, detail: "Would not exit".into() });
        }
    }
}

pub fn reclaim(shared: &SharedSampler, tier: &str, pids: &[u32]) -> Result<ReclaimResult, String> {
    let aggressive = match tier {
        "safe" => false,
        "aggressive" => true,
        other => return Err(format!("unknown tier {other}")),
    };
    let before = memory_mod::snapshot(history(shared));
    sample(shared);
    let rows = rows(shared);
    let mut actions = Vec::new();
    let mut targets: Vec<&ProcRow> = rows.iter().filter(|r| r.has_reason("orphan")).collect();
    if aggressive {
        for pid in pids {
            match rows.iter().find(|r| r.pid == *pid) {
                None => actions.push(ReclaimAction {
                    label: format!("End pid {pid}"),
                    ok: false,
                    detail: "Not running".into(),
                }),
                Some(r) if r.has_reason("protected") => actions.push(ReclaimAction {
                    label: format!("End {} ({})", r.name, r.pid),
                    ok: false,
                    detail: "Refused: protected process".into(),
                }),
                Some(r) if r.has_reason("vm") => actions.push(ReclaimAction {
                    label: format!("End {} ({})", r.name, r.pid),
                    ok: false,
                    detail: "Refused: virtual machine. Stop it from its own app.".into(),
                }),
                Some(r) => {
                    if !targets.iter().any(|t| t.pid == r.pid) {
                        targets.push(r);
                    }
                }
            }
        }
    }
    end_processes(&targets, &mut actions);

    match admin_shell("purge") {
        Ok(_) => actions.push(ReclaimAction {
            label: "Flush file cache".into(),
            ok: true,
            detail: "purge ran".into(),
        }),
        Err(e) => actions.push(ReclaimAction {
            label: "Flush file cache".into(),
            ok: false,
            detail: if e.contains("canceled") { "Cancelled at the password prompt".into() } else { e },
        }),
    }

    std::thread::sleep(SETTLE);
    sample(shared);
    let after = memory_mod::snapshot(history(shared));
    let free_pct_delta = after.free_pct - before.free_pct;
    let swap_delta = before.swap_used as i64 - after.swap_used as i64;
    let ram_freed = (free_pct_delta.max(0.0) / 100.0 * before.total_bytes as f64) as u64;
    let freed_bytes = ram_freed + swap_delta.max(0) as u64;
    Ok(ReclaimResult { freed_bytes, free_pct_delta, swap_delta, before, after, actions })
}

// ---------- soft restart ----------

struct Editor {
    app: &'static str,
    marker: &'static str,
    settings: &'static str,
    host: &'static str,
}

const VSCODE: Editor = Editor {
    app: "Visual Studio Code",
    marker: "Visual Studio Code.app/Contents/MacOS",
    settings: "Library/Application Support/Code/User/settings.json",
    host: "vscode",
};
const CURSOR: Editor = Editor {
    app: "Cursor",
    marker: "Cursor.app/Contents/MacOS",
    settings: "Library/Application Support/Cursor/User/settings.json",
    host: "cursor",
};

/// `window.restoreWindows` from settings.json; tolerates comments by falling back to a textual scan.
pub fn restore_windows_value(text: &str) -> Option<String> {
    if let Ok(v) = serde_json::from_str::<Value>(text) {
        return v.get("window.restoreWindows").and_then(|x| x.as_str()).map(String::from);
    }
    let i = text.find("\"window.restoreWindows\"")? + "\"window.restoreWindows\"".len();
    let rest = text[i..].trim_start().strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    Some(rest[..rest.find('"')?].to_string())
}

/// Chrome's "Continue where you left off" (session.restore_on_startup == 1).
pub fn chrome_restores_session(prefs: &str) -> bool {
    serde_json::from_str::<Value>(prefs)
        .ok()
        .and_then(|v| v.get("session")?.get("restore_on_startup")?.as_i64())
        .map(|n| n == 1)
        .unwrap_or(false)
}

fn quit_and_relaunch(app: &str, running: impl Fn() -> bool) -> Result<String, String> {
    run_cmd("osascript", &["-e", &format!("tell application \"{app}\" to quit")])
        .ok_or_else(|| format!("{app} refused the quit request"))?;
    let deadline = std::time::Instant::now() + QUIT_WAIT;
    while running() {
        if std::time::Instant::now() > deadline {
            return Err(format!("{app} did not quit within 20s (unsaved changes?). Nothing was relaunched."));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    std::thread::sleep(Duration::from_millis(500));
    run_cmd("open", &["-a", app])
        .ok_or_else(|| format!("{app} quit but `open -a` failed; launch it by hand"))?;
    Ok(format!("{app} relaunched"))
}

fn restart_chrome(home: &Path) -> Result<String, String> {
    let running = || pgrep(&["-x", "Google Chrome"]);
    if !running() {
        return Err("Chrome is not running".into());
    }
    let prefs =
        std::fs::read_to_string(home.join("Library/Application Support/Google/Chrome/Default/Preferences"))
            .unwrap_or_default();
    if !chrome_restores_session(&prefs) {
        return Err("Chrome is not set to restore tabs. Turn on \"Continue where you left off\" at chrome://settings/onStartup first; nothing was quit.".into());
    }
    quit_and_relaunch("Google Chrome", running).map(|_| "Chrome relaunched with tabs restored".into())
}

/// Claude Code agents whose ancestor chain runs through this editor (`host` is only set
/// on agent rows).
pub fn agents_in(rows: &[ProcRow], host: &str) -> usize {
    rows.iter().filter(|r| r.host.as_deref() == Some(host)).count()
}

/// Why an editor restart is refused while agents run inside it.
fn agents_running_message(agents: usize, app: &str) -> String {
    let (noun, them) = if agents == 1 { ("agent is", "it") } else { ("agents are", "them") };
    format!("{agents} Claude Code {noun} running in {app}. Restarting {app} would stop {them}. Let {them} finish first.")
}

fn restart_editor(home: &Path, shared: &SharedSampler, ed: &Editor) -> Result<String, String> {
    let running = || pgrep(&["-f", ed.marker]);
    if !running() {
        return Err(format!("{} is not running", ed.app));
    }
    sample(shared);
    let agents = agents_in(&rows(shared), ed.host);
    if agents > 0 {
        return Err(agents_running_message(agents, ed.app));
    }
    let settings = std::fs::read_to_string(home.join(ed.settings)).unwrap_or_default();
    if restore_windows_value(&settings).as_deref() == Some("none") {
        return Err(format!("{} has window.restoreWindows set to \"none\", so windows would not come back. Change it first; nothing was quit.", ed.app));
    }
    quit_and_relaunch(ed.app, running).map(|_| format!("{} relaunched with windows restored", ed.app))
}

pub fn soft_restart(shared: &SharedSampler, app: &str, home: &Path) -> FreshenResult {
    let res = match app {
        "chrome" => restart_chrome(home),
        "vscode" => restart_editor(home, shared, &VSCODE),
        "cursor" => restart_editor(home, shared, &CURSOR),
        other => Err(format!("unknown app {other}")),
    };
    match res {
        Ok(message) => FreshenResult { ok: true, message },
        Err(message) => FreshenResult { ok: false, message },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MB: u64 = 1024 * 1024;
    const NODE: &str = "/opt/homebrew/bin/node";
    const CHROME: &str = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
    const VSCODE_APP: &str = "/Applications/Visual Studio Code.app";
    const NATIVE_CLAUDE: &str =
        "/Users/n/.vscode/extensions/anthropic.claude-code/resources/native-binary/claude";

    fn p(pid: u32, ppid: u32, rss_mb: u64, cpu: f64, elapsed: u64, comm: &str) -> RawProc {
        RawProc {
            pid,
            ppid,
            pgid: pid,
            rss: rss_mb * MB,
            cpu_pct: cpu,
            elapsed_sec: elapsed,
            comm: comm.into(),
            args: String::new(),
        }
    }

    fn pa(pid: u32, ppid: u32, rss_mb: u64, comm: &str, args: &str) -> RawProc {
        RawProc { args: args.into(), ..p(pid, ppid, rss_mb, 1.0, 100, comm) }
    }

    fn in_group(proc: RawProc, pgid: u32) -> RawProc {
        RawProc { pgid, ..proc }
    }

    fn ctx() -> TagCtx {
        TagCtx {
            self_pid: 999,
            frontmost: Some(Frontmost {
                pid: None,
                bundle_path: Some("/Applications/Google Chrome.app".into()),
            }),
            launchd_jobs: HashMap::new(),
            orbstack_running: true,
            now: 10_000,
        }
    }

    /// A sampler that has recorded each process table in turn, with no session ids.
    fn observed(tables: &[(i64, Vec<RawProc>)]) -> Sampler {
        let mut s = Sampler::default();
        for (ts, procs) in tables {
            s.record(procs, *ts, |_| None);
        }
        s
    }

    fn reasons(rows: &[ProcRow], pid: u32) -> Vec<String> {
        rows.iter().find(|r| r.pid == pid).map(|r| r.reasons.clone()).unwrap_or_default()
    }

    fn row(rows: &[ProcRow], pid: u32) -> &ProcRow {
        rows.iter().find(|r| r.pid == pid).expect("row for pid")
    }

    fn is_protected(rows: &[ProcRow], pid: u32) -> bool {
        row(rows, pid).has_reason("protected")
    }

    #[test]
    fn ps_parsing() {
        let rows = parse_ps("  922   631   631  40448   2.5 01-07:02:57 /Applications/Visual Studio Code.app/Contents/Frameworks/Code Helper.app/Contents/MacOS/Code Helper\n 4 0 0 10 0.0 05:12 kernel_task\nbad line\n");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].pid, 922);
        assert_eq!(rows[0].ppid, 631);
        assert_eq!(rows[0].pgid, 631);
        assert_eq!(rows[0].rss, 40448 * 1024);
        assert_eq!(rows[0].cpu_pct, 2.5);
        assert_eq!(rows[0].elapsed_sec, 86400 + 7 * 3600 + 2 * 60 + 57);
        assert_eq!(basename(&rows[0].comm), "Code Helper");
        assert_eq!(app_label(&rows[0].comm), "Visual Studio Code");
        assert_eq!(rows[1].elapsed_sec, 312);
        assert_eq!(parse_etime("03:04:05"), 3 * 3600 + 4 * 60 + 5);
        assert_eq!(parse_etime("07"), 7);
        assert_eq!(app_label("/usr/bin/node"), "node");
    }

    #[test]
    fn launchctl_and_lsappinfo_parsing() {
        let jobs = parse_launchctl_list(
            "PID\tStatus\tLabel\n-\t0\tcom.apple.progressd\n812\t0\thomebrew.mxcl.postgresql@16\n",
        );
        assert_eq!(jobs, HashMap::from([(812, "homebrew.mxcl.postgresql@16".to_string())]));
        let front =
            parse_lsappinfo("\"LSBundlePath\"=\"/Applications/Visual Studio Code.app\"\n\"pid\"=631\n");
        assert_eq!(front, Frontmost { pid: Some(631), bundle_path: Some(VSCODE_APP.into()) });
        assert_eq!(parse_lsappinfo("\"LSBundlePath\"=\"/\"\n").bundle_path, None, "only .app bundles");
    }

    #[test]
    fn protected_basics() {
        let procs = vec![
            p(1, 0, 5, 0.0, 99_999, "/sbin/launchd"),
            p(11, 1, 300, 0.0, 50_000, "/usr/libexec/logd"),
            p(12, 1, 300, 0.0, 50_000, "/System/Library/CoreServices/WindowServer"),
            p(13, 500, 300, 20.0, 50_000, NODE),
            p(14, 1, 100, 0.0, 50_000, CHROME),
            p(500, 1, 300, 0.0, 50_000, "/Applications/iTerm.app/Contents/MacOS/iTerm2"),
            p(501, 500, 300, 0.0, 50_000, "/bin/zsh"),
            p(999, 501, 100, 0.0, 10, "/Applications/Jetsam.app/Contents/MacOS/jetsam"),
        ];
        let rows = tag(&procs, &observed(&[(10_000, procs.clone())]), &ctx());
        assert_eq!(reasons(&rows, 11), vec!["protected", "idle"]);
        assert_eq!(reasons(&rows, 12), vec!["protected", "idle"]);
        assert!(reasons(&rows, 13).is_empty());
        assert_eq!(reasons(&rows, 14), vec!["protected"], "frontmost app is protected");
        assert_eq!(reasons(&rows, 999), vec!["protected"], "self is protected");
        assert_eq!(reasons(&rows, 500), vec!["protected", "idle"], "the terminal we run under is protected");
        assert_eq!(reasons(&rows, 501), vec!["protected", "idle"], "and so is its shell");
        assert_eq!(reasons(&rows, 1), vec!["protected"]);
        assert_eq!(rows[0].rss, 300 * MB, "sorted by rss desc");
    }

    #[test]
    fn reparented_node_with_dead_parent_is_orphan() {
        let before = vec![
            p(1, 0, 5, 0.0, 99_990, "/sbin/launchd"),
            p(500, 1, 50, 0.0, 49_990, "/bin/zsh"),
            p(10, 500, 300, 0.0, 49_990, NODE),
            p(16, 500, 300, 0.0, 49_990, NODE),
        ];
        let after = vec![
            p(1, 0, 5, 0.0, 100_000, "/sbin/launchd"),
            p(10, 1, 300, 0.0, 50_000, NODE),
            p(16, 1, 300, 0.0, 50_000, NODE),
        ];
        let seen = observed(&[(9_990, before), (10_000, after.clone())]);
        let ctx = TagCtx { launchd_jobs: HashMap::from([(16, "homebrew.mxcl.redis".into())]), ..ctx() };
        let rows = tag(&after, &seen, &ctx);
        assert_eq!(reasons(&rows, 10), vec!["orphan", "idle"], "its shell exited and launchd took it");
        assert_eq!(reasons(&rows, 16), vec!["idle"], "a launchd job is never an orphan");
    }

    #[test]
    fn daemonised_at_birth_node_is_not_orphan() {
        let table =
            |elapsed| vec![p(1, 0, 5, 0.0, 99_999, "/sbin/launchd"), p(20, 1, 300, 0.0, elapsed, NODE)];
        let seen = observed(&[(9_990, table(49_990)), (10_000, table(50_000))]);
        let rows = tag(&table(50_000), &seen, &ctx());
        assert_eq!(reasons(&rows, 20), vec!["idle"], "launchd's child since we first saw it");
    }

    #[test]
    fn reused_pid_forgets_its_parent() {
        let first = vec![p(500, 1, 50, 0.0, 1000, "/bin/zsh"), p(30, 500, 300, 0.0, 1000, NODE)];
        let reborn = vec![p(30, 1, 300, 0.0, 3, NODE)];
        let seen = observed(&[(9_990, first), (10_000, reborn.clone())]);
        assert!(!seen.lineage[&30].had_parent, "a different process now holds pid 30");
        assert!(reasons(&tag(&reborn, &seen, &ctx()), 30).is_empty());
    }

    #[test]
    fn idle_and_leak_from_samples() {
        let procs = vec![
            p(20, 2, 500, 0.1, 100, "/opt/homebrew/bin/python3"),
            p(21, 2, 500, 0.1, 100, "/opt/homebrew/bin/ruby"),
            p(22, 2, 50, 0.0, 99_999, "/opt/homebrew/bin/java"),
        ];
        let mut seen = Sampler::default();
        seen.samples.insert(
            20,
            (0..8).map(|i| Sample { ts: 10_000 - 600 + i * 60, rss: 500 * MB, cpu_pct: 0.1 }).collect(),
        );
        seen.samples.insert(
            21,
            vec![
                Sample { ts: 9_500, rss: 200 * MB, cpu_pct: 3.0 },
                Sample { ts: 9_800, rss: 400 * MB, cpu_pct: 3.0 },
            ],
        );
        let rows = tag(&procs, &seen, &ctx());
        assert_eq!(reasons(&rows, 20), vec!["idle"]);
        assert_eq!(reasons(&rows, 21), vec!["leak"], "two samples, +200MB and +100%");
        assert!(reasons(&rows, 22).is_empty(), "under 200 MB is never idle");
    }

    #[test]
    fn vm_is_labelled_never_actioned() {
        let procs = vec![
            p(30, 1, 8000, 1.0, 5000, "/Applications/OrbStack.app/Contents/Frameworks/OrbStack Helper.app/Contents/MacOS/com.apple.Virtualization.VirtualMachine"),
            p(31, 1, 2000, 1.0, 5000, "/opt/homebrew/bin/qemu-system-aarch64"),
        ];
        let seen = Sampler::default();
        let rows = tag(&procs, &seen, &ctx());
        let orb = row(&rows, 30);
        assert_eq!(orb.reasons, vec!["vm"], "vm is informational: no idle action tag");
        assert_eq!(orb.app, "OrbStack VM");
        let qemu = row(&rows, 31);
        assert_eq!(qemu.reasons, vec!["vm"]);
        assert_eq!(qemu.app, "Virtual machine", "only the Virtualization.framework host is OrbStack's");
        let no_orb = TagCtx { orbstack_running: false, ..ctx() };
        let rows = tag(&procs, &seen, &no_orb);
        assert_eq!(row(&rows, 30).app, "Virtual machine", "OrbStack app not alive");
    }

    fn vscode(rel: &str) -> String {
        format!("{VSCODE_APP}/Contents/{rel}")
    }

    #[test]
    fn claude_descendants_and_ancestors_are_protected() {
        let plugin_host = vscode("Frameworks/Code Helper (Plugin).app/Contents/MacOS/Code Helper (Plugin)");
        let procs = vec![
            p(1, 0, 5, 0.0, 99_999, "/sbin/launchd"),
            p(40, 1, 100, 0.0, 100, &vscode("MacOS/Electron")),
            p(41, 40, 300, 0.0, 50_000, &plugin_host),
            p(42, 41, 100, 0.0, 100, "/bin/zsh"),
            p(43, 42, 400, 5.0, 100, NATIVE_CLAUDE),
            p(44, 43, 300, 0.0, 50_000, NODE),
            p(45, 44, 300, 0.0, 50_000, "/opt/homebrew/bin/bun"),
            p(46, 1, 300, 0.0, 50_000, "/Applications/Cursor.app/Contents/MacOS/Cursor"),
            p(47, 46, 300, 0.0, 50_000, "/opt/homebrew/bin/claude"),
            p(
                48,
                40,
                300,
                0.0,
                50_000,
                &vscode("Frameworks/Code Helper (Renderer).app/Contents/MacOS/Code Helper (Renderer)"),
            ),
        ];
        let no_front = TagCtx { frontmost: None, ..ctx() };
        let rows = tag(&procs, &observed(&[(10_000, procs.clone())]), &no_front);
        for pid in [43, 44, 45, 47] {
            let r = row(&rows, pid);
            assert_eq!(r.reasons[0], "protected", "pid {pid}");
            assert_eq!(r.app, "Claude Code", "pid {pid}");
        }
        for pid in [40, 41, 42, 46] {
            assert!(is_protected(&rows, pid), "agent host {pid} is protected");
            assert_ne!(row(&rows, pid).app, "Claude Code", "pid {pid} keeps its own label");
        }
        assert!(!is_protected(&rows, 48), "a sibling renderer is not hosting the agent");
        assert_eq!(row(&rows, 43).host.as_deref(), Some("vscode"));
        assert_eq!(row(&rows, 47).host.as_deref(), Some("cursor"));
        assert_eq!(row(&rows, 44).host, None, "only claude itself carries host");
        assert_eq!(agents_in(&rows, "vscode"), 1);
        assert_eq!(agents_in(&rows, "cursor"), 1);
    }

    #[test]
    fn npm_claude_code_is_detected() {
        let cli = "/opt/homebrew/lib/node_modules/@anthropic-ai/claude-code/cli.js";
        assert!(is_claude(&pa(1, 2, 1, NODE, &format!("{cli} --resume"))));
        assert!(is_claude(&pa(
            1,
            2,
            1,
            NODE,
            "--no-warnings /Users/n/.npm/_npx/x/node_modules/@anthropic-ai/claude-code/cli.mjs"
        )));
        assert!(is_claude(&pa(1, 2, 1, NODE, "/opt/homebrew/bin/claude -p hi")), "bin link via shebang");
        assert!(is_claude(&pa(1, 2, 1, "claude", "")), "argv[0] from PATH");
        assert!(is_claude(&pa(1, 2, 1, "/Users/n/.local/share/claude/versions/2.1.318", "")));
        assert!(is_claude(&pa(1, 2, 1, "/Users/n/.local/bin/claude", "")));
        assert!(!is_claude(&pa(1, 2, 1, NODE, "/Users/n/app/server.js claude")));
        assert!(!is_claude(&pa(1, 2, 1, NODE, "/Users/n/app/node_modules/@anthropic-ai/sdk/cli.js")));
        assert!(!is_claude(&pa(1, 2, 1, "/Applications/Claude.app/Contents/MacOS/Claude", "")));

        let procs = vec![pa(50, 1, 400, NODE, cli), p(51, 50, 300, 0.0, 50_000, "/opt/homebrew/bin/bun")];
        let rows = tag(&procs, &observed(&[(10_000, procs.clone())]), &ctx());
        for pid in [50, 51] {
            assert!(is_protected(&rows, pid), "pid {pid}");
            assert_eq!(row(&rows, pid).app, "Claude Code", "pid {pid}");
        }
    }

    #[test]
    fn frontmost_bundle_helpers_are_protected() {
        let procs = vec![
            p(80, 1, 300, 0.0, 100, &vscode("MacOS/Electron")),
            p(
                81,
                80,
                300,
                0.0,
                100,
                &vscode("Frameworks/Code Helper (Renderer).app/Contents/MacOS/Code Helper (Renderer)"),
            ),
            p(
                82,
                80,
                300,
                0.0,
                100,
                &vscode("Frameworks/Code Helper (Plugin).app/Contents/MacOS/Code Helper (Plugin)"),
            ),
            p(
                83,
                1,
                300,
                0.0,
                100,
                "/Applications/Cursor.app/Contents/Frameworks/Cursor Helper.app/Contents/MacOS/Cursor Helper",
            ),
            p(84, 1, 300, 0.0, 100, "/Applications/Visual Studio Code.app.old/Contents/MacOS/Electron"),
            p(85, 1, 300, 0.0, 100, NODE),
        ];
        let front = Frontmost { pid: Some(85), bundle_path: Some(format!("{VSCODE_APP}/")) };
        let ctx = TagCtx { frontmost: Some(front), ..ctx() };
        let rows = tag(&procs, &Sampler::default(), &ctx);
        for pid in [80, 81, 82] {
            assert!(is_protected(&rows, pid), "pid {pid} lives in the frontmost bundle");
        }
        assert!(!is_protected(&rows, 83), "another app");
        assert!(!is_protected(&rows, 84), "a sibling path that only shares a prefix");
        assert!(is_protected(&rows, 85), "the frontmost pid itself");
    }

    #[test]
    fn system_executables_and_apple_jobs_are_protected() {
        let procs = vec![
            p(1, 0, 5, 0.0, 99_999, "/sbin/launchd"),
            p(20, 1, 300, 0.0, 50_000, "/usr/sbin/cfprefsd"),
            p(21, 1, 300, 0.0, 50_000, "/usr/bin/sshd-keygen-wrapper"),
            p(22, 1, 300, 0.0, 50_000, "/sbin/mount_apfs"),
            p(23, 1, 300, 0.0, 50_000, "/bin/zsh"),
            p(
                24,
                1,
                300,
                0.0,
                50_000,
                "/Library/Apple/System/Library/CoreServices/XProtect.app/Contents/MacOS/XProtect",
            ),
            p(25, 1, 300, 0.0, 50_000, "/usr/libexec/trustd"),
            p(26, 1, 300, 0.0, 50_000, "/Applications/Utilities/SomeAppleAgent"),
            p(27, 1, 300, 0.0, 50_000, "/opt/homebrew/opt/redis/bin/redis-server"),
            p(28, 1, 300, 0.0, 50_000, "/usr/local/bin/node"),
            p(29, 1, 300, 0.0, 50_000, "/Library/Application Support/Vendor/agent"),
            p(30, 1, 300, 0.0, 50_000, "/usr/binaries/tool"),
        ];
        let jobs = HashMap::from([
            (26, "com.apple.something.agent".to_string()),
            (27, "homebrew.mxcl.redis".to_string()),
            (29, "application.com.apple.Safari.1234.5678".to_string()),
        ]);
        let rows = tag(&procs, &Sampler::default(), &TagCtx { launchd_jobs: jobs, ..ctx() });
        for pid in [20, 21, 22, 23, 24, 25] {
            assert!(is_protected(&rows, pid), "pid {pid} runs from a system path");
        }
        assert!(is_protected(&rows, 26), "Apple's launchd job");
        assert!(!is_protected(&rows, 27), "a brew service is a launchd job but not a system one");
        assert!(!is_protected(&rows, 28), "/usr/local is the user's");
        assert!(!is_protected(&rows, 29), "an app launched by the user is not an Apple job");
        assert!(!is_protected(&rows, 30), "a sibling that only shares a prefix");
    }

    #[test]
    fn last_non_jetsam_frontmost_is_protected_while_jetsam_is_in_front() {
        let electron = vscode("MacOS/Electron");
        let renderer = vscode("Frameworks/Code Helper (Renderer).app/Contents/MacOS/Code Helper (Renderer)");
        let jetsam_exe = "/Applications/Jetsam.app/Contents/MacOS/jetsam";
        let table = |ts: i64| {
            let age = |born: i64| (ts - born) as u64;
            vec![
                p(1, 0, 5, 0.0, age(0), "/sbin/launchd"),
                p(14, 1, 300, 0.0, age(5_000), CHROME),
                p(80, 1, 300, 0.0, age(5_000), &electron),
                p(81, 80, 300, 0.0, age(5_000), &renderer),
                p(999, 1, 100, 0.0, age(9_000), jetsam_exe),
            ]
        };
        let me = SelfApp { pid: 999, exe: jetsam_exe.into() };
        let chrome = Frontmost { pid: Some(14), bundle_path: Some("/Applications/Google Chrome.app".into()) };
        let code = Frontmost { pid: Some(80), bundle_path: Some(VSCODE_APP.into()) };
        let jetsam = Frontmost { pid: Some(999), bundle_path: Some("/Applications/Jetsam.app".into()) };
        let mut seen = Sampler::default();
        for (ts, front) in [(9_970, &chrome), (9_980, &code), (9_990, &jetsam)] {
            seen.record(&table(ts), ts, |_| None);
            seen.note_frontmost(Some(front.clone()), &me);
        }
        assert_eq!(
            seen.last_front.as_ref().map(|l| &l.front),
            Some(&code),
            "Jetsam itself is never remembered"
        );

        let procs = table(10_000);
        seen.record(&procs, 10_000, |_| None);
        let front = seen.frontmost_to_protect(Some(jetsam.clone()), &me);
        assert_eq!(front, Some(code.clone()));
        let rows = tag(&procs, &seen, &TagCtx { self_pid: 999, frontmost: front, ..ctx() });
        assert!(is_protected(&rows, 80), "the editor the user was in before clicking Reclaim");
        assert!(is_protected(&rows, 81), "and every helper in its bundle");
        assert!(!is_protected(&rows, 14), "an app that was in front earlier is not");

        let unknown = SelfApp { pid: 999, exe: String::new() };
        let dev_build = Frontmost { pid: Some(999), bundle_path: None };
        assert_eq!(
            seen.frontmost_to_protect(Some(dev_build), &unknown),
            Some(code.clone()),
            "matched by pid"
        );
        assert_eq!(seen.frontmost_to_protect(None, &me), Some(code), "lsappinfo failed");
        assert_eq!(
            seen.frontmost_to_protect(Some(chrome.clone()), &me),
            Some(chrome),
            "another app is in front"
        );

        // VS Code quit and a new process took pid 80: only the bundle stays protected.
        seen.record(&[p(80, 1, 300, 0.0, 3, NODE)], 10_010, |_| None);
        let front = seen.frontmost_to_protect(Some(jetsam), &me);
        assert_eq!(front, Some(Frontmost { pid: None, bundle_path: Some(VSCODE_APP.into()) }));
    }

    #[test]
    fn restart_refusal_reads_naturally() {
        assert_eq!(
            agents_running_message(3, "Visual Studio Code"),
            "3 Claude Code agents are running in Visual Studio Code. Restarting Visual Studio Code would stop them. Let them finish first."
        );
        assert_eq!(
            agents_running_message(1, "Cursor"),
            "1 Claude Code agent is running in Cursor. Restarting Cursor would stop it. Let it finish first."
        );
    }

    #[test]
    fn reparented_claude_child_stays_protected_within_grace() {
        // Agent 7 has exited; its node children were handed to launchd.
        let procs = vec![p(60, 1, 300, 0.0, 50_000, NODE), p(61, 1, 300, 0.0, 50_000, NODE)];
        let mut seen = Sampler::default();
        for pid in [60, 61] {
            seen.lineage.insert(pid, Lineage { started: 10_000 - 50_000, had_parent: true });
        }
        seen.claude_seen.insert(60, ClaudeMark { agent: 7, ts: 10_000 - 30 });
        seen.claude_seen.insert(61, ClaudeMark { agent: 7, ts: 10_000 - 600 });
        let rows = tag(&procs, &seen, &ctx());
        assert_eq!(reasons(&rows, 60), vec!["protected", "idle"], "agent seen 30s ago");
        assert_eq!(reasons(&rows, 61), vec!["orphan", "idle"], "agent gone 10 min: killable again");
    }

    #[test]
    fn background_server_of_live_agent_outlives_grace() {
        let agent = p(70, 1, 400, 5.0, 1_000, "/opt/homebrew/bin/claude");
        let before = vec![agent.clone(), p(71, 70, 10, 0.0, 30, "/bin/zsh"), p(72, 71, 300, 0.0, 30, NODE)];
        let after = vec![RawProc { elapsed_sec: 1_300, ..agent }, p(72, 1, 300, 0.0, 330, NODE)];
        let seen = observed(&[(9_700, before), (10_000, after.clone())]);
        let rows = tag(&after, &seen, &ctx());
        assert_eq!(reasons(&rows, 72), vec!["protected"], "its shell exited, the agent is still running");
        assert_eq!(row(&rows, 72).app, "Claude Code");
    }

    #[test]
    fn young_process_in_agent_group_or_session_is_adopted() {
        let agent = p(70, 1, 400, 5.0, 1_000, "/opt/homebrew/bin/claude");
        let shell = in_group(p(71, 70, 10, 0.0, 2, "/bin/bash"), 71);
        let after = vec![
            RawProc { elapsed_sec: 1_010, ..agent.clone() },
            in_group(p(73, 1, 300, 0.0, 5, NODE), 71),
            p(74, 1, 300, 0.0, 5, NODE),
            p(75, 1, 300, 0.0, 5, NODE),
            in_group(p(76, 1, 300, 0.0, 5_000, NODE), 71),
        ];
        let mut seen = Sampler::default();
        seen.record(&[agent, shell], 9_990, |_| None);
        seen.record(&after, 10_000, |pid| Some(if pid == 74 { 71 } else { 1 }));
        let rows = tag(&after, &seen, &ctx());
        assert!(is_protected(&rows, 73), "same process group as the agent's shell");
        assert!(is_protected(&rows, 74), "same session as the agent's shell");
        assert!(reasons(&rows, 75).is_empty(), "unrelated daemon: neither protected nor orphan");
        assert!(!is_protected(&rows, 76), "too old to have come from that shell");
    }

    #[test]
    fn headless_root_detection() {
        assert!(is_headless_root(&pa(1, 2, 1, CHROME, "--remote-debugging-pipe --user-data-dir=/tmp/x")));
        assert!(is_headless_root(&pa(1, 2, 1, CHROME, "--headless=new --disable-gpu")));
        assert!(is_headless_root(&pa(
            1,
            2,
            1,
            "/opt/pw/chromium/chrome-headless-shell",
            "--remote-debugging-port=9222"
        )));
        assert!(
            !is_headless_root(&pa(1, 2, 1, CHROME, "--type=renderer --headless")),
            "--type= children are not roots"
        );
        assert!(!is_headless_root(&pa(1, 2, 1, CHROME, "")), "the user's Chrome has no automation flags");
        assert!(!is_headless_root(&pa(1, 2, 1, NODE, "--headless")), "not a browser");
    }

    #[test]
    fn headless_trees_group_and_tag() {
        let mut procs = vec![
            p(70, 1, 400, 5.0, 100, "/opt/homebrew/bin/claude"),
            pa(71, 70, 100, NODE, "/Users/n/scrape/crawl.js --fast"),
            pa(72, 71, 300, CHROME, "--remote-debugging-pipe"),
            pa(73, 72, 200, CHROME, "--type=renderer"),
            pa(74, 73, 100, CHROME, "--type=utility"),
            pa(80, 1, 300, CHROME, "--headless"),
            pa(81, 80, 200, CHROME, "--type=renderer"),
            pa(85, 1, 300, CHROME, "--headless"),
            pa(90, 1, 50, CHROME, ""),
        ];
        let out = parse_ps("  70   1  70  409600 5.0 01:40 /opt/homebrew/bin/claude\n");
        attach_args(&mut procs, "70 /opt/homebrew/bin/claude\n72 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome --remote-debugging-pipe\n71 node /Users/n/scrape/crawl.js --fast\n");
        assert_eq!(out.len(), 1);
        assert_eq!(procs[2].args, "--remote-debugging-pipe");
        assert_eq!(procs[1].args, "/Users/n/scrape/crawl.js --fast", "basename argv[0] is stripped too");
        let mut seen = Sampler::default();
        seen.lineage.insert(80, Lineage { started: 9_900, had_parent: true });
        let rows = tag(&procs, &seen, &TagCtx { frontmost: None, ..ctx() });
        for pid in [72, 73, 74] {
            let r = row(&rows, pid);
            assert_eq!(r.app, "Headless browser · crawl.js · agent 70", "pid {pid}");
            assert_eq!(r.reasons, vec!["protected", "headless"], "pid {pid}");
        }
        for pid in [80, 81] {
            let r = row(&rows, pid);
            assert_eq!(r.app, "Headless browser · orphaned", "pid {pid}");
            assert_eq!(r.reasons, vec!["orphan", "headless"], "pid {pid}");
        }
        let unproven = row(&rows, 85);
        assert_eq!(unproven.app, "Headless browser", "never seen with a driver");
        assert_eq!(unproven.reasons, vec!["headless"]);
        let user = row(&rows, 90);
        assert_eq!(user.app, "Google Chrome");
        assert!(user.reasons.is_empty());
    }

    #[test]
    fn editor_and_chrome_settings() {
        assert_eq!(restore_windows_value("{\"window.restoreWindows\": \"none\"}").as_deref(), Some("none"));
        assert_eq!(
            restore_windows_value("// comment\n{\n  \"window.restoreWindows\": \"all\", // x\n}").as_deref(),
            Some("all")
        );
        assert_eq!(restore_windows_value("{}"), None);
        assert!(chrome_restores_session("{\"session\":{\"restore_on_startup\":1}}"));
        assert!(!chrome_restores_session("{\"session\":{\"restore_on_startup\":5}}"));
        assert!(!chrome_restores_session("{}"));
    }
}
