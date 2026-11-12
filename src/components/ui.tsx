import { AlertCircle, Loader2, X } from "lucide-react";
import { useEffect, type ReactNode } from "react";
import type { Flag } from "../types";

const FLAG_META: Record<Flag, { label: string; tip: string; cls: string }> = {
  dirty: {
    label: "dirty",
    tip: "This repo has uncommitted changes, so nothing in it is selected",
    cls: "text-warn border-warn/40 bg-warn/10",
  },
  unmerged: {
    label: "unmerged",
    tip: "This repo has commits that are not pushed or merged, so nothing in it is selected",
    cls: "text-warn border-warn/40 bg-warn/10",
  },
  "git-tracked": {
    label: "tracked",
    tip: "Git tracks this path, so it cannot be regenerated and is never cleaned",
    cls: "text-danger border-danger/40 bg-danger/10",
  },
  recent: {
    label: "recent",
    tip: "In a project you touched recently. Use Full to include.",
    cls: "text-accent border-accent/40 bg-accent-soft",
  },
  review: {
    label: "review",
    tip: "Not regenerable. Check before deleting.",
    cls: "text-warn border-warn/40 bg-warn/10",
  },
  "report-only": {
    label: "size only",
    tip: "Shown for its size. Jetsam does not clean this location.",
    cls: "text-fg-2 border-line-strong bg-hover",
  },
  worktree: {
    label: "worktree",
    tip: "A git worktree, separate from the main checkout of its repo",
    cls: "text-fg-2 border-line-strong",
  },
};

export function Badge({ flag }: { flag: string }) {
  const m = FLAG_META[flag as Flag] ?? { label: flag, tip: flag, cls: "text-fg-2 border-line-strong" };
  return (
    <Chip tip={m.tip} className={m.cls}>
      {m.label}
    </Chip>
  );
}

/** Small bordered label with a hover tooltip. Reachable by keyboard so the tooltip is not mouse-only. */
export function Chip({ tip, className = "", children }: { tip: string; className?: string; children: ReactNode }) {
  return (
    <span
      tabIndex={0}
      title={tip}
      aria-label={tip}
      className={`inline-block rounded border px-1 text-[10px] font-medium leading-4 ${className}`}
    >
      {children}
    </span>
  );
}

export function Spinner({ className = "" }: { className?: string }) {
  return <Loader2 size={14} aria-hidden className={`spin text-fg-3 ${className}`} />;
}

export function Skeleton({ className = "" }: { className?: string }) {
  return <span aria-hidden className={`shimmer block rounded bg-line ${className}`} />;
}

/** Placeholder rows that match a table's height while it loads, so the layout does not jump. */
export function TableSkeleton({ rows = 6, label }: { rows?: number; label: string }) {
  return (
    <div className="card overflow-hidden" role="status" aria-live="polite" aria-label={label}>
      <div className="h-7 border-b border-line bg-panel-2" />
      {Array.from({ length: rows }, (_, i) => (
        <div key={i} className="flex h-10 items-center gap-4 border-b border-line px-4 last:border-0">
          <Skeleton className="h-3 flex-1" />
          <Skeleton className="h-3 w-16" />
          <Skeleton className="h-3 w-20" />
          <Skeleton className="h-3 w-12" />
        </div>
      ))}
      <span className="sr-only">{label}</span>
    </div>
  );
}

