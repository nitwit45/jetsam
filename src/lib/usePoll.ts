import { useCallback, useEffect, useState } from "react";

/** Calls `fn` on mount and every `intervalMs` (0 = once). Keeps the last good value while an error is shown. */
export function usePoll<T>(fn: () => Promise<T>, intervalMs: number) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const load = useCallback(async () => {
    try {
      setData(await fn());
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [fn]);
  useEffect(() => {
    void load();
    if (!intervalMs) return;
    const t = setInterval(() => void load(), intervalMs);
    return () => clearInterval(t);
  }, [load, intervalMs]);
  return { data, error, load };
}
