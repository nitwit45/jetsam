import { ChevronRight } from "lucide-react";
import { useEffect, useRef } from "react";
import { fmtBytes } from "../../lib/format";
import { MODE_LABEL } from "../../lib/modes";
import { isSelectable, type Category } from "../../types";
import { Chip } from "../ui";
import { ItemTable } from "./ItemTable";

export function CategoryRow({
  cat,
  open,
  onToggle,
  selected,
  toggle,
  setMany,
}: {
  cat: Category;
  open: boolean;
  onToggle: () => void;
  selected: Set<string>;
  toggle: (id: string) => void;
  setMany: (ids: string[], on: boolean) => void;
}) {
  const sel = cat.items.filter((i) => selected.has(i.id));
  const selectableIds = cat.items.filter(isSelectable).map((i) => i.id);
  const onCount = selectableIds.filter((id) => selected.has(id)).length;
  const state = onCount === 0 ? "none" : onCount === selectableIds.length ? "all" : "some";
  const cb = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (cb.current) cb.current.indeterminate = state === "some";
  }, [state]);
  return (
    <div className="card overflow-hidden">
      <div className="flex h-10 items-center gap-2 px-2 hover:bg-hover">
        <input
          ref={cb}
          type="checkbox"
          checked={state === "all"}
          disabled={selectableIds.length === 0}
          onChange={() => setMany(selectableIds, state !== "all")}
          aria-label={
            state === "all" ? `Deselect everything in ${cat.label}` : `Select everything you can in ${cat.label}`
          }
          title={
            selectableIds.length === 0
              ? "Nothing here can be selected"
              : state === "all"
                ? "Deselect all"
                : "Select every item that is not locked"
          }
        />
        <button
          className="flex h-full min-w-0 flex-1 items-center gap-2 rounded text-left"
          onClick={onToggle}
          aria-expanded={open}
        >
          <ChevronRight size={14} className={`shrink-0 text-fg-3 transition-transform ${open ? "rotate-90" : ""}`} />
          <span className="truncate font-medium">{cat.label}</span>
          <Chip
            tip={`Found by the ${MODE_LABEL[cat.mode]} scan and every mode above it`}
            className="border-line text-fg-3 uppercase"
          >
            {cat.mode}
          </Chip>
        </button>
        <span className="tnum w-44 whitespace-nowrap text-right text-fg-2">
          {sel.length ? `${fmtBytes(sel.reduce((a, i) => a + i.bytes, 0))} of ` : ""}
          {fmtBytes(cat.totalBytes)}
        </span>
        <span className="tnum w-16 text-right text-xs text-fg-3">
          {cat.items.length === 1 ? "1 item" : `${cat.items.length} items`}
        </span>
      </div>
      {open && <ItemTable items={cat.items} selected={selected} toggle={toggle} />}
    </div>
  );
}
