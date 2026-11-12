import type { ScanMode } from "../types";

export const MODE_ORDER: ScanMode[] = ["light", "standard", "deep", "full"];

export const MODE_LABEL: Record<ScanMode, string> = {
  light: "Light",
  standard: "Standard",
  deep: "Deep",
  full: "Full",
};

export const MODE_DESC: Record<ScanMode, string> = {
  light:
    "Caches, logs, old .dmg and .pkg installers, and Homebrew and Docker build leftovers, plus what is in the Trash. Files go to the Trash. Emptying the Trash and the Homebrew and Docker prunes are permanent.",
  standard: "Light, plus package-manager caches and Xcode and Android leftovers.",
  deep: "Standard, plus build folders in inactive projects, stale merged worktrees, unused Docker images, and AI session logs for you to review.",
  full: "Deep, including projects you are working on now. Nothing git-tracked is ever touched.",
};

export const MODE_OPTIONS = MODE_ORDER.map((value) => ({ value, label: MODE_LABEL[value] }));

export const NEVER_TOUCHED = [
  {
    title: "Your personal folders",
    detail:
      "Documents, Desktop, Pictures, Movies and Music are never removed. Build folders inside projects kept there can be listed in Deep and Full, and Aged Files can show big files there for you to decide on.",
  },
  {
    title: "Git-tracked files and repos",
    detail: "Anything git tracks is skipped, even inside a build folder, and a repository is never removed.",
  },
  {
    title: "Docker volumes and containers",
    detail: "Only images and the build cache are pruned. Volumes and containers can hold your data and are left alone.",
  },
  {
    title: "Claude Code agents",
    detail:
      "A running Claude Code agent, anything it started, and the terminal or editor it runs in are never ended or restarted.",
  },
  {
    title: "The app you were last using",
    detail: "The app you used before switching to Jetsam, and its helpers, are never ended to free memory.",
  },
  {
    title: "Jetsam and macOS processes",
    detail:
      "Jetsam itself and processes under /System and /usr/libexec are never ended. Virtual machines are only labelled.",
  },
  {
    title: "Keychains, mail and credentials",
    detail:
      "Keychains, Mail, Messages, Photos libraries, iCloud Drive, iPhone backups, ~/.ssh, ~/.gnupg, ~/.aws, ~/.kube, ~/.docker, Claude Code memory and the system folders stay as they are.",
  },
];

export function nextDeeperMode(mode: ScanMode): ScanMode | null {
  const i = MODE_ORDER.indexOf(mode);
  return i >= 0 && i < MODE_ORDER.length - 1 ? MODE_ORDER[i + 1] : null;
}
