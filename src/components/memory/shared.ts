import { api } from "../../api";
import type { ProcRow } from "../../types";

export const memoryFn = () => api.memory();
export const procsFn = () => api.memProcesses();

export const PRESSURE_TONE = {
  normal: "text-accent border-accent/40 bg-accent-soft",
  warn: "text-warn border-warn/40 bg-warn/10",
  critical: "text-danger border-danger/40 bg-danger-soft",
} as const;

export const has = (r: ProcRow, reason: string) => r.reasons.includes(reason);
export const isProtected = (r: ProcRow) => has(r, "protected");
export const protectTip = (r: ProcRow) => {
  if (has(r, "vm")) return "OrbStack manages its own memory and hands it back when idle. Nothing to do here.";
  if (has(r, "headless")) return "Started by a running Claude Code agent. End the agent's task instead.";
  if (r.app === "Claude Code") return "A running Claude Code agent. Never touched.";
  return "Protected. Never touched.";
};
