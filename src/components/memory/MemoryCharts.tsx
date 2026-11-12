import type { MemoryInfo } from "../../types";

const colorFor = (p: MemoryInfo["pressure"]) =>
  p === "critical" ? "var(--c-danger)" : p === "warn" ? "var(--c-warn)" : "var(--c-accent)";

export function Sparkline({ points, pressure }: { points: number[]; pressure: MemoryInfo["pressure"] }) {
  const w = 300;
  const h = 28;
  const color = colorFor(pressure);
  if (points.length < 2) return <div className="h-7 rounded bg-panel-2" />;
  const step = w / Math.max(1, points.length - 1);
  const y = (v: number) => h - 2 - (Math.max(0, Math.min(100, v)) / 100) * (h - 4);
  const d = points.map((v, i) => `${i === 0 ? "M" : "L"} ${(i * step).toFixed(1)} ${y(v).toFixed(1)}`).join(" ");
  return (
    <svg
      viewBox={`0 0 ${w} ${h}`}
      preserveAspectRatio="none"
      className="h-7 w-full rounded bg-panel-2"
      role="img"
      aria-label="Free memory over the last 10 minutes"
    >
      <path d={`${d} L ${w} ${h} L 0 ${h} Z`} fill={color} opacity={0.12} />
      <path d={d} stroke={color} strokeWidth={1.5} fill="none" vectorEffect="non-scaling-stroke" />
    </svg>
  );
}

export function PressureGauge({ value, pressure }: { value: number; pressure: MemoryInfo["pressure"] }) {
  const size = 120;
  const sw = 10;
  const r = (size - sw) / 2;
  const cx = size / 2;
  const cy = size / 2 + 8;
  const arc = (from: number, to: number) => {
    const a = (deg: number) => [
      cx + r * Math.cos(Math.PI * (1 + deg / 180)),
      cy + r * Math.sin(Math.PI * (1 + deg / 180)),
    ];
    const [x1, y1] = a(from);
    const [x2, y2] = a(to);
    return `M ${x1} ${y1} A ${r} ${r} 0 ${to - from > 180 ? 1 : 0} 1 ${x2} ${y2}`;
  };
  return (
    <svg
      width={size}
      height={size / 2 + 20}
      className="shrink-0"
      role="img"
      aria-label={`Memory pressure ${Math.round(value)} percent`}
    >
      <path d={arc(0, 180)} stroke="var(--c-line)" strokeWidth={sw} fill="none" strokeLinecap="round" />
      <path
        d={arc(0, Math.max(1, Math.min(180, value * 1.8)))}
        stroke={colorFor(pressure)}
        strokeWidth={sw}
        fill="none"
        strokeLinecap="round"
      />
      <text x={cx} y={cy - 6} textAnchor="middle" fontSize={18} fontWeight={600} fill="var(--c-fg)">
        {Math.round(value)}%
      </text>
    </svg>
  );
}
