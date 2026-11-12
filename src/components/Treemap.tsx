import { useLayoutEffect, useRef, useState } from "react";
import { squarify } from "../lib/treemap";
import { fmtBytes } from "../lib/format";
import type { Category } from "../types";

const PALETTE = ["#0e9f8f", "#3b82c4", "#8b6cc9", "#c9677a", "#d9892b", "#5f9e3a", "#4c9ab8", "#9b7c4a"];
const CHAR_W = 6.2;
const PAD = 8;
const LINE_H = 14;

/** Fit a label into a tile: whole on one line, wrapped to two lines, shortened (parenthetical dropped), then cut at a word. */
function fitLabel(label: string, w: number, h: number): string[] {
  const maxChars = Math.floor((w - PAD * 2) / CHAR_W);
  if (maxChars < 4) return [];
  if (label.length <= maxChars) return [label];
  const twoLines = h >= LINE_H * 2 + PAD * 2 + 14;
  if (twoLines) {
    const words = label.split(" ");
    const lines: string[] = [];
    let cur = "";
    for (const word of words) {
      const next = cur ? `${cur} ${word}` : word;
      if (next.length <= maxChars) cur = next;
      else {
        if (cur) lines.push(cur);
        cur = word;
      }
    }
    if (cur) lines.push(cur);
    if (lines.length <= 2 && lines.every((l) => l.length <= maxChars)) return lines;
  }
  const short = label.replace(/\s*\(.*?\)/g, "").trim();
  if (short.length <= maxChars) return [short];
  const cut = short.slice(0, maxChars - 1);
  const atWord = cut.lastIndexOf(" ");
  return [`${(atWord > maxChars / 2 ? cut.slice(0, atWord) : cut).trimEnd()}…`];
}

export function Treemap({
  categories,
  height = 220,
  onPick,
}: {
  categories: Category[];
  height?: number;
  onPick?: (id: string) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [w, setW] = useState(0);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    setW(el.clientWidth);
    const ro = new ResizeObserver(([e]) => setW(e.contentRect.width));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  const rects = squarify(
    categories.map((c) => ({ id: c.id, value: c.totalBytes })),
    w,
    height,
  );
  const byId = Object.fromEntries(categories.map((c, i) => [c.id, { c, color: PALETTE[i % PALETTE.length] }]));
  return (
    <div ref={ref} className="w-full">
      {w > 0 && (
        <svg
          width={w}
          height={height}
          className="block overflow-hidden rounded-md"
          role="img"
          aria-label="Reclaimable space by category"
        >
          {rects.map((r) => {
            const { c, color } = byId[r.id];
            const lines = r.h > 30 ? fitLabel(c.label, r.w, r.h) : [];
            const sizeY = r.y + PAD + LINE_H * lines.length + 10;
            const showSize = lines.length > 0 && sizeY + 4 < r.y + r.h;
            const tip = `${c.label} · ${fmtBytes(c.totalBytes)} · ${c.items.length} items`;
            return (
              <g
                key={r.id}
                onClick={() => onPick?.(r.id)}
                onKeyDown={(e) => {
                  if (onPick && (e.key === "Enter" || e.key === " ")) {
                    e.preventDefault();
                    onPick(r.id);
                  }
                }}
                tabIndex={onPick ? 0 : undefined}
                role={onPick ? "button" : undefined}
                aria-label={tip}
                style={{ cursor: onPick ? "pointer" : undefined }}
                className="outline-none focus-visible:[&>rect]:stroke-fg focus-visible:[&>rect]:stroke-2"
              >
                <title>{tip}</title>
                <rect
                  x={r.x + 1}
                  y={r.y + 1}
                  width={Math.max(0, r.w - 2)}
                  height={Math.max(0, r.h - 2)}
                  rx={4}
                  fill={color}
                  fillOpacity={0.85}
                  className="transition hover:fill-opacity-100"
                />
                {lines.map((line, i) => (
                  <text
                    key={i}
                    x={r.x + PAD}
                    y={r.y + PAD + 10 + i * LINE_H}
                    fill="#fff"
                    fontSize={11}
                    fontWeight={600}
                    style={{ pointerEvents: "none" }}
                  >
                    {line}
                  </text>
                ))}
                {showSize && (
                  <text
                    x={r.x + PAD}
                    y={sizeY}
                    fill="#fff"
                    fillOpacity={0.85}
                    fontSize={11}
                    style={{ pointerEvents: "none" }}
                  >
                    {fmtBytes(c.totalBytes)}
                  </text>
                )}
              </g>
            );
          })}
        </svg>
      )}
    </div>
  );
}
