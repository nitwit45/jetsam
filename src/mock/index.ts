/**
 * In-memory stand-in for the Rust backend, loaded only in mock mode (see `api.ts`).
 *
 * It keeps just enough state to behave like the real commands: scans are stored by id and `clean` refuses ids it
 * did not hand out, the Aged view is stored under the `aged` scan id, the last Trash batch can be undone, and
 * schedule refuses the modes the backend refuses. URL switches: `?autoscan` runs a scan on load, `?nofda`
 * pretends Full Disk Access is off, `?norestore` makes the Chrome soft restart refuse.
 */
import packageJson from "../../package.json?raw";
import type { Events } from "../api";
import {
  AGED_SCAN_ID,
  SCHEDULE_MODES,
  TRASH_REFUSED,
  type CleanResult,
  type FreshenAction,
  type FreshenStatus,
  type Item,
  type MemoryInfo,
  type ProcRow,
  type ReclaimResult,
  type ReclaimTier,
  type ScanMode,
  type ScanResult,
  type ScheduleMode,
  type SoftRestartApp,
  type SoftRestartResult,
  type UndoResult,
} from "../types";
import { AGED_FILES, GB, HOME, MB, MOCK_PROCS, PROJECTS, daysAgo, mockCategories } from "./fixtures";

const listeners: { [K in keyof Events]: Set<(p: Events[K]) => void> } = {
  "scan:progress": new Set(),
  "clean:progress": new Set(),
};

function emit<K extends keyof Events>(name: K, p: Events[K]) {
  listeners[name].forEach((cb) => cb(p));
}

export function mockListen<K extends keyof Events>(name: K, cb: (p: Events[K]) => void): () => void {
  listeners[name].add(cb);
  return () => listeners[name].delete(cb);
}