export function Segmented<T extends string>({
  value,
  options,
  onChange,
  disabled,
  label,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
  disabled?: boolean;
  label: string;
}) {
  return (
    <div className="inline-flex rounded-md border border-line bg-panel-2 p-0.5" role="radiogroup" aria-label={label}>
      {options.map((o) => (
        <button
          key={o.value}
          role="radio"
          aria-checked={value === o.value}
          disabled={disabled}
          onClick={() => onChange(o.value)}
          className={`h-6 whitespace-nowrap rounded px-3 text-xs font-medium transition ${value === o.value ? "bg-panel text-fg shadow-[0_0_0_1px_var(--color-line)]" : "text-fg-2 hover:text-fg"} disabled:opacity-50`}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function PageHeader({ title, subtitle, children }: { title: string; subtitle?: string; children?: ReactNode }) {
  return (
    <div className="mb-4 flex min-h-8 items-start justify-between gap-4">
      <div>
        <h1 className="text-[17px] font-semibold leading-8 tracking-tight">{title}</h1>
        {subtitle && <p className="-mt-1 text-xs text-fg-2">{subtitle}</p>}
      </div>
      <div className="flex h-8 items-center gap-2">{children}</div>
    </div>
  );
}

export function Empty({
  icon,
  title,
  hint,
  action,
}: {
  icon: ReactNode;
  title: string;
  hint?: string;
  action?: ReactNode;
}) {
  return (
    <div className="fade-in flex flex-col items-center justify-center gap-2 py-16 text-center">
      <div className="text-fg-3">{icon}</div>
      <div className="font-medium">{title}</div>
      {hint && <div className="max-w-sm text-fg-2">{hint}</div>}
      {action && <div className="mt-2">{action}</div>}
    </div>
  );
}

export function ErrorBox({ message, retry, title }: { message: string; retry?: () => void; title?: string }) {
  return (
    <div
      role="alert"
      className="fade-in flex items-start gap-2 rounded-md border border-danger/40 bg-danger-soft p-3 text-fg"
    >
      <AlertCircle size={16} className="mt-0.5 shrink-0 text-danger" />
      <div className="min-w-0 flex-1 select-text break-words">
        {title && <div className="font-medium">{title}</div>}
        <div className={title ? "text-xs text-fg-2" : ""}>{message}</div>
      </div>
      {retry && (
        <button className="btn btn-sm" onClick={retry}>
          Retry
        </button>
      )}
    </div>
  );
}

export function Modal({
  title,
  onClose,
  children,
  footer,
  danger,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  danger?: boolean;
}) {
  useEffect(() => {
    const k = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, [onClose]);
  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/30 pt-[12vh]" onMouseDown={onClose}>
      <div
        role="dialog"
        aria-modal="true"
        aria-label={title}
        onMouseDown={(e) => e.stopPropagation()}
        className={`fade-in card w-[560px] max-w-[calc(100vw-32px)] shadow-2xl ${danger ? "border-danger/50" : ""}`}
      >
        <div className="flex items-center justify-between border-b border-line px-4 py-2.5">
          <div className={`font-semibold ${danger ? "text-danger" : ""}`}>{title}</div>
          <button className="btn btn-ghost btn-sm -mr-2 px-1" onClick={onClose} aria-label="Close">
            <X size={14} />
          </button>
        </div>
        <div className="px-4 py-3">{children}</div>
        {footer && <div className="flex justify-end gap-2 border-t border-line px-4 py-2.5">{footer}</div>}
      </div>
    </div>
  );
}

export function Stat({
  label,
  value,
  sub,
  tone,
}: {
  label: string;
  value: ReactNode;
  sub?: ReactNode;
  tone?: "warn" | "danger" | "accent";
}) {
  const c = tone === "warn" ? "text-warn" : tone === "danger" ? "text-danger" : tone === "accent" ? "text-accent" : "";
  return (
    <div className="card px-3 py-2.5">
      <div className="label-xs">{label}</div>
      <div className={`tnum mt-0.5 text-[20px] font-semibold leading-tight ${c}`}>{value}</div>
      {sub && <div className="mt-0.5 text-xs text-fg-3">{sub}</div>}
    </div>
  );
}

export function Progress({ value, danger, label }: { value: number; danger?: boolean; label?: string }) {
  const v = Math.max(0, Math.min(100, value));
  return (
    <div
      className="h-1.5 w-full overflow-hidden rounded-full bg-line"
      role="progressbar"
      aria-valuenow={Math.round(v)}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-label={label}
    >
      <div
        className={`h-full rounded-full transition-[width] duration-150 ${danger ? "bg-danger" : "bg-accent"}`}
        style={{ width: `${v}%` }}
      />
    </div>
  );
}

export function StatusPill({
  tone,
  children,
  tip,
}: {
  tone: "ok" | "warn" | "muted";
  children: ReactNode;
  tip?: string;
}) {
  const cls =
    tone === "ok"
      ? "border-accent/40 bg-accent-soft text-accent"
      : tone === "warn"
        ? "border-warn/40 bg-warn/10 text-warn"
        : "border-line-strong text-fg-2";
  return (
    <span title={tip} className={`shrink-0 rounded-full border px-2 text-[11px] leading-5 ${cls}`}>
      {children}
    </span>
  );
}
