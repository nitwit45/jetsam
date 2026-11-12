import { Check, Copy, ExternalLink } from "lucide-react";
import { useCallback, useEffect, useState, type ReactNode } from "react";
import { api } from "../api";
import { useStore } from "../storeContext";
import { ErrorBox, PageHeader, Skeleton, Spinner, StatusPill } from "../components/ui";
import type { FreshenAction, FreshenStatus } from "../types";

const CHROME_URL = "chrome://settings/performance";

export default function Freshen() {
  const { pushToast } = useStore();
  const [st, setSt] = useState<FreshenStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<FreshenAction | null>(null);
  const [copied, setCopied] = useState(false);
  const load = useCallback(async () => {
    try {
      setSt(await api.freshenStatus());
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);
  useEffect(() => {
    void load();
  }, [load]);

  const apply = async (a: FreshenAction) => {
    setBusy(a);
    try {
      const r = await api.freshen(a);
      pushToast({ kind: r.ok ? "success" : "error", text: r.message });
      await load();
    } catch (e) {
      pushToast({ kind: "error", text: e instanceof Error ? e.message : String(e) });
    } finally {
      setBusy(null);
    }
  };
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(CHROME_URL);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      pushToast({ kind: "error", text: `Could not copy. The address is ${CHROME_URL}` });
    }
  };

  if (error)
    return (
      <div className="fade-in mx-auto max-w-[880px]">
        <PageHeader title="Freshen" />
        <ErrorBox title="Could not check system state" message={error} retry={() => void load()} />
      </div>
    );
  if (!st) return <FreshenSkeleton />;

  const hibernateOff = st.hibernateMode === 0;
  const cards: { action: FreshenAction; title: string; status: ReactNode; desc: string; label: string }[] = [
    {
      action: "thin_snapshots",
      title: "Thin local Time Machine snapshots",
      status: <StatusPill tone="warn">Permanent</StatusPill>,
      desc: "Asks macOS to delete the local Time Machine snapshots on this disk to free their space. This is permanent: you cannot restore from a thinned snapshot. Backups on your Time Machine disk are untouched, and new local snapshots appear on the next backup.",
      label: "Thin now",
    },
    {
      action: hibernateOff ? "hibernate_default" : "hibernate_off",
      title: "Hibernation sleep image",
      status: (
        <StatusPill tone={hibernateOff ? "ok" : "muted"}>
          {hibernateOff ? "Off" : `On (mode ${st.hibernateMode})`}
        </StatusPill>
      ),
      desc: hibernateOff
        ? "The sleep image is off. Restore the default to get deep hibernate on low battery back. Asks for your password."
        : "macOS keeps a sleep image as large as your RAM. Turning it off frees that space; a laptop loses deep hibernate on low battery. Asks for your password.",
      label: hibernateOff ? "Restore default" : "Turn off",
    },
    {
      action: "vscode_excludes",
      title: "VS Code watcher excludes",
      status: (
        <StatusPill tone={st.vscodeExcludes ? "ok" : "muted"}>
          {st.vscodeExcludes ? "Configured" : "Not configured"}
        </StatusPill>
      ),
      desc: "Stops VS Code from indexing node_modules, build output and similar folders, so its helper process stays small.",
      label: st.vscodeExcludes ? "Re-apply" : "Apply",
    },
    {
      action: "purge_inactive",
      title: "Purge inactive memory",
      status: <StatusPill tone="muted">On demand</StatusPill>,
      desc: "Flushes the disk cache and inactive pages for a short-lived bump in free memory. Handy right before a big build. Asks for your password.",
      label: "Purge now",
    },
  ];

  return (
    <div className="fade-in mx-auto max-w-[880px]">
      <PageHeader
        title="Freshen"
        subtitle="One-off system tweaks. Each says what it changes and whether it can be undone."
      />
      <div className="grid grid-cols-2 gap-4">
        {cards.map((c) => (
          <FreshenCard key={c.action} title={c.title} status={c.status} desc={c.desc}>
            <button className="btn btn-sm" disabled={busy !== null} onClick={() => void apply(c.action)}>
              {busy === c.action ? <Spinner /> : null}
              {c.label}
            </button>
          </FreshenCard>
        ))}

        <FreshenCard
          title="Chrome Memory Saver"
          status={
            <StatusPill tone={st.chromeRunning ? "warn" : "muted"}>
              {st.chromeRunning ? "Chrome running" : "Chrome not running"}
            </StatusPill>
          }
          desc="Chrome does not let other apps open its settings. Paste this address into Chrome and turn on Memory Saver so idle tabs stop holding memory."
        >
          <div className="flex w-full items-center gap-2">
            <code className="field flex flex-1 items-center bg-panel-2 text-xs">{CHROME_URL}</code>
            <button className="btn btn-sm" onClick={() => void copy()}>
              {copied ? <Check size={13} className="text-accent" /> : <Copy size={13} />} {copied ? "Copied" : "Copy"}
            </button>
          </div>
        </FreshenCard>

        <FreshenCard
          title="Spotlight indexing"
          status={
            <StatusPill tone={st.spotlightDevVolume ? "muted" : "ok"}>
              {st.spotlightDevVolume ? "On" : "Off"}
            </StatusPill>
          }
          desc={
            st.spotlightDevVolume
              ? "Spotlight indexing is on for your Mac's data volume, so it also indexes node_modules and build output while they change. To leave your projects out, add their folder under Spotlight > Search Privacy. Jetsam checks only the volume, so this card still says On after you do."
              : "Jetsam did not find Spotlight indexing on for your Mac's data volume. Nothing to do here."
          }
        >
          <a
            className="btn btn-sm"
            href="x-apple.systempreferences:com.apple.Siri-Settings.extension"
            target="_blank"
            rel="noreferrer"
          >
            <ExternalLink size={13} /> Open Spotlight settings
          </a>
        </FreshenCard>

        <FreshenCard
          title="OrbStack"
          status={
            <StatusPill tone={st.orbstackRunning ? "ok" : "muted"}>
              {st.orbstackRunning ? "Running" : "Not running"}
            </StatusPill>
          }
          desc={
            st.orbstackRunning
              ? "OrbStack is up. It hands memory back on its own when containers go idle, so there is nothing to tune here. Stop containers you are not using and it shrinks by itself."
              : "OrbStack is not running, so no VM is holding memory."
          }
        />
      </div>
    </div>
  );
}

function FreshenCard({
  title,
  status,
  desc,
  children,
}: {
  title: string;
  status: ReactNode;
  desc: string;
  children?: ReactNode;
}) {
  return (
    <div className="card flex flex-col p-4">
      <div className="flex items-start justify-between gap-2">
        <div className="font-medium">{title}</div>
        {status}
      </div>
      <p className="mt-1 flex-1 text-xs text-fg-2">{desc}</p>
      {children && <div className="mt-3 flex">{children}</div>}
    </div>
  );
}

function FreshenSkeleton() {
  return (
    <div className="fade-in mx-auto max-w-[880px]" role="status" aria-label="Checking system state">
      <PageHeader title="Freshen" />
      <div className="grid grid-cols-2 gap-4">
        {Array.from({ length: 6 }, (_, i) => (
          <div key={i} className="card flex flex-col gap-3 p-4">
            <div className="flex justify-between">
              <Skeleton className="h-3.5 w-40" />
              <Skeleton className="h-5 w-20 rounded-full" />
            </div>
            <Skeleton className="h-3 w-full" />
            <Skeleton className="h-3 w-3/4" />
            <Skeleton className="h-6 w-20" />
          </div>
        ))}
      </div>
    </div>
  );
}
