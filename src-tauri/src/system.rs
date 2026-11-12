//! System glue: timed subprocess helpers, the Freshen actions (snapshots, hibernate,
//! VS Code excludes, purge, restart prompt), Finder reveal, the weekly launchd schedule
//! and the Full Disk Access probe.

use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::rules;
use crate::types::{FreshenResult, FreshenStatus};

/// Run a command with a hard 8s timeout. Stdout on exit 0; None on spawn failure,
/// non-zero exit or timeout.
pub fn run_cmd(cmd: &str, args: &[&str]) -> Option<String> {
    let (status, stdout) = run_with_timeout(cmd, args, std::time::Duration::from_secs(8))?;
    if status {
        Some(stdout)
    } else {
        None
    }
}

/// Run a command, returning stdout regardless of exit status (for tools that exit non-zero on "nothing to do").
pub fn run_cmd_lenient(cmd: &str, args: &[&str]) -> Option<String> {
    run_with_timeout(cmd, args, std::time::Duration::from_secs(8)).map(|(_, out)| out)
}

fn run_with_timeout(cmd: &str, args: &[&str], timeout: std::time::Duration) -> Option<(bool, String)> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut child = Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let start = std::time::Instant::now();
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            let buf = reader.join().unwrap_or_default();
            return Some((status.success(), String::from_utf8_lossy(&buf).into_owned()));
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

pub fn has_cmd(cmd: &str) -> bool {
    run_cmd("which", &[cmd]).is_some()
}

/// True when `pgrep` with these arguments matches at least one process.
pub fn pgrep(args: &[&str]) -> bool {
    run_cmd("pgrep", args).map(|s| !s.trim().is_empty()).unwrap_or(false)
}

/// The OrbStack app is running. Its VM shows up as a `com.apple.Virtualization.VirtualMachine`
/// process; we only ever label it, since OrbStack returns idle memory to macOS on its own.
pub fn orbstack_running() -> bool {
    pgrep(&["-x", "OrbStack"])
}

mod libsystem {
    extern "C" {
        pub fn getsid(pid: i32) -> i32;
        pub fn getuid() -> u32;
    }
}

/// Session id of a live process. `ps` on macOS always prints 0 for it, so ask the kernel.
pub fn session_id(pid: u32) -> Option<u32> {
    let pid = i32::try_from(pid).ok()?;
    // SAFETY: getsid takes a plain integer and touches no memory of ours; it returns -1 on error.
    let sid = unsafe { libsystem::getsid(pid) };
    u32::try_from(sid).ok()
}

/// Real uid of this process, which names the user's launchd GUI domain.
pub fn real_uid() -> u32 {
    // SAFETY: getuid has no arguments and cannot fail.
    unsafe { libsystem::getuid() }
}

/// Run a shell line with an admin prompt via osascript.
pub fn admin_shell(line: &str) -> Result<String, String> {
    let script = format!(
        "do shell script \"{}\" with administrator privileges",
        line.replace('\\', "\\\\").replace('"', "\\\"")
    );
    let out =
        std::process::Command::new("osascript").arg("-e").arg(script).output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

fn vscode_settings_path(home: &Path) -> PathBuf {
    home.join("Library/Application Support/Code/User/settings.json")
}

const EXCLUDE_GLOBS: [&str; 4] = ["**/node_modules/**", "**/.next/**", "**/dist/**", "**/.git/objects/**"];

fn vscode_excludes_present(settings: &Value) -> bool {
    settings
        .get("files.watcherExclude")
        .and_then(|v| v.as_object())
        .map(|o| EXCLUDE_GLOBS.iter().all(|g| o.get(*g).and_then(|v| v.as_bool()).unwrap_or(false)))
        .unwrap_or(false)
}

/// Merge exclude globs and tsserver memory into a VS Code settings object, preserving other keys.
pub fn merge_vscode_settings(settings: &mut Value) {
    if !settings.is_object() {
        *settings = Value::Object(Map::new());
    }
    let obj = settings.as_object_mut().expect("object");
    for key in ["files.watcherExclude", "search.exclude"] {
        let entry = obj.entry(key).or_insert_with(|| json!({}));
        if !entry.is_object() {
            *entry = json!({});
        }
        let map = entry.as_object_mut().expect("object");
        for g in EXCLUDE_GLOBS {
            map.insert(g.to_string(), Value::Bool(true));
        }
    }
    obj.insert("typescript.tsserver.maxTsServerMemory".into(), json!(2048));
}

fn apply_vscode_excludes(home: &Path) -> Result<String, String> {
    let path = vscode_settings_path(home);
    let mut settings = if path.exists() {
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        if text.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&text).map_err(|_| {
                "settings.json is not strict JSON (comments or trailing commas); add the excludes by hand"
                    .to_string()
            })?
        }
    } else {
        json!({})
    };
    merge_vscode_settings(&mut settings);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    Ok("VS Code excludes and tsserver memory written".into())
}

