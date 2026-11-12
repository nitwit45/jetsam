import { useEffect, useState, type ReactNode } from "react";
import { api, MOCK } from "../api";
import { useStore } from "../storeContext";
import { PageHeader, Segmented } from "../components/ui";
import { MODE_LABEL } from "../lib/modes";
import { SCHEDULE_MODES, type ScheduleMode } from "../types";

const SCHEDULE_OPTIONS = SCHEDULE_MODES.map((value) => ({ value, label: MODE_LABEL[value] }));
const MANUAL_ONLY =
  "Deep and Full are manual only: they reach into your projects and AI logs, so you see the list first.";
/** Where the backend appends one line per cleaned item (`cleaner::ops_log_path`). */
const OPS_LOG_PATH = "~/Library/Application Support/Jetsam/ops.log";

function useAppVersion() {
  const [version, setVersion] = useState<string | null>(null);
  useEffect(() => {
    api
      .appVersion()
      .then(setVersion)
      .catch(() => setVersion(null));
  }, []);
  return version;
}

export default function Settings() {
  const { settings, setSettings, pushToast, fdaGranted, recheckFda } = useStore();
  const [saving, setSaving] = useState(false);
  const version = useAppVersion();
  const applySchedule = async (enabled: boolean, mode: ScheduleMode) => {
    setSaving(true);
    try {
      const ok = await api.schedule(enabled, mode);
      setSettings({ scheduleEnabled: ok && enabled, scheduleMode: mode });
      if (enabled && !ok)
        pushToast({ kind: "error", text: "Could not set up the weekly schedule. Try again, or run cleans by hand." });
    } catch (e) {
      pushToast({ kind: "error", text: e instanceof Error ? e.message : String(e) });
    } finally {
      setSaving(false);
    }
  };
  const fdaHint =
    fdaGranted === false
      ? "Off. Mail, Safari, Messages and some caches are hidden from Jetsam."
      : fdaGranted
        ? "On. Jetsam can see every user-library location."
        : "Checking…";

  return (
    <div className="fade-in mx-auto max-w-[720px]">
      <PageHeader title="Settings" />
      <div className="card mb-4">
        <SettingRow label="Full Disk Access" hint={fdaHint}>
          <span
            className={`tnum text-xs font-medium ${fdaGranted === false ? "text-warn" : fdaGranted ? "text-accent" : "text-fg-3"}`}
          >
            {fdaGranted === false ? "Off" : fdaGranted ? "On" : "—"}
          </span>
          <button className="btn btn-sm" onClick={() => void api.openFdaSettings()}>
            Open System Settings
          </button>
          <button className="btn btn-sm" onClick={() => void recheckFda()}>
            Recheck
          </button>
        </SettingRow>
      </div>
      <div className="card">
        <SettingRow
          label="Recent window"
          hint="Projects touched within this many days count as active. Deep shows their build folders but cannot select them; only Full cleans inside them."
        >
          <input
            id="recent-days"
            aria-label="Recent window in days"
            type="number"
            min={1}
            max={365}
            className="field tnum w-20 text-right"
            value={settings.recentDays}
            onChange={(e) => setSettings({ recentDays: Math.max(1, Math.min(365, Number(e.target.value) || 14)) })}
          />
          <span className="text-fg-2">days</span>
        </SettingRow>
        <SettingRow
          label="Weekly clean"
          hint={
            settings.scheduleEnabled
              ? `Runs a ${MODE_LABEL[settings.scheduleMode]} clean every Sunday at 10:00 with no preview. What a scan would preselect goes to the Trash, and the Homebrew and Docker prunes run, which are permanent. It never empties the Trash. ${MANUAL_ONLY}`
              : `Off. Turn on to run a Light or Standard clean every Sunday at 10:00 in the background. ${MANUAL_ONLY}`
          }
        >
          <Segmented
            label="Scheduled clean mode"
            value={settings.scheduleMode}
            disabled={saving}
            options={SCHEDULE_OPTIONS}
            onChange={(m) => void applySchedule(settings.scheduleEnabled, m)}
          />
          <button
            role="switch"
            aria-checked={settings.scheduleEnabled}
            aria-label="Weekly clean"
            disabled={saving}
            onClick={() => void applySchedule(!settings.scheduleEnabled, settings.scheduleMode)}
            className={`relative h-5 w-9 rounded-full transition ${settings.scheduleEnabled ? "bg-accent" : "bg-line-strong"} disabled:opacity-50`}
          >
            <span
              className={`absolute top-0.5 h-4 w-4 rounded-full bg-white shadow transition ${settings.scheduleEnabled ? "left-[18px]" : "left-0.5"}`}
            />
          </button>
        </SettingRow>
        <SettingRow
          label="Operations log"
          hint="Every clean is recorded here, including where each file landed in the Trash so Undo can put it back."
        >
          <code className="select-text text-xs text-fg-2">{OPS_LOG_PATH}</code>
        </SettingRow>
        <SettingRow label="Version">
          <span className="tnum text-fg-2">
            {version ?? "—"}
            {MOCK ? " · sample data" : ""}
          </span>
        </SettingRow>
      </div>
    </div>
  );
}

function SettingRow({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div className="flex items-center gap-4 border-b border-line px-4 py-3 last:border-0">
      <div className="flex-1">
        <div className="font-medium">{label}</div>
        {hint && <div className="text-xs text-fg-2">{hint}</div>}
      </div>
      <div className="flex items-center gap-2">{children}</div>
    </div>
  );
}
