import { ShieldCheck } from "lucide-react";
import { NEVER_TOUCHED } from "../lib/modes";
import { Modal } from "./ui";

export function NeverTouchedSheet({ onClose }: { onClose: () => void }) {
  return (
    <Modal
      title="What is never touched"
      onClose={onClose}
      footer={
        <button className="btn btn-primary" onClick={onClose}>
          Got it
        </button>
      }
    >
      <p className="mb-3 text-xs text-fg-2">
        Scans list caches, logs, old installers and build output. AI session logs show up from Deep and are never
        selected for you. These stay off limits in every mode, including Full.
      </p>
      <ul className="flex flex-col gap-2">
        {NEVER_TOUCHED.map((n) => (
          <li key={n.title} className="flex gap-2">
            <ShieldCheck size={15} className="mt-0.5 shrink-0 text-accent" />
            <div>
              <div className="font-medium">{n.title}</div>
              <div className="text-xs text-fg-2">{n.detail}</div>
            </div>
          </li>
        ))}
      </ul>
      <p className="mt-3 text-xs text-fg-3">
        In the app, every item is listed before anything moves. The weekly clean and --headless runs remove what a scan
        preselects without asking. Files go to the Trash by default, and Undo puts the last batch back. If the Trash
        refuses something, it stays where it is until you choose to delete it permanently.
      </p>
    </Modal>
  );
}