fn hibernate_mode() -> i64 {
    run_cmd("pmset", &["-g"])
        .and_then(|s| {
            s.lines().find_map(|l| {
                let mut it = l.split_whitespace();
                (it.next()? == "hibernatemode").then(|| it.next()?.parse().ok())?
            })
        })
        .unwrap_or(-1)
}

pub fn freshen_status(home: &Path) -> FreshenStatus {
    let spotlight = run_cmd("mdutil", &["-s", "/System/Volumes/Data"])
        .or_else(|| run_cmd("mdutil", &["-s", "/"]))
        .map(|s| s.contains("Indexing enabled"))
        .unwrap_or(false);
    let chrome = pgrep(&["-x", "Google Chrome"]);
    let vscode = std::fs::read_to_string(vscode_settings_path(home))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .map(|v| vscode_excludes_present(&v))
        .unwrap_or(false);
    FreshenStatus {
        hibernate_mode: hibernate_mode(),
        spotlight_dev_volume: spotlight,
        chrome_running: chrome,
        orbstack_running: orbstack_running(),
        vscode_excludes: vscode,
    }
}

pub fn freshen(action: &str, home: &Path) -> FreshenResult {
    let res: Result<String, String> = match action {
        "thin_snapshots" => std::process::Command::new("tmutil")
            .args(["thinlocalsnapshots", "/", "999999999999", "4"])
            .output()
            .map_err(|e| e.to_string())
            .and_then(|o| {
                if o.status.success() {
                    Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
                } else {
                    Err(String::from_utf8_lossy(&o.stderr).trim().to_string())
                }
            }),
        "hibernate_off" => admin_shell("pmset -a hibernatemode 0").map(|_| "hibernatemode set to 0".into()),
        "hibernate_default" => {
            admin_shell("pmset -a hibernatemode 3").map(|_| "hibernatemode set to 3".into())
        }
        "vscode_excludes" => apply_vscode_excludes(home),
        "purge_inactive" => admin_shell("purge").map(|_| "inactive memory purged".into()),
        "restart_prompt" => restart_prompt(),
        other => Err(format!("unknown action {other}")),
    };
    match res {
        Ok(message) => FreshenResult { ok: true, message },
        Err(message) => FreshenResult { ok: false, message },
    }
}

/// Ask loginwindow for the standard macOS restart dialog. The user can cancel it there;
/// left alone, macOS restarts when its countdown runs out.
fn restart_prompt() -> Result<String, String> {
    let out = std::process::Command::new("osascript")
        .args(["-e", "tell application \"loginwindow\" to «event aevtrrst»"])
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok("Restart dialog opened".into())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

pub fn reveal(path: &str) -> Result<(), String> {
    let st = std::process::Command::new("open").arg("-R").arg(path).status().map_err(|e| e.to_string())?;
    if st.success() {
        Ok(())
    } else {
        Err(format!("open -R exited {st}"))
    }
}

const LAUNCH_LABEL: &str = "com.nith.jetsam.weekly";

/// The label builds before the rename installed the weekly job under.
const LEGACY_LAUNCH_LABEL: &str = "com.fresh.weekly";

fn launch_agent_path(home: &Path) -> PathBuf {
    home.join("Library/LaunchAgents").join(format!("{LAUNCH_LABEL}.plist"))
}

/// Unload and delete the pre-rename weekly job if it is still installed. It points at a
/// binary that no longer exists, so leaving it would only fail every Sunday.
pub fn remove_legacy_launch_agent(home: &Path) {
    let plist = home.join("Library/LaunchAgents").join(format!("{LEGACY_LAUNCH_LABEL}.plist"));
    if !plist.exists() {
        return;
    }
    let domain = format!("gui/{}", real_uid());
    let _ = run_cmd("launchctl", &["bootout", &domain, &plist.to_string_lossy()]);
    let _ = std::fs::remove_file(&plist);
}

pub fn launch_agent_plist(binary: &Path, mode: &str, log: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LAUNCH_LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{bin}</string>
    <string>--headless</string>
    <string>--mode</string>
    <string>{mode}</string>
  </array>
  <key>StartCalendarInterval</key>
  <dict>
    <key>Weekday</key><integer>0</integer>
    <key>Hour</key><integer>10</integer>
    <key>Minute</key><integer>0</integer>
  </dict>
  <key>StandardOutPath</key><string>{log}</string>
  <key>StandardErrorPath</key><string>{log}</string>
</dict>
</plist>
"#,
        bin = binary.display(),
        log = log.display()
    )
}

