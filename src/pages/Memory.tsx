import { Power, RefreshCw } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { api } from "../api";
import { useStore } from "../storeContext";
import { usePoll } from "../lib/usePoll";
import { ErrorBox, PageHeader, Progress, Skeleton, Stat } from "../components/ui";
import { PressureGauge, Sparkline } from "../components/memory/MemoryCharts";
import { ReclaimCard } from "../components/memory/ReclaimCard";
import { SoftRestartCards } from "../components/memory/SoftRestartCards";
import { ProcessTable } from "../components/memory/ProcessTable";
import { EndProcessesModal } from "../components/memory/EndProcessesModal";
import { memoryFn, procsFn } from "../components/memory/shared";
import { fmtBytes, fmtUptime } from "../lib/format";
import type { ProcRow, ReclaimResult, ReclaimTier, SoftRestartApp } from "../types";

export default function Memory() {
  const { data: m, error, load } = usePoll(memoryFn, 10000);
  const { data: procs, error: procsError, load: loadProcs } = usePoll(procsFn, 10000);
  const { pushToast } = useStore();
  const [restartError, setRestartError] = useState<string | null>(null);
  const [reclaiming, setReclaiming] = useState<ReclaimTier | null>(null);
  const [result, setResult] = useState<ReclaimResult | null>(null);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [confirm, setConfirm] = useState(false);
  const [restarting, setRestarting] = useState<SoftRestartApp | null>(null);
  const [restartMsg, setRestartMsg] = useState<Partial<Record<SoftRestartApp, string>>>({});

  const rows = useMemo(() => procs ?? [], [procs]);
  const byPid = useMemo(() => new Map(rows.map((r) => [r.pid, r])), [rows]);
  useEffect(() => {
    setSelected((s) => {
      const n = new Set([...s].filter((pid) => byPid.has(pid)));
      return n.size === s.size ? s : n;
    });
  }, [byPid]);

  const refresh = () => Promise.all([load(), loadProcs()]);
  /** Asks macOS for its own restart confirmation, so open apps can still save or cancel the restart. */
  const restart = async () => {
    setRestartError(null);
    try {
      const r = await api.freshen("restart_prompt");
      if (!r.ok) setRestartError(r.message);
    } catch (e) {
      setRestartError(e instanceof Error ? e.message : String(e));
    }
  };
  const reclaim = async (tier: ReclaimTier, pids: number[]) => {
    setReclaiming(tier);
    setResult(null);
    try {
      setResult(await api.memReclaim(tier, pids));
      setSelected(new Set());
      await refresh();
    } catch (e) {
      pushToast({ kind: "error", text: e instanceof Error ? e.message : String(e) });
    } finally {
      setReclaiming(null);
    }
  };
  const softRestart = async (app: SoftRestartApp) => {
    setRestarting(app);
    setRestartMsg((s) => ({ ...s, [app]: undefined }));
    try {
      const r = await api.softRestart(app);
      if (r.ok) pushToast({ kind: "success", text: r.message });
      else setRestartMsg((s) => ({ ...s, [app]: r.message }));
      await refresh();
    } catch (e) {
      setRestartMsg((s) => ({ ...s, [app]: e instanceof Error ? e.message : String(e) }));
    } finally {
      setRestarting(null);
    }
  };
  const togglePid = (pid: number) =>
    setSelected((s) => {
      const n = new Set(s);
      if (n.has(pid)) n.delete(pid);
      else n.add(pid);
      return n;
    });

  if (error && !m)
    return (
      <div className="fade-in mx-auto max-w-[880px]">
        <PageHeader title="Memory" />
        <ErrorBox title="Could not read memory stats" message={error} retry={() => void load()} />
      </div>
    );
  if (!m) return <MemorySkeleton />;

  const swapPct = m.swapTotal ? (m.swapUsed / m.swapTotal) * 100 : 0;
  const recommend = m.uptimeSec > 5 * 86400 || swapPct > 70;
  const gauge = m.pressure === "critical" ? 92 : m.pressure === "warn" ? 65 : Math.max(8, 100 - m.freePct * 2);
  const selectedRows = [...selected].map((pid) => byPid.get(pid)).filter((r): r is ProcRow => !!r);
  const selectedBytes = selectedRows.reduce((a, r) => a + r.rss, 0);
  const busy = reclaiming !== null;
  const freeRange = m.history.length
    ? `${Math.round(Math.min(...m.history.map((h) => h.freePct)))}–${Math.round(Math.max(...m.history.map((h) => h.freePct)))}%`
    : "collecting…";

  return (
    <div className="fade-in mx-auto max-w-[880px]">
      <PageHeader title="Memory" subtitle="Refreshes every 10 seconds">
        <button className="btn btn-sm" onClick={() => void refresh()}>
          <RefreshCw size={13} /> Refresh
        </button>
      </PageHeader>

      <div className="card mb-4 flex flex-wrap items-center gap-6 p-4">
        <PressureGauge value={gauge} pressure={m.pressure} />
        <div className="min-w-[240px] flex-1">
          <div className="text-[15px] font-semibold capitalize">Pressure {m.pressure}</div>
          <div className="text-fg-2">
            {fmtBytes(m.totalBytes)} installed · {Math.round(m.freePct)}% free
          </div>
          <div className="mt-2">
            <div className="mb-1 flex justify-between text-xs">
              <span className="text-fg-2">Free, last 10 min</span>
              <span className="tnum text-fg-3">{freeRange}</span>
            </div>
            <Sparkline points={m.history.map((h) => h.freePct)} pressure={m.pressure} />
          </div>
          <div className="mt-2">
            <div className="mb-1 flex justify-between text-xs">
              <span className="text-fg-2">Swap</span>
              <span className="tnum">
                {fmtBytes(m.swapUsed)} / {fmtBytes(m.swapTotal)}
              </span>
            </div>
            <Progress label="Swap used" value={swapPct} danger={swapPct > 70} />
          </div>
        </div>
        <div className="flex flex-col items-end gap-2">
          <button className="btn" onClick={() => void restart()}>
            <Power size={14} /> Restart now
          </button>
          {recommend && <span className="text-xs font-medium text-warn">A restart would help</span>}
        </div>
      </div>
      {restartError && (
        <div className="mb-4">
          <ErrorBox title="Could not open the restart dialog" message={restartError} retry={() => void restart()} />
        </div>
      )}

      <div className="mb-4 grid grid-cols-4 gap-2">
        <Stat label="Compressed" value={fmtBytes(m.compressedBytes)} />
        <Stat label="Pageouts" value={m.pageouts.toLocaleString()} tone={m.pageouts > 1e6 ? "warn" : undefined} />
        <Stat
          label="Uptime"
          value={fmtUptime(m.uptimeSec)}
          tone={m.uptimeSec > 5 * 86400 ? "warn" : undefined}
          sub={recommend ? "Over 5 days or swap above 70%" : undefined}
        />
        <Stat label="Swap used" value={`${Math.round(swapPct)}%`} tone={swapPct > 70 ? "danger" : undefined} />
      </div>

      <ReclaimCard procs={procs} busy={busy} result={result} onReclaim={() => void reclaim("safe", [])} />
      <SoftRestartCards
        rows={rows}
        restarting={restarting}
        disabled={busy}
        messages={restartMsg}
        onRestart={(app) => void softRestart(app)}
      />
      <ProcessTable
        rows={rows}
        loading={!procs}
        error={procsError}
        onRetry={() => void loadProcs()}
        selected={selected}
        onToggle={togglePid}
      />

      {selected.size > 0 && (
        <div className="sticky -bottom-6 -mx-6 -mb-6 mt-4 flex items-center gap-4 border-t border-line bg-panel/90 px-6 py-2.5 backdrop-blur">
          <span className="text-sm">
            <span className="font-medium">{selected.size} selected</span>{" "}
            <span className="tnum text-fg-2">· {fmtBytes(selectedBytes)}</span>
          </span>
          <button className="btn btn-ghost btn-sm" onClick={() => setSelected(new Set())}>
            Clear
          </button>
          <div className="flex-1" />
          <button className="btn btn-danger" disabled={busy} onClick={() => setConfirm(true)}>
            End selected
          </button>
        </div>
      )}

      {confirm && (
        <EndProcessesModal
          rows={selectedRows}
          onClose={() => setConfirm(false)}
          onConfirm={() => {
            setConfirm(false);
            void reclaim(
              "aggressive",
              selectedRows.map((r) => r.pid),
            );
          }}
        />
      )}
    </div>
  );
}

function MemorySkeleton() {
  return (
    <div className="fade-in mx-auto max-w-[880px]" role="status" aria-label="Reading memory stats">
      <PageHeader title="Memory" />
      <div className="card mb-4 flex items-center gap-6 p-4">
        <Skeleton className="h-[80px] w-[120px] rounded-full" />
        <div className="flex flex-1 flex-col gap-3">
          <Skeleton className="h-4 w-40" />
          <Skeleton className="h-3 w-56" />
          <Skeleton className="h-7 w-full" />
        </div>
      </div>
      <div className="mb-4 grid grid-cols-4 gap-2">
        {Array.from({ length: 4 }, (_, i) => (
          <div key={i} className="card flex flex-col gap-2 px-3 py-2.5">
            <Skeleton className="h-2.5 w-16" />
            <Skeleton className="h-5 w-20" />
          </div>
        ))}
      </div>
    </div>
  );
}
