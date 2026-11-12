import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { api, listen, MOCK } from "./api";
import {
  isPreselectable,
  SCHEDULE_MODES,
  TRASH_REFUSED,
  type CleanFailureDetail,
  type CleanProgress,
  type CleanResult,
  type Item,
  type ScanMode,
  type ScanProgress,
  type ScanResult,
} from "./types";
import { StoreContext, type LastClean, type Settings, type Store, type Toast } from "./storeContext";
import { isPage, type Page } from "./lib/pages";
import { fmtBytes } from "./lib/format";

const SETTINGS_KEY = "jetsam.settings";
const LAST_KEY = "jetsam.lastClean";
const DEFAULT_SETTINGS: Settings = { recentDays: 14, scheduleEnabled: false, scheduleMode: "standard" };

function loadSettings(): Settings {
  try {
    const saved: Settings = { ...DEFAULT_SETTINGS, ...JSON.parse(localStorage.getItem(SETTINGS_KEY) || "{}") };
    // Older builds let Deep be scheduled; the backend never ran it, so fall back to the default.
    if (!SCHEDULE_MODES.includes(saved.scheduleMode)) return { ...saved, scheduleMode: DEFAULT_SETTINGS.scheduleMode };
    return saved;
  } catch {
    return DEFAULT_SETTINGS;
  }
}

const errText = (e: unknown) => (e instanceof Error ? e.message : String(e));
const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

/** What disk space a clean gives back now. Trashed files still use space until the Trash is emptied. */
function bytesFreedOnDisk(cleaned: Item[], permanent: boolean): number {
  return cleaned.filter((i) => permanent || i.kind === "command").reduce((a, i) => a + i.bytes, 0);
}

function withoutItems(scan: ScanResult, gone: Set<string>, freedOnDisk: number): ScanResult {
  return {
    ...scan,
    diskFree: scan.diskFree + freedOnDisk,
    categories: scan.categories
      .map((c) => {
        const items = c.items.filter((i) => !gone.has(i.id));
        return { ...c, items, totalBytes: items.reduce((a, i) => a + i.bytes, 0) };
      })
      .filter((c) => c.items.length > 0),
  };
}

function cleanSummary(r: CleanResult, permanent: boolean, trashedFiles: boolean): string {
  const bytes = fmtBytes(r.freedBytes);
  if (r.failed.length === 0) {
    if (permanent) return `Deleted ${bytes} permanently`;
    return trashedFiles ? `Cleaned ${bytes}. Files are in the Trash until you empty it.` : `Cleaned ${bytes}`;
  }
  const refused = r.failed.filter((f) => f.reason === TRASH_REFUSED).length;
  const why = refused < r.failed.length ? "see Details" : `the Trash refused ${refused === 1 ? "it" : "them"}`;
  return `Cleaned ${bytes} · ${plural(r.failed.length, "item")} left in place, ${why}`;
}
function loadLast(): LastClean | null {
  try {
    return JSON.parse(localStorage.getItem(LAST_KEY) || "null");
  } catch {
    return null;
  }
}

