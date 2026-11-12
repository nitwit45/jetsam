import { useState } from "react";
import { fmtBytes } from "../../lib/format";
import { TRASH_REFUSED, type CleanFailureDetail, type Item } from "../../types";
import { Modal } from "../ui";

export function PreviewModal({
  items,
  permanent,
  onClose,
  onConfirm,
}: {
  items: Item[];
  permanent: boolean;
  onClose: () => void;
  onConfirm: () => void;
}) {
  const bytes = items.reduce((a, i) => a + i.bytes, 0);
  const hasCommands = items.some((i) => i.kind === "command");
  return (
    <Modal
      title={`Preview · ${items.length} items · ${fmtBytes(bytes)}`}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>
            Close
          </button>
          <button className={`btn ${permanent ? "btn-danger" : "btn-primary"}`} onClick={onConfirm}>
            {permanent ? "Delete permanently" : "Move to Trash"}
          </button>
        </>
      }
    >
      <p className="mb-2 text-xs text-fg-2">
        This is the exact list. Nothing moves until you confirm.{" "}
        {permanent
          ? "Permanent deletion skips the Trash and cannot be undone."
          : "Files go to the Trash, and Undo puts this batch back."}
        {hasCommands && " Commands such as brew or docker clean-ups run directly and cannot be undone."}
      </p>
      <div className="mono max-h-[50vh] select-text overflow-auto rounded border border-line bg-panel-2">
        {items.map((i) => (
          <div key={i.id} className="flex gap-3 border-b border-line px-2 py-1 last:border-0">
            <span className="tnum w-16 shrink-0 text-right text-fg-3">{fmtBytes(i.bytes)}</span>
            <span className="truncate">{i.path}</span>
          </div>
        ))}
      </div>
    </Modal>
  );
}

export function PermanentDeleteModal({
  count,
  bytes,
  onClose,
  onConfirm,
}: {
  count: number;
  bytes: number;
  onClose: () => void;
  onConfirm: () => void;
}) {
  const [typed, setTyped] = useState("");
  const ready = typed === "delete";
  const noun = count === 1 ? "1 item" : `${count} items`;
  return (
    <Modal
      danger
      title="Delete permanently?"
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>
            Cancel
          </button>
          <button className="btn btn-danger" disabled={!ready} onClick={onConfirm}>
            Delete {noun}
          </button>
        </>
      }
    >
      <p className="mb-3">
        This skips the Trash. <span className="font-semibold">{fmtBytes(bytes)}</span> in {noun} cannot be recovered and
        there is no Undo.
      </p>
      <label className="mb-1 block text-xs text-fg-2" htmlFor="confirm-delete">
        Type <code className="rounded bg-danger-soft px-1 text-danger">delete</code> to confirm
      </label>
      <input
        id="confirm-delete"
        autoFocus
        className="field w-full"
        value={typed}
        onChange={(e) => setTyped(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && ready) onConfirm();
        }}
      />
    </Modal>
  );
}

const reasonText = (reason: string) => (reason === TRASH_REFUSED ? "The Trash refused this item" : reason);

/** Lists what a clean left behind. Items the Trash refused can be deleted permanently, behind the typed confirm. */
export function CleanFailuresModal({
  failures,
  onClose,
  onDeletePermanently,
}: {
  failures: CleanFailureDetail[];
  onClose: () => void;
  onDeletePermanently: (ids: string[]) => void;
}) {
  const [confirming, setConfirming] = useState(false);
  const refused = failures.filter((f) => f.reason === TRASH_REFUSED);
  const others = failures.length - refused.length;

  if (confirming)
    return (
      <PermanentDeleteModal
        count={refused.length}
        bytes={refused.reduce((a, f) => a + f.bytes, 0)}
        onClose={() => setConfirming(false)}
        onConfirm={() => onDeletePermanently(refused.map((f) => f.id))}
      />
    );
  return (
    <Modal
      title={`${failures.length} item${failures.length === 1 ? " was" : "s were"} not cleaned`}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>
            Close
          </button>
          {refused.length > 0 && (
            <button className="btn btn-danger" onClick={() => setConfirming(true)}>
              Delete these permanently
            </button>
          )}
        </>
      }
    >
      <div className="mb-2 flex flex-col gap-1 text-xs text-fg-2">
        {refused.length > 0 && (
          <p>
            The Trash refused {refused.length === 1 ? "one item" : `${refused.length} items`}, so{" "}
            {refused.length === 1 ? "it is" : "they are"} still on disk. You can delete{" "}
            {refused.length === 1 ? "it" : "them"} permanently instead.
          </p>
        )}
        {others > 0 && (
          <p>
            {refused.length > 0 ? "The rest were" : "Usually a file is"} in use or needs Full Disk Access. Quit the app
            holding it and run the clean again.
          </p>
        )}
        <p>Everything else was cleaned.</p>
      </div>
      <div className="mono max-h-[50vh] select-text overflow-auto rounded border border-line bg-panel-2 text-xs">
        {failures.map((f) => (
          <div key={f.id} className="border-b border-line px-2 py-1.5 last:border-0">
            <div className="flex gap-3">
              <span className="min-w-0 flex-1 truncate" title={f.path}>
                {f.path}
              </span>
              <span className="tnum shrink-0 text-fg-3">{fmtBytes(f.bytes)}</span>
            </div>
            <div className="text-danger">{reasonText(f.reason)}</div>
          </div>
        ))}
      </div>
    </Modal>
  );
}
