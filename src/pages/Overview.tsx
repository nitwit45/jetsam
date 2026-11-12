import { Eye, Search, ShieldCheck, Trash2 } from "lucide-react";
import { useState } from "react";
import { useStore } from "../storeContext";
import { Empty, ErrorBox, PageHeader, Segmented, Skeleton, Spinner } from "../components/ui";
import { Ring } from "../components/Ring";
import { Treemap } from "../components/Treemap";
import { NeverTouchedSheet } from "../components/NeverTouchedSheet";
import { fmtBytes, relTime, truncMiddle } from "../lib/format";
import { MODE_DESC, MODE_LABEL, MODE_OPTIONS, nextDeeperMode } from "../lib/modes";
import { MemoryPill } from "../components/memory/MemoryPill";

export default function Overview() {
  const {
    scan,
    scanning,
    scanProgress,
    scanError,
    runScan,
    mode,
    setMode,
    lastClean,
    selectedBytes,
    selectedCount,
    setPage,
  } = useStore();
  const [showNeverTouched, setShowNeverTouched] = useState(false);
  const total = scan?.diskTotal ?? 0;
  const free = scan?.diskFree ?? 0;
  const deeper = nextDeeperMode(mode);

  return (
    <div className="fade-in mx-auto max-w-[880px]">
      <PageHeader title="Overview">
        <MemoryPill />
      </PageHeader>

      <div className="card mb-4 flex flex-wrap items-center gap-6 p-6">
        {scan ? (
          <Ring total={total} free={free} wouldFree={selectedBytes} />
        ) : (
          <div className="flex h-[148px] w-[148px] shrink-0 items-center justify-center rounded-full border-[12px] border-line text-center text-xs text-fg-3">
            {scanning ? <Spinner /> : "No scan yet"}
          </div>
        )}
        <div className="flex min-w-[280px] flex-1 flex-col gap-4">
          <div>
            <Segmented label="Scan mode" value={mode} options={MODE_OPTIONS} onChange={setMode} disabled={scanning} />
            <p className="mt-2 min-h-8 text-xs text-fg-2">{MODE_DESC[mode]}</p>
            <button
              className="mt-1 inline-flex items-center gap-1 rounded text-xs text-fg-3 hover:text-fg hover:underline"
              onClick={() => setShowNeverTouched(true)}
            >
              <ShieldCheck size={12} /> What is never touched
            </button>
          </div>
          <div className="flex items-center gap-2">
            <button className="btn btn-primary h-9 px-5 text-[14px]" onClick={() => void runScan()} disabled={scanning}>
              {scanning ? <Spinner className="text-white" /> : <Search size={15} />}
              {scanning ? "Scanning…" : scan ? "Rescan" : "Scan"}
            </button>
            {scan && !scanning && (
              <button className="btn h-9" onClick={() => setPage("clean")} disabled={selectedBytes === 0}>
                <Eye size={14} /> Preview {fmtBytes(selectedBytes)}
              </button>
            )}
          </div>
          {scanning ? (
            scanProgress ? (
              <div className="mono truncate text-[11px] text-fg-3">
                {scanProgress.scanned.toLocaleString()} paths · {fmtBytes(scanProgress.bytes)} ·{" "}
                {truncMiddle(scanProgress.current, 60)}
              </div>
            ) : (
              <Skeleton className="h-3 w-64" />
            )
          ) : (
            <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-fg-3">
              <span className="inline-flex items-center gap-1">
                <Eye size={12} /> In the app, every item is listed before anything moves
              </span>
              <span className="inline-flex items-center gap-1">
                <Trash2 size={12} /> Goes to the Trash by default, with Undo
              </span>
            </div>
          )}
          <div className="text-xs text-fg-3">
            {lastClean ? (
              <>
                Last cleaned {relTime(lastClean.at / 1000)} ·{" "}
                <span className="tnum text-fg-2">{fmtBytes(lastClean.freedBytes)}</span>
              </>
            ) : (
              "Not cleaned yet"
            )}
            {scan && !scanning && selectedCount > 0 && (
              <>
                {" "}
                · <span className="tnum text-fg-2">{selectedCount.toLocaleString()}</span> items preselected, none with
                a warning flag
              </>
            )}
          </div>
        </div>
      </div>

      {scanError && (
        <div className="mb-4">
          <ErrorBox title="The scan did not finish" message={scanError} retry={() => void runScan()} />
        </div>
      )}

      {scan && scan.categories.length > 0 && (
        <div className="card p-4">
          <div className="mb-2 flex items-baseline justify-between">
            <div className="label-xs">Reclaimable by category</div>
            <div className="tnum text-xs text-fg-3">
              {fmtBytes(scan.categories.reduce((a, c) => a + c.totalBytes, 0))} found · click a block to review it
            </div>
          </div>
          <Treemap categories={scan.categories} onPick={() => setPage("clean")} />
        </div>
      )}
      {scan && scan.categories.length === 0 && (
        <div className="card">
          <Empty
            icon={<Search size={28} />}
            title={`Nothing to reclaim in ${MODE_LABEL[mode]} mode`}
            hint={
              deeper
                ? `Your Mac is already tidy at this level. ${MODE_LABEL[deeper]} looks further: ${MODE_DESC[deeper]}`
                : "Your Mac is already tidy. Check back after your next big build."
            }
            action={
              deeper ? (
                <button className="btn" onClick={() => setMode(deeper)}>
                  Switch to {MODE_LABEL[deeper]}
                </button>
              ) : undefined
            }
          />
        </div>
      )}

      {showNeverTouched && <NeverTouchedSheet onClose={() => setShowNeverTouched(false)} />}
    </div>
  );
}
