import { createContext, useContext } from "react";
import type { CleanFailureDetail, CleanProgress, ScanMode, ScanProgress, ScanResult, ScheduleMode } from "./types";
import type { Page } from "./lib/pages";

export interface Settings {
  recentDays: number;
  scheduleEnabled: boolean;
  scheduleMode: ScheduleMode;
}
export interface LastClean {
  at: number;
  freedBytes: number;
}
export interface Toast {
  id: number;
  text: string;
  kind?: "info" | "error" | "success";
  action?: { label: string; run: () => void };
}

export interface Store {
  page: Page;
  setPage: (p: Page) => void;
  mode: ScanMode;
  setMode: (m: ScanMode) => void;
  settings: Settings;
  setSettings: (s: Partial<Settings>) => void;
  scan: ScanResult | null;
  scanning: boolean;
  scanProgress: ScanProgress | null;
  scanError: string | null;
  runScan: () => Promise<void>;
  selected: Set<string>;
  toggle: (id: string) => void;
  setMany: (ids: string[], on: boolean) => void;
  clearSelection: () => void;
  selectedBytes: number;
  selectedCount: number;
  cleaning: boolean;
  cleanProgress: CleanProgress | null;
  runClean: (permanent: boolean) => Promise<void>;
  /** Re-runs a clean of these scan items with permanent deletion, for items the Trash refused. */
  deletePermanently: (ids: string[]) => Promise<void>;
  lastClean: LastClean | null;
  toasts: Toast[];
  pushToast: (t: Omit<Toast, "id">) => number;
  dismissToast: (id: number) => void;
  /** Shows a success toast with Undo, replacing any older Undo toast, since Undo only reaches the newest batch. */
  offerUndo: (text: string) => void;
  /** Hides the current Undo toast; call before any other clean so Undo cannot reach the wrong batch. */
  retireUndo: () => void;
  cleanFailures: CleanFailureDetail[] | null;
  showCleanFailures: (f: CleanFailureDetail[] | null) => void;
  fdaGranted: boolean | null;
  recheckFda: () => Promise<void>;
}

export const StoreContext = createContext<Store | null>(null);

export const useStore = () => {
  const s = useContext(StoreContext);
  if (!s) throw new Error("useStore must be used inside StoreProvider");
  return s;
};
