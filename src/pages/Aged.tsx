import { ArrowDown, ArrowUp, Clock, FolderOpen, Trash2 } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../api";
import { useStore } from "../storeContext";
import { Empty, ErrorBox, PageHeader, Segmented, Spinner, TableSkeleton } from "../components/ui";
import { PermanentDeleteModal } from "../components/clean/CleanModals";
import { basename, dirname, fmtBytes, fmtDate, relTime, truncMiddle } from "../lib/format";
import { AGED_SCAN_ID, TRASH_REFUSED, type Item } from "../types";

type SortKey = "bytes" | "opened" | "mtime" | "name";
const MB = 1e6;
const STEPS = [50, 100, 250, 500, 1000];
const stepLabel = (mb: number) => (mb >= 1000 ? "1 GB" : `${mb} MB`);

const AGE_OPTIONS = [
  { value: "30", label: "30 d" },
  { value: "90", label: "90 d" },
  { value: "180", label: "180 d" },
  { value: "365", label: "1 y" },
];
type AgeDays = (typeof AGE_OPTIONS)[number]["value"];
const ageLabel = (days: AgeDays) => (days === "365" ? "a year" : `${days} days`);

const lastOpened = (it: Item) => it.lastUsed ?? it.mtime;

/** The backend tags each aged file with a rough type; anything untagged reads as a plain file. */
const FILE_TYPES: Record<string, string> = {
  video: "video",
  archive: "archive",
  "disk-image": "disk image",
  model: "model",
};
const fileType = (it: Item) => FILE_TYPES[it.flags.find((f) => f in FILE_TYPES) ?? ""] ?? "file";
const errText = (e: unknown) => (e instanceof Error ? e.message : String(e));

