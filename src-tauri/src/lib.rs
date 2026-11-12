//! Jetsam: a macOS disk and memory cleaner for developers. This crate is the whole
//! backend: scanning (`categories`, `scanner`, `rules`), deleting (`cleaner`), memory
//! (`memory`, `ram`), system actions (`system`) and the Tauri command surface
//! (`commands`). `run` starts the app; `run_headless` is the CLI path.

pub mod aged;
pub mod categories;
pub mod cleaner;
pub mod commands;
pub mod memory;
pub mod ram;
pub mod rules;
pub mod scanner;
pub mod system;
pub mod types;
pub mod worktrees;

use tauri::Manager;
use types::Mode;

/// Apps launched from Finder or launchd inherit a minimal PATH, so brew, docker and bun
/// would be invisible without this.
fn extend_path() {
    let home = std::env::var("HOME").unwrap_or_default();
    let extra =
        format!("/opt/homebrew/bin:/usr/local/bin:{home}/.local/bin:{home}/.cargo/bin:{home}/.bun/bin");
    let cur = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{extra}:{cur}"));
}

/// One-time moves from the pre-rename layout: the support folder and the old launchd job.
fn migrate_legacy() {
    let home = rules::home();
    cleaner::migrate_legacy_support_dir(&home);
    system::remove_legacy_launch_agent(&home);
}

pub fn run() {
    extend_path();
    migrate_legacy();
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(commands::AppState::default())
        .setup(|app| {
            let sampler = app.state::<commands::AppState>().sampler.clone();
            std::thread::spawn(move || loop {
                ram::sample(&sampler);
                std::thread::sleep(ram::SAMPLE_INTERVAL);
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::scan,
            commands::clean,
            commands::memory,
            commands::mem_processes,
            commands::mem_reclaim,
            commands::soft_restart,
            commands::aged,
            commands::freshen_status,
            commands::freshen,
            commands::reveal,
            commands::undo_last,
            commands::schedule,
            commands::fda_status,
            commands::open_fda_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Jetsam");
}

fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

/// `jetsam --headless`: scan, trash what the app would preselect, print one summary line.
/// `review` items are left alone: a person decides on those. With `dry_run` nothing is
/// touched and a per-category breakdown is printed instead; `main` only lets `Deep` and
/// `Full` through with `dry_run` set.
pub fn run_headless(mode: Mode, dry_run: bool) -> String {
    extend_path();
    migrate_legacy();
    let result = categories::run_scan(mode, rules::DEFAULT_RECENT_DAYS, scanner::Progress::noop());
    let unattended = |i: &&types::Item| i.selectable() && !i.has_flag(types::flags::REVIEW);
    let items: Vec<types::Item> =
        result.categories.iter().flat_map(|c| c.items.iter().filter(unattended).cloned()).collect();
    let reclaimable: u64 = items.iter().map(|i| i.bytes).sum();
    if dry_run {
        let mut out = format!(
            "jetsam {} dry-run: {} reclaimable across {} items\n",
            mode.as_str(),
            human(reclaimable),
            items.len()
        );
        for c in &result.categories {
            out.push_str(&format!("  {:<18} {:>9}  {} items\n", c.id, human(c.total_bytes), c.items.len()));
        }
        return out;
    }
    let batch = format!("{}:{}", result.id, scanner::now());
    let cleaned = cleaner::clean(&items, false, &rules::home(), &batch, |_| {});
    format!(
        "jetsam {} scan: {} reclaimable, freed {} across {} items, {} failed",
        mode.as_str(),
        human(reclaimable),
        human(cleaned.freed_bytes),
        items.len() - cleaned.failed.len(),
        cleaned.failed.len()
    )
}
