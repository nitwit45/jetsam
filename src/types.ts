export type ScanMode = "light" | "standard" | "deep" | "full";
export type ItemKind = "dir" | "file" | "command";
export type Flag = "dirty" | "unmerged" | "recent" | "git-tracked" | "review" | "report-only" | "worktree";

export interface Item {
  id: string;
  path: string;
  bytes: number;
  mtime: number; // epoch seconds
  kind: ItemKind;
  repo?: string;
  repoLastActive?: number;
  /** When the file was last opened (epoch seconds). Sent by the Aged view only. */
  lastUsed?: number;
  flags: string[];
}

export interface Category {
  id: string;
  label: string;
  mode: ScanMode;
  totalBytes: number;
  items: Item[];
}

export interface ScanResult {
  id: string;
  diskTotal: number;
  diskFree: number;
  categories: Category[];
}

export interface ScanProgress {
  scanned: number;
  bytes: number;
  current: string;
}
export interface CleanProgress {
  done: number;
  total: number;
  freedBytes: number;
}
export interface CleanResult {
  freedBytes: number;
  failed: { id: string; reason: string }[];
}

/** The scan id the backend stores Aged results under, so `clean("aged", ...)` verifies them like any scan. */
export const AGED_SCAN_ID = "aged";

/** Failure reason the cleaner uses when the Trash would not take an item. The item is still on disk. */
export const TRASH_REFUSED = "trash-refused";

export interface UndoResult {
  restored: number;
  skipped: number;
}

export interface MemoryInfo {
  totalBytes: number;
  freePct: number;
  swapUsed: number;
  swapTotal: number;
  compressedBytes: number;
  pageouts: number;
  pressure: "normal" | "warn" | "critical";
  top: { pid: number; rss: number; name: string }[];
  uptimeSec: number;
  history: HistPoint[];
}

export interface HistPoint {
  ts: number;
  freePct: number;
  swapUsed: number;
}

export type ProcReason = "orphan" | "idle" | "leak" | "vm" | "protected" | "headless";
export interface ProcRow {
  pid: number;
  ppid: number;
  name: string;
  app: string;
  rss: number;
  cpuPct: number;
  elapsedSec: number;
  reasons: string[];
  host?: "vscode" | "cursor";
}

export type ReclaimTier = "safe" | "aggressive";
export interface ReclaimAction {
  label: string;
  ok: boolean;
  detail: string;
}
export interface ReclaimResult {
  freedBytes: number;
  freePctDelta: number;
  swapDelta: number;
  before: MemoryInfo;
  after: MemoryInfo;
  actions: ReclaimAction[];
}

export type SoftRestartApp = "chrome" | "vscode" | "cursor";
export interface SoftRestartResult {
  ok: boolean;
  message: string;
}

export interface FreshenStatus {
  hibernateMode: number;
  spotlightDevVolume: boolean;
  chromeRunning: boolean;
  orbstackRunning: boolean;
  vscodeExcludes: boolean;
}

export type FreshenAction =
  "thin_snapshots" | "hibernate_off" | "hibernate_default" | "vscode_excludes" | "purge_inactive" | "restart_prompt";

/** Flags that lock an item out of selection entirely. */
export const BLOCKING_FLAGS = new Set<string>(["report-only", "dirty", "unmerged", "git-tracked", "recent"]);
export const isSelectable = (i: Item) => !i.flags.some((f) => BLOCKING_FLAGS.has(f));
/** Selectable and ticked for you after a scan. A `review` item can be picked by hand but never for you. */
export const isPreselectable = (i: Item) => isSelectable(i) && !i.flags.includes("review");

export interface FdaStatus {
  granted: boolean;
}
export interface CleanFailureDetail {
  id: string;
  path: string;
  bytes: number;
  reason: string;
}

/** Only these modes can run unattended; Deep and Full need a person to read the preview. */
export type ScheduleMode = "light" | "standard";
export const SCHEDULE_MODES: ScheduleMode[] = ["light", "standard"];