export function mockVersion(): string {
  return (JSON.parse(packageJson) as { version: string }).version;
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
let seed = 7;
const rnd = () => {
  seed = (seed * 16807) % 2147483647;
  return seed / 2147483647;
};

const state = {
  scans: new Map<string, Map<string, Item>>(),
  lastTrashed: 0,
  freshen: {
    hibernateMode: 3,
    spotlightDevVolume: true,
    chromeRunning: true,
    orbstackRunning: true,
    vscodeExcludes: false,
  } as FreshenStatus,
  schedule: false,
  mem: { freePct: 11, swapUsed: 7.6 * GB, dead: new Set<number>() },
};

function store(id: string, list: Item[]) {
  state.scans.set(id, new Map(list.map((i) => [i.id, i])));
}

/** The simulator caches stand in for an item the Trash will not take, so the permanent follow-up can be tried. */
const trashRefuses = (it: Item) => it.path.includes("CoreSimulator");

async function scan(mode: ScanMode, recentDays: number): Promise<ScanResult> {
  for (let i = 0; i <= 1400; i += 70) {
    const project = PROJECTS[i % PROJECTS.length].name;
    emit("scan:progress", {
      scanned: i,
      bytes: i * 48 * MB,
      current: `${HOME}/Projects/${project}/node_modules/.cache/${i}`,
    });
    await sleep(45);
  }
  const result = {
    id: `scan-${mode}-${Date.now()}`,
    diskTotal: 994 * GB,
    diskFree: 112 * GB,
    categories: mockCategories(mode, recentDays),
  };
  store(
    result.id,
    result.categories.flatMap((c) => c.items),
  );
  return result;
}

async function clean(scanId: string, ids: string[], permanent: boolean): Promise<CleanResult> {
  const known = state.scans.get(scanId);
  if (!known) throw new Error(`unknown scan ${scanId}; rescan first`);
  const failed: CleanResult["failed"] = [];
  let freed = 0;
  let trashed = 0;
  for (const [n, id] of ids.entries()) {
    const it = known.get(id);
    if (!it) failed.push({ id, reason: "not in this scan; rescan first" });
    else if (!permanent && it.kind !== "command" && trashRefuses(it)) failed.push({ id, reason: TRASH_REFUSED });
    else {
      freed += it.bytes;
      if (!permanent && it.kind !== "command") trashed++;
      known.delete(id);
    }
    emit("clean:progress", { done: n + 1, total: ids.length, freedBytes: freed });
    await sleep(Math.min(120, 1500 / ids.length));
  }
  if (!permanent) state.lastTrashed = trashed;
  return { freedBytes: freed, failed };
}

function agedHint(path: string): string {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  if (["mov", "mp4", "mkv"].includes(ext)) return "video";
  if (["zip", "gz", "tar"].includes(ext)) return "archive";
  if (["dmg", "iso", "pkg"].includes(ext)) return "disk-image";
  if (["gguf", "safetensors", "onnx"].includes(ext)) return "model";
  return "other";
}

function aged(minBytes: number, minAgeDays: number): Item[] {
  const cutoff = daysAgo(minAgeDays);
  const list: Item[] = AGED_FILES.map(([path, bytes, usedDays, modifiedDays], i) => ({
    id: `aged-${i}`,
    path,
    bytes,
    mtime: daysAgo(modifiedDays),
    lastUsed: daysAgo(usedDays),
    kind: "file" as const,
    flags: [agedHint(path), "review"],
  }))
    .filter((it) => it.bytes >= minBytes && it.lastUsed < cutoff)
    .sort((a, b) => b.bytes - a.bytes);
  store(AGED_SCAN_ID, list);
  return list;
}

function undoLast(): UndoResult {
  const restored = state.lastTrashed;
  state.lastTrashed = 0;
  return { restored, skipped: 0 };
}

function schedule(enabled: boolean, mode: ScheduleMode): boolean {
  if (!SCHEDULE_MODES.includes(mode)) throw new Error(`scheduled runs are light or standard only, not ${mode}`);
  state.schedule = enabled;
  return state.schedule;
}

function memory(): MemoryInfo {
  const { freePct, swapUsed } = state.mem;
  const now = Math.floor(Date.now() / 1000);
  const history = Array.from({ length: 60 }, (_, i) => ({
    ts: now - (59 - i) * 10,
    freePct: Math.max(4, Math.min(60, freePct + 12 - i * 0.2 + Math.sin(i / 4) * 3)),
    swapUsed,
  }));
  return {
    totalBytes: 36 * GB,
    freePct,
    swapUsed,
    swapTotal: 10 * GB,
    compressedBytes: 5.9 * GB,
    pageouts: 1_284_301,
    pressure: freePct < 10 ? "critical" : freePct < 25 ? "warn" : "normal",
    uptimeSec: 6.3 * 86400,
    history,
    top: [
      { pid: 412, name: "Google Chrome Helper (Renderer)", rss: 3.1 * GB },
      { pid: 981, name: "Code Helper (Plugin)", rss: 2.4 * GB },
      { pid: 77, name: "OrbStack Helper (VM)", rss: 2.1 * GB },
      { pid: 2210, name: "Slack Helper", rss: 1.1 * GB },
      { pid: 311, name: "WindowServer", rss: 0.9 * GB },
      { pid: 5541, name: "node", rss: 0.8 * GB },
      { pid: 220, name: "Spotlight (mds_stores)", rss: 0.6 * GB },
      { pid: 8812, name: "Figma Helper", rss: 0.5 * GB },
    ].sort((a, b) => b.rss - a.rss),
  };
}

function processes(): ProcRow[] {
  return MOCK_PROCS.filter((r) => !state.mem.dead.has(r.pid)).map((r) => ({
    ...r,
    rss: r.rss * (0.97 + rnd() * 0.06),
  }));
}

async function reclaim(tier: ReclaimTier, pids: number[]): Promise<ReclaimResult> {
  const before = memory();
  await sleep(900);
  const actions: ReclaimResult["actions"] = [];
  let freed = 0;
  const end = (r: ProcRow, detail: string) => {
    state.mem.dead.add(r.pid);
    freed += r.rss;
    actions.push({ label: `End ${r.name} (${r.pid})`, ok: true, detail });
  };
  for (const r of MOCK_PROCS) if (r.reasons.includes("orphan") && !state.mem.dead.has(r.pid)) end(r, "Ended cleanly");
  if (tier === "aggressive")
    for (const pid of pids) {
      const r = MOCK_PROCS.find((x) => x.pid === pid);
      if (!r) actions.push({ label: `End pid ${pid}`, ok: false, detail: "Not running" });
      else if (r.reasons.includes("protected"))
        actions.push({ label: `End ${r.name} (${r.pid})`, ok: false, detail: "Refused: protected process" });
      else if (!state.mem.dead.has(pid)) end(r, pid % 2 ? "Did not exit in 3s, force-ended" : "Ended cleanly");
    }
  actions.push({ label: "Flush file cache", ok: true, detail: "purge ran" });
  await sleep(1500);
  state.mem.freePct = Math.min(80, state.mem.freePct + Math.round((freed / (36 * GB)) * 100));
  const swapDelta = Math.min(state.mem.swapUsed, freed * 0.35);
  state.mem.swapUsed -= swapDelta;
  const after = memory();
  return {
    freedBytes: freed + swapDelta,
    freePctDelta: after.freePct - before.freePct,
    swapDelta,
    before,
    after,
    actions,
  };
}

async function softRestart(app: SoftRestartApp): Promise<SoftRestartResult> {
  await sleep(1200);
  if (app === "vscode")
    return {
      ok: false,
      message:
        "1 Claude Code agent is running in Visual Studio Code. Restarting would stop it. Finish or stop it first.",
    };
  if (app === "chrome" && location.search.includes("norestore"))
    return {
      ok: false,
      message:
        'Chrome is not set to restore tabs. Turn on "Continue where you left off" at chrome://settings/onStartup first; nothing was quit.',
    };
  state.mem.freePct = Math.min(80, state.mem.freePct + 6);
  state.mem.swapUsed = Math.max(0, state.mem.swapUsed - 2.2 * GB);
  return {
    ok: true,
    message: app === "chrome" ? "Chrome relaunched with tabs restored" : "Cursor relaunched with windows restored",
  };
}

const FRESHEN_MESSAGES: Record<FreshenAction, string> = {
  thin_snapshots: "Thinned 4 local snapshots, reclaimed 18.2 GB",
  hibernate_off: "hibernatemode set to 0; sleepimage removed (≈36 GB)",
  hibernate_default: "hibernatemode restored to 3",
  vscode_excludes: "Added 6 watcher excludes to settings.json",
  purge_inactive: "Purged inactive memory",
  restart_prompt: "Restart dialog opened",
};

function freshen(action: FreshenAction) {
  if (action === "hibernate_off") state.freshen.hibernateMode = 0;
  if (action === "hibernate_default") state.freshen.hibernateMode = 3;
  if (action === "vscode_excludes") state.freshen.vscodeExcludes = true;
  return { ok: true, message: FRESHEN_MESSAGES[action] };
}

/** Same command names and argument keys as the Tauri handlers. */
export async function mockInvoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  return (await route(cmd, args)) as T;
}

