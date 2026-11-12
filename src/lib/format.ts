export function fmtBytes(n: number): string {
  if (!Number.isFinite(n) || n <= 0) return "0 B";
  const GB = 1e9,
    MB = 1e6,
    KB = 1e3;
  if (n >= GB) return `${(n / GB).toFixed(1)} GB`;
  if (n >= MB) return `${(n / MB).toFixed(1)} MB`;
  if (n >= KB) return `${(n / KB).toFixed(0)} KB`;
  return `${n} B`;
}

export function relTime(epochSec?: number): string {
  if (!epochSec) return "—";
  const s = Math.max(0, Date.now() / 1000 - epochSec);
  const m = s / 60,
    h = m / 60,
    d = h / 24,
    mo = d / 30.44,
    y = d / 365.25;
  if (s < 60) return "just now";
  if (m < 60) return `${Math.round(m)} min ago`;
  if (h < 24) return `${Math.round(h)} h ago`;
  if (d < 30) return `${Math.round(d)} d ago`;
  if (mo < 12) return `${Math.round(mo)} mo ago`;
  return `${y.toFixed(y < 10 ? 1 : 0)} y ago`;
}

export function fmtDate(epochSec?: number): string {
  if (!epochSec) return "—";
  return new Date(epochSec * 1000).toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
}

export function fmtUptime(sec: number): string {
  const d = Math.floor(sec / 86400),
    h = Math.floor((sec % 86400) / 3600),
    m = Math.floor((sec % 3600) / 60);
  if (d > 0) return `${d}d ${h}h`;
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m`;
}

/** Truncate a path in the middle, keeping the tail (basename) intact where possible. */
export function truncMiddle(s: string, max = 64): string {
  if (s.length <= max) return s;
  const tail = Math.floor(max * 0.6),
    head = max - tail - 1;
  return `${s.slice(0, head)}…${s.slice(-tail)}`;
}

export function basename(p: string): string {
  const parts = p.replace(/\/+$/, "").split("/");
  return parts[parts.length - 1] || p;
}

export function dirname(p: string): string {
  const b = basename(p);
  return p.slice(0, Math.max(0, p.length - b.length - 1)) || "/";
}
