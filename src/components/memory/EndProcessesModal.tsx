import { fmtBytes } from "../../lib/format";
import type { ProcRow } from "../../types";
import { Modal } from "../ui";

export function EndProcessesModal({
  rows,
  onClose,
  onConfirm,
}: {
  rows: ProcRow[];
  onClose: () => void;
  onConfirm: () => void;
}) {
  return (
    <Modal
      title={`End ${rows.length} process${rows.length === 1 ? "" : "es"}`}
      danger
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>
            Cancel
          </button>
          <button className="btn btn-danger" onClick={onConfirm}>
            End {rows.length} and reclaim
          </button>
        </>
      }
    >
      <p className="mb-2 text-xs text-fg-2">
        Each one is asked to quit, then force-ended after 3 seconds. Unsaved work in them is lost. The regular reclaim
        (ending orphan helpers and flushing the file cache) runs as well.
      </p>
      <ul className="max-h-64 overflow-auto rounded border border-line text-sm">
        {rows.map((r) => (
          <li key={r.pid} className="flex h-7 items-center border-b border-line px-2 last:border-0">
            <span className="flex-1 truncate">{r.name}</span>
            <span className="tnum text-xs text-fg-3">
              {r.app} · {r.pid}
            </span>
            <span className="tnum ml-3 w-20 text-right">{fmtBytes(r.rss)}</span>
          </li>
        ))}
      </ul>
    </Modal>
  );
}