async function route(cmd: string, args: Record<string, unknown>): Promise<unknown> {
  switch (cmd) {
    case "scan":
      return scan(args.mode as ScanMode, args.recentDays as number);
    case "clean":
      return clean(args.scanId as string, args.itemIds as string[], args.permanent as boolean);
    case "memory":
      await sleep(200);
      return memory();
    case "mem_processes":
      await sleep(150);
      return processes();
    case "mem_reclaim":
      return reclaim(args.tier as ReclaimTier, (args.pids as number[]) ?? []);
    case "soft_restart":
      return softRestart(args.app as SoftRestartApp);
    case "aged":
      await sleep(350);
      return aged(args.minBytes as number, args.minAgeDays as number);
    case "freshen_status":
      await sleep(150);
      return { ...state.freshen };
    case "freshen":
      await sleep(600);
      return freshen(args.action as FreshenAction);
    case "reveal":
      await sleep(80);
      return undefined;
    case "undo_last":
      await sleep(700);
      return undoLast();
    case "fda_status":
      await sleep(60);
      return { granted: !location.search.includes("nofda") };
    case "open_fda_settings":
      await sleep(80);
      return undefined;
    case "schedule":
      await sleep(120);
      return schedule(args.enabled as boolean, args.mode as ScheduleMode);
    default:
      throw new Error(`mock: unknown command ${cmd}`);
  }
}
