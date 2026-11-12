import { Eye, ListChecks, Search, Trash2 } from "lucide-react";
import { useMemo, useState } from "react";
import { useStore } from "../storeContext";
import { Empty, Progress, Segmented, Spinner, TableSkeleton } from "../components/ui";
import { CategoryRow } from "../components/clean/CategoryRow";
import { PermanentDeleteModal, PreviewModal } from "../components/clean/CleanModals";
import { fmtBytes } from "../lib/format";
import { MODE_LABEL, nextDeeperMode } from "../lib/modes";
import { isPreselectable } from "../types";

export default function Clean() {
  const {
    scan,
    scanning,
    runScan,
    selected,
    toggle,
    setMany,
    selectedBytes,
    selectedCount,
    cleaning,
    cleanProgress,
    runClean,
    mode,
    setMode,
    setPage,
  } = useStore();
  const [openOverride, setOpen] = useState<Set<string> | null>(null);
  const [filter, setFilter] = useState("");
  const [permanent, setPermanent] = useState(false);
  const [preview, setPreview] = useState(false);
  const [confirm, setConfirm] = useState(false);

  const q = filter.trim().toLowerCase();
  const filtered = useMemo(
    () =>
      (scan?.categories ?? [])
        .map((c) => ({
          ...c,
          items: q
            ? c.items.filter((i) => i.path.toLowerCase().includes(q) || i.repo?.toLowerCase().includes(q))
            : c.items,
        }))
        .filter((c) => !q || c.items.length),
    [scan, q],
  );

  const open = openOverride ?? new Set(scan?.categories[0] ? [scan.categories[0].id] : []);

  if (!scan && !scanning)
    return (
      <div className="fade-in">
        <h1 className="mb-4 text-[17px] font-semibold tracking-tight">Clean</h1>
        <Empty
          icon={<Trash2 size={28} />}
          title="Nothing to review yet"
          hint="Run a scan to list what this mode finds. Items without a warning flag start ticked, and nothing moves until you confirm."
          action={
            <button className="btn btn-primary" onClick={() => void runScan()}>
              <Search size={14} /> Scan now
            </button>
          }
        />
      </div>
    );
  if (!scan)
    return (
      <div className="fade-in">
        <h1 className="mb-4 text-[17px] font-semibold tracking-tight">Clean</h1>
        <div className="mb-2 flex items-center gap-2 text-xs text-fg-3">
          <Spinner /> Scanning, results appear here as soon as it finishes
        </div>
        <TableSkeleton rows={8} label="Scanning" />
      </div>
    );

  const allIds = scan.categories.flatMap((c) => c.items.map((i) => i.id));
  const preselectIds = scan.categories.flatMap((c) => c.items.filter(isPreselectable).map((i) => i.id));
  const selectedItems = scan.categories.flatMap((c) => c.items.filter((i) => selected.has(i.id)));
  const deeper = nextDeeperMode(mode);
  const startClean = () => {
    if (permanent) setConfirm(true);
    else void runClean(false);
  };

  return (
    <div className="fade-in">
      <div className="mb-4 flex h-8 items-center justify-between gap-4">
        <h1 className="text-[17px] font-semibold tracking-tight">Clean</h1>
        <div className="flex items-center gap-2">
          <div className="relative">
            <Search size={13} className="pointer-events-none absolute left-2 top-2 text-fg-3" />
            <input
              className="field w-56 pl-7"
              placeholder="Filter by path or project"
              aria-label="Filter by path or project"
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
            />
          </div>
          <button className="btn btn-sm" onClick={() => setMany(allIds, false)} disabled={selected.size === 0}>
            Clear
          </button>
          <button
            className="btn btn-sm"
            onClick={() => {
              setMany(allIds, false);
              setMany(preselectIds, true);
            }}
            title="Go back to what the scan preselected: every item without a warning flag"
          >
            <ListChecks size={13} /> Reset to preselected
          </button>
        </div>
      </div>

      {scan.categories.length === 0 ? (
        <Empty
          icon={<Trash2 size={28} />}
          title={`Nothing to clean in ${MODE_LABEL[mode]} mode`}
          hint={
            deeper
              ? `Your Mac is tidy at this level. ${MODE_LABEL[deeper]} mode looks further.`
              : "Your Mac is tidy. Check back after your next big build."
          }
          action={
            deeper ? (
              <button
                className="btn"
                onClick={() => {
                  setMode(deeper);
                  setPage("overview");
                }}
              >
                Switch to {MODE_LABEL[deeper]}
              </button>
            ) : undefined
          }
        />
      ) : filtered.length === 0 ? (
        <Empty
          icon={<Search size={28} />}
          title="No matches"
          hint={`Nothing matches “${filter}”. Try a project name or a folder like node_modules.`}
          action={
            <button className="btn btn-sm" onClick={() => setFilter("")}>
              Clear filter
            </button>
          }
        />
      ) : (
        <div className="flex flex-col gap-2 pb-20">
          {filtered.map((c) => (
            <CategoryRow
              key={c.id}
              cat={c}
              open={open.has(c.id)}
              onToggle={() =>
                setOpen(() => {
                  const n = new Set(open);
                  if (n.has(c.id)) n.delete(c.id);
                  else n.add(c.id);
                  return n;
                })
              }
              selected={selected}
              toggle={toggle}
              setMany={setMany}
            />
          ))}
        </div>
      )}

      <div className="sticky -bottom-6 -mx-6 -mb-6 flex items-center gap-4 border-t border-line bg-panel/90 px-6 py-2.5 backdrop-blur">
        {cleaning && cleanProgress ? (
          <>
            <div className="flex-1">
              <div className="mb-1 flex justify-between text-xs">
                <span>
                  Cleaning {cleanProgress.done} / {cleanProgress.total}
                </span>
                <span className="tnum font-medium text-accent">{fmtBytes(cleanProgress.freedBytes)} freed</span>
              </div>
              <Progress
                label="Clean progress"
                value={(cleanProgress.done / Math.max(1, cleanProgress.total)) * 100}
                danger={permanent}
              />
            </div>
            <Spinner />
          </>
        ) : (
          <>
            <div className="tnum">
              <span className="font-semibold">{selectedCount.toLocaleString()}</span>{" "}
              {selectedCount === 1 ? "item" : "items"} ·{" "}
              <span className="font-semibold">{fmtBytes(selectedBytes)}</span> selected
            </div>
            <div className="ml-auto flex items-center gap-2">
              <Segmented
                label="Where cleaned items go"
                value={permanent ? "perm" : "trash"}
                options={[
                  { value: "trash", label: "Trash" },
                  { value: "perm", label: "Permanent" },
                ]}
                onChange={(v) => setPermanent(v === "perm")}
              />
              <button className="btn" onClick={() => setPreview(true)} disabled={selectedCount === 0}>
                <Eye size={14} /> Preview
              </button>
              <button
                className={`btn ${permanent ? "btn-danger" : "btn-primary"}`}
                onClick={startClean}
                disabled={selectedCount === 0 || scanning}
              >
                <Trash2 size={14} /> {permanent ? "Delete permanently" : "Move to Trash"}
              </button>
            </div>
          </>
        )}
      </div>

      {preview && (
        <PreviewModal
          items={selectedItems}
          permanent={permanent}
          onClose={() => setPreview(false)}
          onConfirm={() => {
            setPreview(false);
            startClean();
          }}
        />
      )}
      {confirm && (
        <PermanentDeleteModal
          count={selectedCount}
          bytes={selectedBytes}
          onClose={() => setConfirm(false)}
          onConfirm={() => {
            setConfirm(false);
            void runClean(true);
          }}
        />
      )}
    </div>
  );
}