/// Install or remove the weekly launchd job. Only `light` and `standard` may be
/// scheduled: the headless runner refuses to clean in deeper modes without `--dry-run`,
/// so scheduling one would only produce a failing job.
pub fn schedule(enabled: bool, mode: &str) -> Result<bool, String> {
    let mode = crate::types::Mode::parse(mode).ok_or("bad mode")?;
    if mode > crate::types::Mode::Standard {
        return Err(format!("scheduled runs are light or standard only, not {}", mode.as_str()));
    }
    let home = rules::home();
    remove_legacy_launch_agent(&home);
    let plist = launch_agent_path(&home);
    let domain = format!("gui/{}", real_uid());
    let _ = run_cmd("launchctl", &["bootout", &domain, &plist.to_string_lossy()]);
    if !enabled {
        if plist.exists() {
            std::fs::remove_file(&plist).map_err(|e| e.to_string())?;
        }
        return Ok(false);
    }
    let bin = std::env::current_exe().map_err(|e| e.to_string())?;
    let log = home.join("Library/Logs/Jetsam/weekly.log");
    for dir in [plist.parent(), log.parent()].into_iter().flatten() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&plist, launch_agent_plist(&bin, mode.as_str(), &log)).map_err(|e| e.to_string())?;
    run_cmd("launchctl", &["bootstrap", &domain, &plist.to_string_lossy()])
        .ok_or("launchctl bootstrap failed")?;
    Ok(true)
}

/// Full Disk Access probe: TCC-protected dirs are unreadable (EPERM) without it.
pub fn fda_granted(home: &Path) -> bool {
    ["Library/Mail", "Library/Safari"].iter().all(|rel| {
        let dir = home.join(rel);
        if !dir.exists() {
            return true;
        }
        match std::fs::read_dir(&dir) {
            Ok(_) => true,
            Err(e) => e.kind() != std::io::ErrorKind::PermissionDenied,
        }
    })
}

pub fn open_fda_settings() -> Result<(), String> {
    let status = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("open exited with {status}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vscode_merge_preserves_keys() {
        let mut v = json!({"editor.fontSize": 14, "files.watcherExclude": {"**/foo/**": true}});
        assert!(!vscode_excludes_present(&v));
        merge_vscode_settings(&mut v);
        assert_eq!(v["editor.fontSize"], 14);
        assert_eq!(v["files.watcherExclude"]["**/foo/**"], true);
        assert_eq!(v["files.watcherExclude"]["**/node_modules/**"], true);
        assert_eq!(v["search.exclude"]["**/.git/objects/**"], true);
        assert_eq!(v["typescript.tsserver.maxTsServerMemory"], 2048);
        assert!(vscode_excludes_present(&v));
    }

    #[test]
    fn plist_shape() {
        let p = launch_agent_plist(
            Path::new("/Applications/Jetsam.app/Contents/MacOS/jetsam"),
            "light",
            Path::new("/tmp/f.log"),
        );
        assert!(p.contains("<key>Label</key><string>com.nith.jetsam.weekly</string>"));
        assert!(p.contains("<string>--headless</string>"));
        assert!(p.contains("<string>light</string>"));
        assert!(p.contains("<key>Weekday</key><integer>0</integer>"));
    }

    #[test]
    fn schedule_refuses_deep_modes() {
        for m in ["deep", "full", "nuke"] {
            let err = schedule(true, m).unwrap_err();
            assert!(err.contains("light or standard"), "{m}: {err}");
        }
        assert_eq!(schedule(true, "bogus"), Err("bad mode".to_string()));
    }

    #[test]
    fn session_id_of_self() {
        assert!(session_id(std::process::id()).is_some());
        assert_eq!(session_id(u32::MAX), None, "not a valid pid");
    }
}
