//! Tauri command surface. Names and JSON shapes are the UI contract.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::categories;
use crate::cleaner::{self, ScanStore};
use crate::ram::{self, SharedSampler};
use crate::rules;
use crate::scanner::{Progress, ProgressSink};
use crate::types::{
    CleanResult, FdaStatus, FreshenResult, FreshenStatus, Item, MemoryInfo, Mode, ProcRow, ReclaimResult,
    ScanResult, UndoResult,
};
use crate::{aged as aged_mod, memory as memory_mod, system};

#[derive(Default)]
pub struct AppState {
    pub store: ScanStore,
    pub sampler: SharedSampler,
}

#[tauri::command]
pub async fn scan(
    app: AppHandle,
    state: State<'_, AppState>,
    mode: String,
    recent_days: Option<u32>,
) -> Result<ScanResult, String> {
    let mode = Mode::parse(&mode).ok_or_else(|| format!("unknown mode {mode}"))?;
    let recent = recent_days.unwrap_or(rules::DEFAULT_RECENT_DAYS);
    let emitter = app.clone();
    let sink: ProgressSink = Arc::new(move |p| {
        let _ = emitter.emit("scan:progress", &p);
    });
    let result =
        tauri::async_runtime::spawn_blocking(move || categories::run_scan(mode, recent, Progress::new(sink)))
            .await
            .map_err(|e| e.to_string())?;
    state.store.put(&result);
    Ok(result)
}

#[tauri::command]
pub async fn clean(
    app: AppHandle,
    state: State<'_, AppState>,
    scan_id: String,
    item_ids: Vec<String>,
    permanent: bool,
) -> Result<CleanResult, String> {
    let items = state.store.select(&scan_id, &item_ids)?;
    let batch = format!("{scan_id}:{}", crate::scanner::now());
    let emitter = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        cleaner::clean(&items, permanent, &rules::home(), &batch, |p| {
            let _ = emitter.emit("clean:progress", &p);
        })
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn memory(state: State<'_, AppState>) -> Result<MemoryInfo, String> {
    let sampler = state.sampler.clone();
    tauri::async_runtime::spawn_blocking(move || memory_mod::snapshot(ram::history(&sampler)))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn mem_processes(state: State<'_, AppState>) -> Result<Vec<ProcRow>, String> {
    let sampler = state.sampler.clone();
    tauri::async_runtime::spawn_blocking(move || ram::rows(&sampler)).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn mem_reclaim(
    state: State<'_, AppState>,
    tier: String,
    pids: Vec<u32>,
) -> Result<ReclaimResult, String> {
    let sampler = state.sampler.clone();
    tauri::async_runtime::spawn_blocking(move || ram::reclaim(&sampler, &tier, &pids))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn soft_restart(state: State<'_, AppState>, app: String) -> Result<FreshenResult, String> {
    let sampler = state.sampler.clone();
    tauri::async_runtime::spawn_blocking(move || ram::soft_restart(&sampler, &app, &rules::home()))
        .await
        .map_err(|e| e.to_string())
}

/// Stored under `cleaner::AGED_SCAN_ID`, so `clean("aged", ...)` verifies these like any scan.
#[tauri::command]
pub async fn aged(
    state: State<'_, AppState>,
    min_bytes: u64,
    min_age_days: Option<u32>,
) -> Result<Vec<Item>, String> {
    let min_age_days = min_age_days.unwrap_or(aged_mod::DEFAULT_MIN_AGE_DAYS);
    let items = tauri::async_runtime::spawn_blocking(move || {
        aged_mod::aged(min_bytes, min_age_days, crate::scanner::now(), &Progress::noop())
    })
    .await
    .map_err(|e| e.to_string())?;
    state.store.put_aged(&items);
    Ok(items)
}

#[tauri::command]
pub async fn freshen_status() -> Result<FreshenStatus, String> {
    tauri::async_runtime::spawn_blocking(|| system::freshen_status(&rules::home()))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn freshen(action: String) -> Result<FreshenResult, String> {
    tauri::async_runtime::spawn_blocking(move || system::freshen(&action, &rules::home()))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reveal(path: String) -> Result<(), String> {
    system::reveal(&path)
}

#[tauri::command]
pub async fn undo_last() -> Result<UndoResult, String> {
    tauri::async_runtime::spawn_blocking(|| cleaner::undo_last(&rules::home()))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn schedule(enabled: bool, mode: String) -> Result<bool, String> {
    system::schedule(enabled, &mode)
}

#[tauri::command]
pub async fn fda_status() -> Result<FdaStatus, String> {
    tauri::async_runtime::spawn_blocking(|| FdaStatus { granted: system::fda_granted(&rules::home()) })
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_fda_settings() -> Result<(), String> {
    system::open_fda_settings()
}
