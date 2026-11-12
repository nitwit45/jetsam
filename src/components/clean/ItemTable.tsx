import { useCallback, useEffect, useRef, useState, type KeyboardEvent } from "react";
import { basename, dirname, fmtBytes, relTime } from "../../lib/format";
import { BLOCKING_FLAGS, isSelectable, type Item } from "../../types";
import { Badge } from "../ui";

const ROW_H = 30;
const VISIBLE_ROWS = 12;
const OVERSCAN = 4;

const lockedReason = (it: Item) => `Cannot be selected: ${it.flags.filter((f) => BLOCKING_FLAGS.has(f)).join(", ")}`;

/** Virtualised list of one category's items. Space toggles, arrows move, so long lists stay keyboard friendly. */
export function ItemTable({
  items,
  selected,
  toggle,
}: {
  items: Item[];
  selected: Set<string>;
  toggle: (id: string) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [focus, setFocus] = useState(0);
  const h = Math.min(items.length, VISIBLE_ROWS) * ROW_H;
  const start = Math.max(0, Math.floor(scrollTop / ROW_H) - OVERSCAN);
  const end = Math.min(items.length, Math.ceil((scrollTop + h) / ROW_H) + OVERSCAN);

  const onKey = useCallback(
    (e: KeyboardEvent<HTMLDivElement>) => {
      if (e.key === " ") {
        e.preventDefault();
        const it = items[focus];
        if (it && isSelectable(it)) toggle(it.id);
      }
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setFocus((f) => Math.min(items.length - 1, f + 1));
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        setFocus((f) => Math.max(0, f - 1));
      }
    },
    [items, focus, toggle],
  );

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const top = focus * ROW_H;
    if (top < el.scrollTop) el.scrollTop = top;
    else if (top + ROW_H > el.scrollTop + h) el.scrollTop = top + ROW_H - h;
  }, [focus, h]);

  return (
    <div className="border-t border-line">
      <div className="flex h-6 items-center gap-2 bg-panel-2 px-2 text-[11px] font-medium text-fg-3">
        <span className="w-[13px]" />
        <span className="flex-1">Path</span>
        <span className="w-24">Project</span>
        <span className="w-20">Last active</span>
        <span className="w-20 text-right">Size</span>
        <span className="w-32">Flags</span>
      </div>
      <div
        ref={ref}
        tabIndex={0}
        role="listbox"
        aria-multiselectable
        aria-label="Items in this category"
        onKeyDown={onKey}
        className="overflow-auto outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent"
        style={{ height: h }}
        onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}
      >
        <div style={{ height: items.length * ROW_H, position: "relative" }}>
          {items.slice(start, end).map((it, k) => {
            const idx = start + k;
            const selectable = isSelectable(it);
            const on = selected.has(it.id);
            return (
              <div
                key={it.id}
                role="option"
                aria-selected={on}
                aria-disabled={!selectable}
                style={{ position: "absolute", top: idx * ROW_H, height: ROW_H }}
                className={`flex w-full items-center gap-2 px-2 text-xs ${on ? "bg-selected" : idx === focus ? "bg-hover" : ""} ${!selectable ? "opacity-60" : ""}`}
                onClick={() => {
                  setFocus(idx);
                  if (selectable) toggle(it.id);
                }}
                title={selectable ? it.path : lockedReason(it)}
              >
                <input
                  type="checkbox"
                  checked={on}
                  disabled={!selectable}
                  aria-label={basename(it.path)}
                  title={selectable ? undefined : lockedReason(it)}
                  onClick={(e) => e.stopPropagation()}
                  onChange={() => toggle(it.id)}
                />
                <span className="mono flex min-w-0 flex-1">
                  {it.kind === "command" ? (
                    <span className="truncate text-accent">$ {it.path}</span>
                  ) : (
                    <>
                      <span className="min-w-0 truncate text-fg-3">{dirname(it.path)}/</span>
                      <span className="shrink-0">{basename(it.path)}</span>
                    </>
                  )}
                </span>
                <span className="w-24 truncate text-fg-2">{it.repo ?? ""}</span>
                <span className="tnum w-20 text-fg-2">{relTime(it.repoLastActive ?? it.mtime)}</span>
                <span className="tnum w-20 text-right">{fmtBytes(it.bytes)}</span>
                <span className="flex w-32 gap-1 overflow-hidden">
                  {it.flags.map((f) => (
                    <Badge key={f} flag={f} />
                  ))}
                </span>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
