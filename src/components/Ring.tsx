import type { SVGProps } from "react";
import { fmtBytes } from "../lib/format";

/** Free-space ring: used (fg), would-free (accent, carved out of used), free (track). */
export function Ring({
  total,
  free,
  wouldFree,
  size = 148,
}: {
  total: number;
  free: number;
  wouldFree: number;
  size?: number;
}) {
  const sw = 12,
    r = (size - sw) / 2,
    c = 2 * Math.PI * r;
  const used = Math.max(0, total - free);
  const wf = Math.min(wouldFree, used);
  const usedPct = total ? (used - wf) / total : 0,
    wfPct = total ? wf / total : 0;
  const seg = (pct: number, offsetPct: number, color: string, extra?: SVGProps<SVGCircleElement>) => (
    <circle
      cx={size / 2}
      cy={size / 2}
      r={r}
      fill="none"
      stroke={color}
      strokeWidth={sw}
      strokeDasharray={`${pct * c} ${c}`}
      strokeDashoffset={-offsetPct * c}
      transform={`rotate(-90 ${size / 2} ${size / 2})`}
      strokeLinecap="butt"
      {...extra}
    />
  );
  return (
    <div className="flex items-center gap-5">
      <svg width={size} height={size} className="shrink-0">
        <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="var(--c-line)" strokeWidth={sw} />
        {seg(usedPct, 0, "var(--c-fg-2)")}
        {seg(wfPct, usedPct, "var(--c-accent)")}
        <text x="50%" y="46%" textAnchor="middle" fontSize={20} fontWeight={600} fill="var(--c-fg)" className="tnum">
          {fmtBytes(free)}
        </text>
        <text x="50%" y="62%" textAnchor="middle" fontSize={11} fill="var(--c-fg-3)">
          free of {fmtBytes(total)}
        </text>
      </svg>
      <div className="flex flex-col gap-1.5 text-xs">
        <Legend color="var(--c-fg-2)" label="Used" value={fmtBytes(used - wf)} />
        <Legend color="var(--c-accent)" label="Would free" value={fmtBytes(wf)} />
        <Legend color="var(--c-line)" label="Free" value={fmtBytes(free)} />
        <div className="mt-1 text-fg-3">
          After clean: <span className="tnum font-medium text-fg">{fmtBytes(free + wf)}</span>
        </div>
      </div>
    </div>
  );
}
function Legend({ color, label, value }: { color: string; label: string; value: string }) {
  return (
    <div className="flex items-center gap-2">
      <span className="h-2.5 w-2.5 rounded-sm" style={{ background: color }} />
      <span className="w-20 text-fg-2">{label}</span>
      <span className="tnum font-medium">{value}</span>
    </div>
  );
}
