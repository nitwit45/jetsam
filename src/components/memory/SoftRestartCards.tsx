import { RotateCw } from "lucide-react";
import { fmtBytes } from "../../lib/format";
import type { ProcRow, SoftRestartApp } from "../../types";
import { Spinner } from "../ui";

const RESTART: { key: SoftRestartApp; label: string; group: string }[] = [
  { key: "chrome", label: "Chrome", group: "Google Chrome" },
  { key: "vscode", label: "VS Code", group: "Visual Studio Code" },
  { key: "cursor", label: "Cursor", group: "Cursor" },
];

export function SoftRestartCards({
  rows,
  restarting,
  disabled,
  messages,
  onRestart,
}: {
  rows: ProcRow[];
  restarting: SoftRestartApp | null;
  disabled: boolean;
  messages: Partial<Record<SoftRestartApp, string>>;
  onRestart: (app: SoftRestartApp) => void;
}) {
  const running = RESTART.map((c) => {
    const list = rows.filter((r) => r.app === c.group);
    const agents = c.key === "chrome" ? 0 : rows.filter((r) => r.name === "claude" && r.host === c.key).length;
    return { ...c, list, rss: list.reduce((a, r) => a + r.rss, 0), agents };
  }).filter((c) => c.list.length > 0);
  if (running.length === 0) return null;
  return (
    <div className="mb-4">
      <div className="label-xs mb-2 px-0.5">Soft restart</div>
      <div className="grid grid-cols-3 gap-2">
        {running.map((c) => {
          const blocked = c.agents > 0;
          const msg = blocked
            ? `${c.agents} Claude Code agent${c.agents === 1 ? " is" : "s are"} running inside ${c.group}. Restarting would stop ${c.agents === 1 ? "it" : "them"}, so finish first.`
            : messages[c.key];
          return (
            <div key={c.key} className="card flex flex-col p-4">
              <div className="flex items-start justify-between gap-2">
                <div className="font-medium">{c.label}</div>
                <div className="flex items-center gap-1">
                  {blocked && (
                    <span className="rounded-full border border-warn/40 bg-warn/10 px-1.5 text-[10px] font-medium text-warn">
                      {c.agents} agent{c.agents === 1 ? "" : "s"} running
                    </span>
                  )}
                  <span className="tnum text-xs text-fg-2">{fmtBytes(c.rss)}</span>
                </div>
              </div>
              <p className="mt-1 flex-1 text-xs text-fg-2">
                Quits and reopens with your tabs and windows back. Frees what it had swapped out.
              </p>
              {msg && <p className={`mt-2 text-xs ${blocked ? "text-warn" : "text-danger"}`}>{msg}</p>}
              <div className="mt-3">
                <button
                  className="btn btn-sm"
                  disabled={blocked || restarting !== null || disabled}
                  onClick={() => onRestart(c.key)}
                >
                  {restarting === c.key ? <Spinner /> : <RotateCw size={13} />} Restart {c.label}
                </button>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