export default function Aged() {
  const { pushToast, offerUndo, retireUndo } = useStore();
  const [step, setStep] = useState(2);
  const [age, setAge] = useState<AgeDays>("90");
  const [items, setItems] = useState<Item[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [sort, setSort] = useState<{ k: SortKey; d: 1 | -1 }>({ k: "bytes", d: -1 });
  const [busy, setBusy] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<Item | null>(null);
  const minBytes = STEPS[step] * MB;
  const request = useRef(0);

  // The home-folder walk can be slow; only the newest request may update the list.
  const load = useCallback(async () => {
    const id = ++request.current;
    setLoading(true);
    setError(null);
    try {
      const found = await api.aged(minBytes, Number(age));
      if (id === request.current) setItems(found);
    } catch (e) {
      if (id === request.current) setError(errText(e));
    } finally {
      if (id === request.current) setLoading(false);
    }
  }, [minBytes, age]);
  useEffect(() => {
    void load();
  }, [load]);

  const sorted = [...(items ?? [])].sort((a, b) => {
    const v =
      sort.k === "bytes"
        ? a.bytes - b.bytes
        : sort.k === "mtime"
          ? a.mtime - b.mtime
          : sort.k === "opened"
            ? lastOpened(a) - lastOpened(b)
            : basename(a.path).localeCompare(basename(b.path));
    return v * sort.d;
  });
  const th = (k: SortKey, label: string, cls = "") => (
    <button
      className={`flex items-center gap-1 rounded hover:text-fg ${cls}`}
      aria-sort={sort.k === k ? (sort.d === -1 ? "descending" : "ascending") : "none"}
      onClick={() => setSort((s) => ({ k, d: s.k === k ? (s.d === 1 ? -1 : 1) : -1 }))}
    >
      {label}
      {sort.k === k && (sort.d === -1 ? <ArrowDown size={11} /> : <ArrowUp size={11} />)}
    </button>
  );
  const removeRow = (it: Item) => setItems((xs) => xs?.filter((x) => x.id !== it.id) ?? null);
  const clean = async (it: Item, permanent: boolean) => {
    setBusy(it.id);
    retireUndo();
    const name = basename(it.path);
    try {
      const { failed } = await api.clean(AGED_SCAN_ID, [it.id], permanent);
      if (failed.length === 0) {
        removeRow(it);
        if (permanent) pushToast({ kind: "success", text: `Deleted ${name} permanently` });
        else offerUndo(`Moved ${name} to the Trash`);
      } else if (failed[0].reason === TRASH_REFUSED) {
        pushToast({
          kind: "error",
          text: `The Trash refused ${name}. It is still on disk.`,
          action: { label: "Delete permanently", run: () => setConfirmDelete(it) },
        });
      } else {
        pushToast({ kind: "error", text: `Could not clean ${name}: ${failed[0].reason}` });
      }
    } catch (e) {
      pushToast({ kind: "error", text: `Could not clean ${name}: ${errText(e)}` });
    } finally {
      setBusy(null);
    }
  };
  const reveal = (it: Item) =>
    api.reveal(it.path).catch((e) => pushToast({ kind: "error", text: `Could not open Finder: ${errText(e)}` }));
  const total = (items ?? []).reduce((a, i) => a + i.bytes, 0);

  return (
    <div className="fade-in">
      <PageHeader
        title="Aged Files"
        subtitle={`Big files in your home folder not opened for ${ageLabel(age)}. Nothing is selected for you here.`}
      >
        <span className="whitespace-nowrap text-xs text-fg-2">Not opened for</span>
        <Segmented label="Not opened for" value={age} options={AGE_OPTIONS} onChange={setAge} />
        <label className="ml-2 whitespace-nowrap text-xs text-fg-2" htmlFor="aged-min">
          Min size
        </label>
        <input
          id="aged-min"
          type="range"
          min={0}
          max={STEPS.length - 1}
          step={1}
          value={step}
          onChange={(e) => setStep(Number(e.target.value))}
          className="w-24"
        />
        <span className="tnum w-14 text-xs font-medium">{stepLabel(STEPS[step])}</span>
      </PageHeader>
      {error ? (
        <ErrorBox title="Could not look for aged files" message={error} retry={() => void load()} />
      ) : items === null ? (
        <TableSkeleton rows={6} label="Looking for big, old files" />
      ) : items.length === 0 ? (
        <Empty
          icon={<Clock size={28} />}
          title="Nothing big and dusty"
          hint={`No files over ${stepLabel(STEPS[step])} left unopened for ${ageLabel(age)}.${step > 0 ? " Lower the minimum size to widen the net." : ""}`}
          action={
            step > 0 ? (
              <button className="btn btn-sm" onClick={() => setStep(step - 1)}>
                Try {stepLabel(STEPS[step - 1])}
              </button>
            ) : undefined
          }
        />
      ) : (
        <>
          <div className="mb-2 text-xs text-fg-3">
            {items.length} file{items.length === 1 ? "" : "s"} · <span className="tnum">{fmtBytes(total)}</span> · hover
            a row to reveal it in Finder or move it to the Trash
          </div>
          <div className={`card overflow-hidden transition-opacity ${loading ? "opacity-60" : ""}`} aria-busy={loading}>
            <div className="flex h-7 items-center gap-3 border-b border-line bg-panel-2 px-3 text-[11px] font-medium text-fg-3">
              {th("name", "Name / path", "flex-1")}
              {th("bytes", "Size", "w-20 justify-end")}
              {th("opened", "Last opened", "w-24")}
              {th("mtime", "Modified", "w-24")}
              <span className="w-16">Type</span>
              <span className="w-[72px]" />
            </div>
            {sorted.map((it) => (
              <div
                key={it.id}
                className="group flex h-10 items-center gap-3 border-b border-line px-3 last:border-0 hover:bg-hover"
              >
                <div className="min-w-0 flex-1">
                  <div className="truncate font-medium">{basename(it.path)}</div>
                  <div className="mono truncate text-[11px] text-fg-3" title={it.path}>
                    {truncMiddle(dirname(it.path), 70)}
                  </div>
                </div>
                <span className="tnum w-20 text-right">{fmtBytes(it.bytes)}</span>
                <span className="tnum w-24 text-fg-2" title={fmtDate(lastOpened(it))}>
                  {relTime(lastOpened(it))}
                </span>
                <span className="tnum w-24 text-fg-2" title={relTime(it.mtime)}>
                  {fmtDate(it.mtime)}
                </span>
                <span className="w-16 text-xs text-fg-3">{fileType(it)}</span>
                <div className="flex w-[72px] justify-end gap-1 opacity-0 focus-within:opacity-100 group-hover:opacity-100">
                  <button
                    className="btn btn-ghost btn-sm px-1.5"
                    title="Reveal in Finder"
                    aria-label={`Reveal ${basename(it.path)} in Finder`}
                    onClick={() => void reveal(it)}
                  >
                    <FolderOpen size={14} />
                  </button>
                  <button
                    className="btn btn-ghost btn-sm px-1.5 text-danger"
                    title="Move to Trash"
                    aria-label={`Move ${basename(it.path)} to Trash`}
                    disabled={busy !== null || loading}
                    onClick={() => void clean(it, false)}
                  >
                    {busy === it.id ? <Spinner /> : <Trash2 size={14} />}
                  </button>
                </div>
              </div>
            ))}
          </div>
        </>
      )}
      {confirmDelete && (
        <PermanentDeleteModal
          count={1}
          bytes={confirmDelete.bytes}
          onClose={() => setConfirmDelete(null)}
          onConfirm={() => {
            setConfirmDelete(null);
            void clean(confirmDelete, true);
          }}
        />
      )}
    </div>
  );
}
