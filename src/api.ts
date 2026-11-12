/**
 * The only module that talks to the Tauri backend.
 *
 * Real mode: `api.*` calls `invoke()` on the Rust commands in `src-tauri/`, and `listen()` subscribes to the
 * progress events the scanner and cleaner emit. Command names and payload shapes mirror
 * `src-tauri/src/commands.rs`; `types.ts` is the shared contract.
 *
 * Mock mode: with `VITE_MOCK=1`, or when the dev server is opened in a plain browser, every command is served
 * by `src/mock`. That module is imported dynamically behind compile-time constants, so a production build
 * contains none of it.
 */
import type {
  CleanProgress,
  CleanResult,
  FdaStatus,
  FreshenAction,
  FreshenStatus,
  Item,
  MemoryInfo,
  ProcRow,
  ReclaimResult,
  ReclaimTier,
  ScanMode,
  ScanProgress,
  ScanResult,
  ScheduleMode,
  SoftRestartApp,
  SoftRestartResult,
  UndoResult,
} from "./types";

export const MOCK = import.meta.env.VITE_MOCK === "1" || (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window));

type Unlisten = () => void;
export type Events = { "scan:progress": ScanProgress; "clean:progress": CleanProgress };

const mock = MOCK ? import("./mock") : null;

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (mock) return (await mock).mockInvoke<T>(cmd, args);
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

export async function listen<K extends keyof Events>(name: K, cb: (p: Events[K]) => void): Promise<Unlisten> {
  if (mock) return (await mock).mockListen(name, cb);
  const { listen: tauriListen } = await import("@tauri-apps/api/event");
  return tauriListen<Events[K]>(name, (e) => cb(e.payload));
}

async function appVersion(): Promise<string> {
  if (mock) return (await mock).mockVersion();
  const { getVersion } = await import("@tauri-apps/api/app");
  return getVersion();
}

export const api = {
  scan: (mode: ScanMode, recentDays: number) => call<ScanResult>("scan", { mode, recentDays }),
  clean: (scanId: string, itemIds: string[], permanent: boolean) =>
    call<CleanResult>("clean", { scanId, itemIds, permanent }),
  memory: () => call<MemoryInfo>("memory"),
  memProcesses: () => call<ProcRow[]>("mem_processes"),
  memReclaim: (tier: ReclaimTier, pids: number[]) => call<ReclaimResult>("mem_reclaim", { tier, pids }),
  softRestart: (app: SoftRestartApp) => call<SoftRestartResult>("soft_restart", { app }),
  aged: (minBytes: number, minAgeDays: number) => call<Item[]>("aged", { minBytes, minAgeDays }),
  freshenStatus: () => call<FreshenStatus>("freshen_status"),
  freshen: (action: FreshenAction) => call<{ ok: boolean; message: string }>("freshen", { action }),
  reveal: (path: string) => call<void>("reveal", { path }),
  undoLast: () => call<UndoResult>("undo_last"),
  schedule: (enabled: boolean, mode: ScheduleMode) => call<boolean>("schedule", { enabled, mode }),
  fdaStatus: () => call<FdaStatus>("fda_status"),
  openFdaSettings: () => call<void>("open_fda_settings"),
  appVersion,
};