export function StoreProvider({ children }: { children: ReactNode }) {
  const [page, setPageState] = useState<Page>(() => {
    const h = location.hash.slice(1);
    return isPage(h) ? h : "overview";
  });
  const setPage = useCallback((p: Page) => {
    setPageState(p);
    history.replaceState(null, "", `#${p}`);
  }, []);
  const [mode, setMode] = useState<ScanMode>("standard");
  const [settings, setSettingsState] = useState<Settings>(loadSettings);
  const [scan, setScan] = useState<ScanResult | null>(null);
  const [scanning, setScanning] = useState(false);
  const [scanProgress, setScanProgress] = useState<ScanProgress | null>(null);
  const [scanError, setScanError] = useState<string | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [cleaning, setCleaning] = useState(false);
  const [cleanProgress, setCleanProgress] = useState<CleanProgress | null>(null);
  const [lastClean, setLastClean] = useState<LastClean | null>(loadLast);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const toastId = useRef(0);
  const [cleanFailures, showCleanFailures] = useState<CleanFailureDetail[] | null>(null);
  const [fdaGranted, setFdaGranted] = useState<boolean | null>(null);

  const recheckFda = useCallback(async () => {
    try {
      setFdaGranted((await api.fdaStatus()).granted);
    } catch {
      setFdaGranted(null);
    }
  }, []);
  useEffect(() => {
    void recheckFda();
    const onFocus = () => void recheckFda();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [recheckFda]);

  const setSettings = useCallback(
    (p: Partial<Settings>) =>
      setSettingsState((s) => {
        const n = { ...s, ...p };
        localStorage.setItem(SETTINGS_KEY, JSON.stringify(n));
        return n;
      }),
    [],
  );

  const pushToast = useCallback((t: Omit<Toast, "id">) => {
    const id = ++toastId.current;
    setToasts((ts) => [...ts, { ...t, id }]);
    setTimeout(() => setToasts((ts) => ts.filter((x) => x.id !== id)), t.action ? 12000 : 5000);
    return id;
  }, []);
  const dismissToast = useCallback((id: number) => setToasts((ts) => ts.filter((x) => x.id !== id)), []);

  // Undo always acts on the newest batch, so only the newest clean may show an Undo button.
  const undoToast = useRef<number | null>(null);
  const retireUndo = useCallback(() => {
    if (undoToast.current !== null) dismissToast(undoToast.current);
    undoToast.current = null;
  }, [dismissToast]);
  const undoLast = useCallback(async () => {
    undoToast.current = null;
    try {
      const { restored, skipped } = await api.undoLast();
      const back = `Put back ${plural(restored, "item")} from the Trash.`;
      const rest = skipped
        ? ` Skipped ${skipped}: no longer in the Trash, or something is already at the original path.`
        : "";
      pushToast({ kind: skipped ? "error" : "info", text: back + rest });
    } catch (e) {
      pushToast({ kind: "error", text: `Undo failed: ${errText(e)}` });
    }
  }, [pushToast]);
  const offerUndo = useCallback(
    (text: string) => {
      retireUndo();
      undoToast.current = pushToast({ kind: "success", text, action: { label: "Undo", run: () => void undoLast() } });
    },
    [retireUndo, pushToast, undoLast],
  );

  useEffect(() => {
    const subs = [listen("scan:progress", setScanProgress), listen("clean:progress", setCleanProgress)];
    return () => {
      subs.forEach((p) => p.then((u) => u()));
    };
  }, []);

  const runScan = useCallback(async () => {
    if (scanning || cleaning) return;
    setScanning(true);
    setScanError(null);
    setScanProgress(null);
    try {
      const r = await api.scan(mode, settings.recentDays);
      setScan(r);
      setSelected(new Set(r.categories.flatMap((c) => c.items.filter(isPreselectable).map((i) => i.id))));
    } catch (e) {
      setScanError(String(e instanceof Error ? e.message : e));
    } finally {
      setScanning(false);
    }
  }, [mode, settings.recentDays, scanning, cleaning]);

  const autoScanned = useRef(false);
  useEffect(() => {
    if (autoScanned.current || !MOCK || !location.search.includes("autoscan")) return;
    autoScanned.current = true;
    void runScan();
  }, [runScan]);

  const toggle = useCallback(
    (id: string) =>
      setSelected((s) => {
        const n = new Set(s);
        if (n.has(id)) n.delete(id);
        else n.add(id);
        return n;
      }),
    [],
  );
  const setMany = useCallback(
    (ids: string[], on: boolean) =>
      setSelected((s) => {
        const n = new Set(s);
        ids.forEach((i) => (on ? n.add(i) : n.delete(i)));
        return n;
      }),
    [],
  );
  const clearSelection = useCallback(() => setSelected(new Set()), []);

  const { selectedBytes, selectedCount } = useMemo(() => {
    let b = 0,
      c = 0;
    scan?.categories.forEach((cat) =>
      cat.items.forEach((i) => {
        if (selected.has(i.id)) {
          b += i.bytes;
          c++;
        }
      }),
    );
    return { selectedBytes: b, selectedCount: c };
  }, [scan, selected]);

  const applyClean = useCallback(
    async (ids: string[], permanent: boolean) => {
      if (!scan || cleaning || ids.length === 0) return;
      setCleaning(true);
      setCleanProgress({ done: 0, total: ids.length, freedBytes: 0 });
      retireUndo();
      try {
        const r = await api.clean(scan.id, ids, permanent);
        const failed = new Set(r.failed.map((f) => f.id));
        const byId = new Map(scan.categories.flatMap((c) => c.items.map((i) => [i.id, i] as const)));
        const cleaned = ids.filter((id) => !failed.has(id)).flatMap((id) => byId.get(id) ?? []);
        const gone = new Set(cleaned.map((i) => i.id));
        setScan(withoutItems(scan, gone, bytesFreedOnDisk(cleaned, permanent)));
        setSelected((s) => new Set([...s].filter((id) => !gone.has(id))));
        const lc = { at: Date.now(), freedBytes: r.freedBytes };
        setLastClean(lc);
        localStorage.setItem(LAST_KEY, JSON.stringify(lc));

        const trashedFiles = !permanent && cleaned.some((i) => i.kind !== "command");
        const text = cleanSummary(r, permanent, trashedFiles);
        const details: CleanFailureDetail[] = r.failed.map((f) => ({
          id: f.id,
          path: byId.get(f.id)?.path ?? f.id,
          bytes: byId.get(f.id)?.bytes ?? 0,
          reason: f.reason.length > 160 ? `${f.reason.slice(0, 160)}…` : f.reason,
        }));
        if (details.length)
          pushToast({ kind: "error", text, action: { label: "Details", run: () => showCleanFailures(details) } });
        else if (trashedFiles) offerUndo(text);
        else pushToast({ kind: "success", text });
      } catch (e) {
        pushToast({ kind: "error", text: `Clean failed: ${errText(e)}` });
      } finally {
        setCleaning(false);
        setCleanProgress(null);
      }
    },
    [scan, cleaning, retireUndo, pushToast, offerUndo],
  );
  const runClean = useCallback((permanent: boolean) => applyClean([...selected], permanent), [applyClean, selected]);
  const deletePermanently = useCallback(
    async (ids: string[]) => {
      showCleanFailures(null);
      await applyClean(ids, true);
    },
    [applyClean],
  );

  const value: Store = {
    page,
    setPage,
    mode,
    setMode,
    settings,
    setSettings,
    scan,
    scanning,
    scanProgress,
    scanError,
    runScan,
    selected,
    toggle,
    setMany,
    clearSelection,
    selectedBytes,
    selectedCount,
    cleaning,
    cleanProgress,
    runClean,
    deletePermanently,
    lastClean,
    toasts,
    pushToast,
    dismissToast,
    offerUndo,
    retireUndo,
    cleanFailures,
    showCleanFailures,
    fdaGranted,
    recheckFda,
  };
  return <StoreContext.Provider value={value}>{children}</StoreContext.Provider>;
}
