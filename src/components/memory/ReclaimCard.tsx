import { Check, ChevronDown, ChevronRight, X, Zap } from "lucide-react";
import { useState } from "react";
import { fmtBytes } from "../../lib/format";
import type { ProcRow, ReclaimResult } from "../../types";
import { Skeleton, Spinner } from "../ui";
import { has } from "./shared";

const fmtSigned = (n: number) => `${n < 0 ? "+" : "−"}${fmtBytes(Math.abs(n))}`;

export function ReclaimCard({
  procs,
  busy,
  result,
  onReclaim,
}: {
  procs: ProcRow[] | null;
  busy: boolean;
  result: ReclaimResult | null;
  onReclaim: () => void;
}) {
  const [showActions, setShowActions] = useState(false);
  const orphans = (procs ?? []).filter((r) => has(r, "orphan"));
  const orphanBytes = orphans.reduce((a, r) => a + r.rss, 0);
  return (
    <div className="card mb-4 p-4">
      <div className="flex items-start justify-between gap-4">
        <div className="flex-1">
          <div className="flex items-center gap-2 font-medium">
            <Zap size={15} className="text-accent" /> Reclaim
          </div>
          <p className="mt-1 text-xs text-fg-2">
            Ends orphan helpers and flushes the file cache. Claude Code agents, the app you were last using and macOS
            processes are left alone.
          </p>
          {!procs ? (
            <div className="mt-3 flex flex-col gap-2">
              <Skeleton className="h-3 w-72" />
              <Skeleton className="h-3 w-48" />
            </div>
          ) : (
            <ul className="mt-3 space-y-1 text-xs">
              <PreviewLine
                on={orphans.length > 0}
                text={
                  orphans.length
                    ? `End ${orphans.length} orphan helper${orphans.length === 1 ? "" : "s"} (${orphans
                        .slice(0, 4)
                        .map((r) => r.name)
                        .join(", ")}${orphans.length > 4 ? ", …" : ""}) · ${fmtBytes(orphanBytes)}`
                    : "No orphan helpers right now"
                }
              />
              <PreviewLine on text="Flush the file cache so apps get fresh room" />
            </ul>
          )}
        </div>
        <div className="flex shrink-0 flex-col items-end gap-1.5">
          <button className="btn btn-primary" disabled={busy || !procs} onClick={onReclaim}>
            {busy ? <Spinner className="text-current" /> : <Zap size={14} />} Reclaim now
          </button>
          <span className="text-[11px] text-fg-3">Flushing the cache asks for your password</span>
        </div>
      </div>
      {result && (
        <div className="mt-3 border-t border-line pt-3 text-sm">
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
            <span className="font-medium">Freed {fmtBytes(result.freedBytes)}</span>
            <span className="tnum text-fg-2">
              free {result.freePctDelta >= 0 ? "+" : ""}
              {result.freePctDelta.toFixed(1)}% · swap {fmtSigned(result.swapDelta)}
            </span>
            <button
              className="btn btn-ghost btn-sm"
              onClick={() => setShowActions((v) => !v)}
              aria-expanded={showActions}
            >
              {showActions ? <ChevronDown size={13} /> : <ChevronRight size={13} />} {result.actions.length} action
              {result.actions.length === 1 ? "" : "s"}
            </button>
          </div>
          {showActions && (
            <ul className="mt-2 space-y-1 text-xs">
              {result.actions.map((a, i) => (
                <li key={i} className="flex items-center gap-2">
                  {a.ok ? (
                    <Check size={12} className="shrink-0 text-accent" />
                  ) : (
                    <X size={12} className="shrink-0 text-danger" />
                  )}
                  <span className="truncate">{a.label}</span>
                  <span className="truncate text-fg-3">{a.detail}</span>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </div>
  );
}

function PreviewLine({ on, text }: { on: boolean; text: string }) {
  return (
    <li className={`flex items-center gap-2 ${on ? "" : "text-fg-3"}`}>
      <span className={`h-1.5 w-1.5 shrink-0 rounded-full ${on ? "bg-accent" : "bg-line-strong"}`} />
      {text}
    </li>
  );
}
