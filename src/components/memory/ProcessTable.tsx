import { ChevronDown, ChevronRight, Lock } from "lucide-react";
import { useMemo, useState } from "react";
import { fmtBytes, fmtUptime } from "../../lib/format";
import type { ProcRow } from "../../types";
import { Chip, ErrorBox, TableSkeleton } from "../ui";
import { has, isProtected, protectTip } from "./shared";

const REASON: Record<string, { label: string; tip: string; cls: string }> = {
  orphan: {
    label: "orphan",
    tip: "Jetsam saw this helper with a parent process, and that parent has since exited. Reclaim ends it.",
    cls: "text-warn border-warn/40 bg-warn/10",
  },
  idle: {
    label: "idle",
    tip: "No CPU for 15 minutes while holding over 200 MB",
    cls: "text-fg-2 border-line-strong bg-hover",
  },
  leak: {
    label: "leak",
    tip: "Memory grew 30% and 150 MB or more over the sample window",
    cls: "text-danger border-danger/40 bg-danger/10",
  },
  vm: {
    label: "vm",
    tip: "Virtual machine. OrbStack manages its own memory, so there is nothing to do.",
    cls: "text-sky-600 border-sky-500/40 bg-sky-500/10 dark:text-sky-400",
  },
  headless: {
    label: "headless",
    tip: "A browser driven by automation (Playwright or Puppeteer)",
    cls: "text-sky-600 border-sky-500/40 bg-sky-500/10 dark:text-sky-400",
  },
};

export function ReasonBadge({ reason }: { reason: string }) {
  const m = REASON[reason] ?? { label: reason, tip: reason, cls: "text-fg-2 border-line-strong" };
  return (
    <Chip tip={m.tip} className={m.cls}>
      {m.label}
    </Chip>
  );
}

const HEAD = "flex h-7 items-center border-b border-line bg-panel-2 px-3 text-[11px] font-medium text-fg-3";

export function ProcessTable({
  rows,
  loading,
  error,
  onRetry,
  selected,
  onToggle,
}: {
  rows: ProcRow[];
  loading: boolean;
  error: string | null;
  onRetry: () => void;
  selected: Set<number>;
  onToggle: (pid: number) => void;
}) {
  const [open, setOpen] = useState<Set<string>>(new Set());
  const groups = useMemo(() => {
    const map = new Map<string, ProcRow[]>();
    for (const r of rows) map.set(r.app, [...(map.get(r.app) ?? []), r]);
    return [...map.entries()]
      .map(([app, list]) => ({
        app,
        list,
        rss: list.reduce((a, r) => a + r.rss, 0),
        cpu: list.reduce((a, r) => a + r.cpuPct, 0),
        headless: list.some((r) => has(r, "headless")),
        vm: list.some((r) => has(r, "vm")),
      }))
      .sort((a, b) => Number(b.headless) - Number(a.headless) || b.rss - a.rss);
  }, [rows]);
  const toggleGroup = (app: string) =>
    setOpen((s) => {
      const n = new Set(s);
      if (n.has(app)) n.delete(app);
      else n.add(app);
      return n;
    });

  if (error && rows.length === 0) return <ErrorBox title="Could not list processes" message={error} retry={onRetry} />;
  if (loading && rows.length === 0) return <TableSkeleton rows={8} label="Listing processes" />;

  return (
    <div className="card overflow-hidden">
      <div className={HEAD}>
        <span className="w-6" />
        <span className="flex-1">Processes by app</span>
        <span className="w-16 text-right">PID</span>
        <span className="w-20 text-right">Memory</span>
        <span className="w-14 text-right">CPU</span>
        <span className="w-14 text-right">Age</span>
        <span className="w-36 pl-3">Why it is listed</span>
      </div>
      {groups.map((g) => {
        const isOpen = open.has(g.app);
        const locked = g.list.every(isProtected);
        return (
          <div key={g.app}>
            <button
              className="flex h-8 w-full items-center border-b border-line px-3 text-left hover:bg-hover"
              onClick={() => toggleGroup(g.app)}
              aria-expanded={isOpen}
            >
              <span className="w-6 text-fg-3">{isOpen ? <ChevronDown size={13} /> : <ChevronRight size={13} />}</span>
              <span className="flex min-w-0 flex-1 items-center gap-1.5">
                {locked && <Lock size={11} className="shrink-0 text-fg-3" aria-label="Protected" />}
                <span className="truncate font-medium" title={locked ? protectTip(g.list[0]) : undefined}>
                  {g.app}
                </span>
                <span className="text-xs text-fg-3">{g.list.length}</span>
                {g.vm && <ReasonBadge reason="vm" />}
                {g.headless && <ReasonBadge reason="headless" />}
                {!isOpen && g.list.some((r) => has(r, "orphan")) && <ReasonBadge reason="orphan" />}
                {!isOpen && g.list.some((r) => has(r, "leak")) && <ReasonBadge reason="leak" />}
              </span>
              <span className="w-16" />
              <span className="tnum w-20 text-right font-medium">{fmtBytes(g.rss)}</span>
              <span className="tnum w-14 text-right text-fg-2">{g.cpu.toFixed(g.cpu >= 10 ? 0 : 1)}%</span>
              <span className="w-14" />
              <span className="w-36 pl-3" />
            </button>
            {isOpen &&
              g.list.map((r) => {
                const prot = isProtected(r);
                return (
                  <label
                    key={r.pid}
                    className={`flex h-8 items-center border-b border-line bg-panel-2/40 px-3 text-sm hover:bg-hover ${prot ? "cursor-default" : "cursor-pointer"}`}
                  >
                    <span className="flex w-6 items-center">
                      {prot ? (
                        <Lock size={11} className="text-fg-3" aria-label="Protected" />
                      ) : (
                        <input
                          type="checkbox"
                          checked={selected.has(r.pid)}
                          onChange={() => onToggle(r.pid)}
                          aria-label={`Select ${r.name}`}
                        />
                      )}
                    </span>
                    <span className="flex-1 truncate" title={prot ? protectTip(r) : r.name}>
                      {r.name}
                    </span>
                    <span className="tnum w-16 text-right text-fg-3">{r.pid}</span>
                    <span className="tnum w-20 text-right">{fmtBytes(r.rss)}</span>
                    <span className="tnum w-14 text-right text-fg-2">{r.cpuPct.toFixed(1)}%</span>
                    <span className="tnum w-14 text-right text-fg-2">{fmtUptime(r.elapsedSec)}</span>
                    <span className="flex w-36 flex-wrap gap-1 pl-3">
                      {prot && <Lock size={11} className="text-fg-3" aria-label="Protected" />}
                      {r.reasons
                        .filter((x) => x !== "protected")
                        .map((x) => (
                          <ReasonBadge key={x} reason={x} />
                        ))}
                    </span>
                  </label>
                );
              })}
          </div>
        );
      })}
    </div>
  );
}
