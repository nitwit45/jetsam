import {
  Clock,
  Gauge,
  LayoutDashboard,
  Settings as SettingsIcon,
  ShieldAlert,
  Sparkles,
  Trash2,
  X,
  type LucideIcon,
} from "lucide-react";
import { api } from "./api";
import { CleanFailuresModal } from "./components/clean/CleanModals";
import { useEffect } from "react";
import { StoreProvider } from "./store";
import { useStore, type Toast } from "./storeContext";
import type { Page } from "./lib/pages";
import Overview from "./pages/Overview";
import Clean from "./pages/Clean";
import Aged from "./pages/Aged";
import Memory from "./pages/Memory";
import Freshen from "./pages/Freshen";
import Settings from "./pages/Settings";

const NAV: { id: Page; label: string; icon: LucideIcon }[] = [
  { id: "overview", label: "Overview", icon: LayoutDashboard },
  { id: "clean", label: "Clean", icon: Trash2 },
  { id: "aged", label: "Aged Files", icon: Clock },
  { id: "memory", label: "Memory", icon: Gauge },
  { id: "freshen", label: "Freshen", icon: Sparkles },
  { id: "settings", label: "Settings", icon: SettingsIcon },
];

function Shell() {
  const {
    page,
    setPage,
    runScan,
    scanning,
    toasts,
    dismissToast,
    selectedCount,
    cleanFailures,
    showCleanFailures,
    deletePermanently,
    fdaGranted,
    recheckFda,
  } = useStore();

  useEffect(() => {
    const k = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "r") {
        e.preventDefault();
        void runScan();
      }
    };
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, [runScan]);

  const Current = {
    overview: Overview,
    clean: Clean,
    aged: Aged,
    memory: Memory,
    freshen: Freshen,
    settings: Settings,
  }[page];

  return (
    <div className="flex h-full">
      <aside className="flex w-[184px] shrink-0 flex-col border-r border-line bg-panel-2">
        <div data-tauri-drag-region className="h-7 shrink-0" />
        <div className="px-3 pb-2 pt-1 text-[15px] font-semibold tracking-tight">Jetsam</div>
        <nav className="flex flex-col gap-0.5 px-2">
          {NAV.map(({ id, label, icon: Icon }) => (
            <button
              key={id}
              onClick={() => setPage(id)}
              className={`flex h-7 items-center gap-2 rounded-md px-2 text-left transition ${page === id ? "bg-selected text-fg font-medium" : "text-fg-2 hover:bg-hover hover:text-fg"}`}
            >
              <Icon size={15} className={page === id ? "text-accent" : ""} />
              <span className="flex-1">{label}</span>
              {id === "clean" && selectedCount > 0 && (
                <span className="tnum rounded-full bg-accent px-1.5 text-[10px] font-semibold leading-4 text-white">
                  {selectedCount}
                </span>
              )}
              {id === "overview" && scanning && <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-accent" />}
            </button>
          ))}
        </nav>
        <div className="mt-auto px-4 pb-3 text-[11px] text-fg-3">⌘R to rescan</div>
      </aside>
      <main className="relative flex min-w-0 flex-1 flex-col">
        <div data-tauri-drag-region className="h-7 shrink-0" />
        {fdaGranted === false && (
          <div className="mx-6 mb-3 flex items-center gap-3 rounded-md border border-warn/50 bg-warn/10 px-3 py-2 text-xs">
            <ShieldAlert size={15} className="shrink-0 text-warn" />
            <div className="flex-1">
              Full Disk Access is off, so Mail, Safari, Messages and some caches are hidden from Jetsam. Everything else
              still works.
            </div>
            <button className="btn btn-sm" onClick={() => void api.openFdaSettings()}>
              Open System Settings
            </button>
            <button className="btn btn-sm" onClick={() => void recheckFda()}>
              Recheck
            </button>
          </div>
        )}
        <div className="min-h-0 flex-1 overflow-auto px-6 pb-6">
          <Current />
        </div>
      </main>
      {cleanFailures && (
        <CleanFailuresModal
          failures={cleanFailures}
          onClose={() => showCleanFailures(null)}
          onDeletePermanently={(ids) => void deletePermanently(ids)}
        />
      )}
      <div className="pointer-events-none fixed bottom-4 right-4 z-40 flex w-[380px] flex-col gap-2">
        {toasts.map((t) => (
          <div
            key={t.id}
            role="status"
            className={`pointer-events-auto fade-in card flex items-center gap-2 px-3 py-2 shadow-lg ${t.kind === "error" ? "border-danger/50" : t.kind === "success" ? "border-accent/50" : ""}`}
          >
            <div className="flex-1 select-text">{t.text}</div>
            {t.action && <ToastAction action={t.action} onDone={() => dismissToast(t.id)} />}
            <button className="btn btn-ghost btn-sm px-1" onClick={() => dismissToast(t.id)} aria-label="Dismiss">
              <X size={13} />
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}

function ToastAction({ action, onDone }: { action: NonNullable<Toast["action"]>; onDone: () => void }) {
  return (
    <button
      className="btn btn-sm"
      onClick={() => {
        action.run();
        onDone();
      }}
    >
      {action.label}
    </button>
  );
}

export default function App() {
  return (
    <StoreProvider>
      <Shell />
    </StoreProvider>
  );
}
