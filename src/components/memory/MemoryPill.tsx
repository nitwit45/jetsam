import { useStore } from "../../storeContext";
import { usePoll } from "../../lib/usePoll";
import { Skeleton } from "../ui";
import { memoryFn, PRESSURE_TONE } from "./shared";

/** Compact pressure readout shown in the Overview header; click opens the Memory page. */
export function MemoryPill() {
  const { data: m, error } = usePoll(memoryFn, 15000);
  const { setPage } = useStore();
  if (error) return null;
  if (!m) return <Skeleton className="h-6 w-36 rounded-full" />;
  return (
    <button
      onClick={() => setPage("memory")}
      title="Open Memory"
      className={`flex h-6 items-center gap-1.5 rounded-full border px-2.5 text-xs font-medium ${PRESSURE_TONE[m.pressure]}`}
    >
      <span className="h-1.5 w-1.5 rounded-full bg-current" /> Memory {m.pressure} · {Math.round(m.freePct)}% free
    </button>
  );
}
