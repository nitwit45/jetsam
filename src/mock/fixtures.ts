/**
 * Sample data for mock mode. Every name and path here is made up; the shapes mirror what the Rust commands
 * return so the UI can be developed and screenshotted without a real Mac to scan.
 */
import type { Category, Item, ProcRow, ScanMode } from "../types";

export const GB = 1e9;
export const MB = 1e6;
export const HOME = "/Users/you";
const PROJECTS_DIR = `${HOME}/Projects`;

const now = Math.floor(Date.now() / 1000);
export const daysAgo = (n: number) => now - Math.round(n * 86400);

const MODE_RANK: Record<ScanMode, number> = { light: 0, standard: 1, deep: 2, full: 3 };

interface Project {
  name: string;
  idleDays: number;
}
export const PROJECTS: Project[] = [
  { name: "acme-web", idleDays: 2 },
  { name: "api-server", idleDays: 9 },
  { name: "side-project", idleDays: 41 },
  { name: "ml-notebook", idleDays: 75 },
  { name: "blog", idleDays: 130 },
];

type Row = [path: string, bytes: number, ageDays: number, flags?: string[]];

function items(cat: string, rows: Row[], kind: Item["kind"] = "dir"): Item[] {
  return rows.map(([path, bytes, ageDays, flags = []], i) => ({
    id: `${cat}-${i}`,
    path,
    bytes,
    mtime: daysAgo(ageDays),
    kind,
    flags,
  }));
}

function command(cat: string, label: string, bytes: number): Item[] {
  return [{ id: `${cat}-0`, path: label, bytes, mtime: now, kind: "command", flags: [] }];
}

const appCaches = (): Item[] =>
  items("app_caches", [
    [`${HOME}/Library/Caches/com.google.Chrome`, 1.9 * GB, 1],
    [`${HOME}/Library/Caches/com.spotify.client`, 780 * MB, 3],
    [`${HOME}/Library/Caches/com.figma.Desktop`, 310 * MB, 6],
    [`${HOME}/Library/Caches/com.tinyspeck.slackmacgap`, 260 * MB, 2],
    [`${HOME}/Library/Logs/DiagnosticReports`, 95 * MB, 12],
    [`${HOME}/Library/Logs/JetBrains`, 140 * MB, 30],
    ["/private/var/folders/q4/7x2k9m1d0b5f/C/com.apple.QuickLook.thumbnailcache", 220 * MB, 4, ["report-only"]],
  ]);

const trash = (): Item[] =>
  items("trash", [
    [`${HOME}/.Trash/Screen Recording 2026-09-02.mov`, 1.3 * GB, 21],
    [`${HOME}/.Trash/old-export.zip`, 640 * MB, 33],
  ]);

const installers = (): Item[] =>
  items(
    "installers",
    [
      [`${HOME}/Downloads/Docker.dmg`, 610 * MB, 62],
      [`${HOME}/Downloads/Figma-125.dmg`, 280 * MB, 40],
      [`${HOME}/Downloads/node-v22.11.0.pkg`, 75 * MB, 95],
    ],
    "file",
  );

const docker = (mode: ScanMode): Item[] =>
  MODE_RANK[mode] >= MODE_RANK.deep
    ? command("docker", "docker prune (unused images, build cache)", 9.8 * GB)
    : command("docker", "docker prune (dangling images, build cache)", 3.2 * GB);

const pkgCaches = (): Item[] =>
  items("pkg_caches", [
    [`${HOME}/Library/pnpm/store`, 3.4 * GB, 1],
    [`${HOME}/.npm/_cacache`, 2.6 * GB, 2],
    [`${HOME}/.gradle/caches`, 1.9 * GB, 18],
    [`${HOME}/Library/Caches/pip`, 1.1 * GB, 9],
    [`${HOME}/.cargo/registry/cache`, 840 * MB, 5],
    [`${HOME}/.nuget/packages`, 520 * MB, 44, ["report-only"]],
  ]);

const xcodeAndroid = (): Item[] =>
  items("xcode_android", [
    [`${HOME}/.android/avd/Pixel_8_API_34.avd`, 6.1 * GB, 120],
    [`${HOME}/Library/Developer/Xcode/iOS DeviceSupport/17.5 (21F79)`, 4.4 * GB, 160],
    [`${HOME}/Library/Developer/Xcode/DerivedData/SideProject-gxhqbwlmzc`, 3.8 * GB, 41],
    [`${HOME}/Library/Developer/CoreSimulator/Caches`, 1.2 * GB, 14],
  ]);

const aiSessions = (): Item[] =>
  items(
    "ai_sessions",
    [
      [`${HOME}/.claude/projects/-Users-you-Projects-side-project/3f9c1a2e-5b7d.jsonl`, 48 * MB, 52, ["review"]],
      [`${HOME}/.claude/projects/-Users-you-Projects-blog/a71e04d9-22c8.jsonl`, 31 * MB, 133, ["review"]],
      [`${HOME}/.codex/sessions/2026/08/rollout-2026-08-14T09-12-55.jsonl`, 22 * MB, 88, ["review"]],
      [`${HOME}/Library/Application Support/Cursor/logs/20260812T101500`, 120 * MB, 90, ["review"]],
    ],
    "file",
  );

/** Build folders per project. Deep marks the ones in recently active projects; Full lists them unflagged. */
function projectArtifacts(mode: ScanMode, recentDays: number): Item[] {
  const folders: [project: string, dir: string, bytes: number, flags?: string[]][] = [
    ["acme-web", "node_modules", 1.6 * GB],
    ["acme-web", ".next", 740 * MB],
    ["api-server", "target", 3.9 * GB],
    ["side-project", "node_modules", 1.1 * GB],
    ["side-project", "dist", 180 * MB, ["dirty"]],
    ["ml-notebook", ".venv", 2.3 * GB],
    ["ml-notebook", "__pycache__", 60 * MB],
    ["blog", "node_modules", 820 * MB],
    ["blog", "dist", 95 * MB, ["git-tracked"]],
  ];
  return folders.map(([project, dir, bytes, flags = []], i) => {
    const idle = PROJECTS.find((p) => p.name === project)?.idleDays ?? 0;
    const recent = mode === "deep" && idle < recentDays;
    return {
      id: `project_artifacts-${i}`,
      path: `${PROJECTS_DIR}/${project}/${dir}`,
      bytes,
      mtime: daysAgo(idle + 1),
      kind: "dir",
      repo: project,
      repoLastActive: daysAgo(idle),
      flags: recent ? [...flags, "recent"] : flags,
    };
  });
}

const worktrees = (): Item[] =>
  items("worktrees", [
    [`${PROJECTS_DIR}/acme-web/.worktrees/old-checkout-flow`, 1.2 * GB, 64, ["worktree"]],
    [`${PROJECTS_DIR}/api-server/.worktrees/spike-grpc`, 410 * MB, 38, ["worktree", "dirty"]],
  ]);

interface CategoryDef {
  id: string;
  label: string;
  mode: ScanMode;
  items: (mode: ScanMode, recentDays: number) => Item[];
}

const CATEGORIES: CategoryDef[] = [
  { id: "app_caches", label: "App caches & logs", mode: "light", items: appCaches },
  { id: "trash", label: "Trash", mode: "light", items: trash },
  { id: "installers", label: "Old installers", mode: "light", items: installers },
  {
    id: "homebrew",
    label: "Homebrew",
    mode: "light",
    items: () => command("homebrew", "brew cleanup --prune=all -s", 1.4 * GB),
  },
  { id: "docker", label: "Docker", mode: "light", items: docker },
  { id: "pkg_caches", label: "Package manager caches", mode: "standard", items: pkgCaches },
  { id: "xcode_android", label: "Xcode & Android", mode: "standard", items: xcodeAndroid },
  { id: "ai_sessions", label: "AI session logs", mode: "deep", items: aiSessions },
  { id: "project_artifacts", label: "Project build artifacts", mode: "deep", items: projectArtifacts },
  { id: "worktrees", label: "Stale worktrees", mode: "deep", items: worktrees },
];

export function mockCategories(mode: ScanMode, recentDays: number): Category[] {
  return CATEGORIES.filter((c) => MODE_RANK[c.mode] <= MODE_RANK[mode]).map((c) => {
    const list = c.items(mode, recentDays).sort((a, b) => b.bytes - a.bytes);
    return {
      id: c.id,
      label: c.label,
      mode: c.mode,
      items: list,
      totalBytes: list.reduce((a, i) => a + i.bytes, 0),
    };
  });
}

/** Path, size, days since last opened, days since modified. */
export const AGED_FILES: [string, number, number, number][] = [
  [`${HOME}/Downloads/Xcode_15.4.xip`, 7.2 * GB, 410, 430],
  [`${HOME}/Movies/screen-recording-2025-03-11.mov`, 3.4 * GB, 230, 240],
  [`${HOME}/Models/llama-3-8b-instruct-q4.gguf`, 4.7 * GB, 150, 160],
  [`${HOME}/Downloads/ubuntu-24.04-live-server-arm64.iso`, 2.1 * GB, 320, 340],
  [`${HOME}/Documents/backup-2024.zip`, 1.6 * GB, 540, 560],
  [`${HOME}/Movies/demo-raw.mp4`, 1.2 * GB, 95, 115],
  [`${HOME}/Desktop/old-dump.sql`, 820 * MB, 275, 290],
  [`${HOME}/Downloads/dataset.parquet`, 640 * MB, 40, 400],
  [`${HOME}/Library/Application Support/Slack/Cache/f_0003`, 310 * MB, 120, 120],
  [`${HOME}/Downloads/design-export.pdf`, 95 * MB, 160, 180],
  [`${HOME}/Downloads/node-v20-darwin-arm64.tar.gz`, 48 * MB, 190, 210],
];

const H = 3600;
const HL = "Headless browser · test-runner.js · agent 3001";
export const MOCK_PROCS: ProcRow[] = [
  {
    pid: 77,
    ppid: 1,
    name: "com.apple.Virtualization.VirtualMachine",
    app: "OrbStack VM",
    rss: 2.1 * GB,
    cpuPct: 1.2,
    elapsedSec: 6 * 86400,
    reasons: ["vm", "protected"],
  },
  {
    pid: 412,
    ppid: 400,
    name: "Google Chrome Helper (Renderer)",
    app: "Google Chrome",
    rss: 3.1 * GB,
    cpuPct: 4.1,
    elapsedSec: 2 * 86400,
    reasons: ["leak"],
  },
  {
    pid: 400,
    ppid: 1,
    name: "Google Chrome",
    app: "Google Chrome",
    rss: 0.9 * GB,
    cpuPct: 2.0,
    elapsedSec: 2 * 86400,
    reasons: [],
  },
  {
    pid: 418,
    ppid: 400,
    name: "Google Chrome Helper (GPU)",
    app: "Google Chrome",
    rss: 0.6 * GB,
    cpuPct: 1.3,
    elapsedSec: 2 * 86400,
    reasons: [],
  },
  {
    pid: 981,
    ppid: 900,
    name: "Code Helper (Plugin)",
    app: "Visual Studio Code",
    rss: 2.4 * GB,
    cpuPct: 0.4,
    elapsedSec: 31 * H,
    reasons: ["idle"],
  },
  {
    pid: 900,
    ppid: 1,
    name: "Electron",
    app: "Visual Studio Code",
    rss: 0.7 * GB,
    cpuPct: 1.1,
    elapsedSec: 31 * H,
    reasons: [],
  },
  {
    pid: 3001,
    ppid: 981,
    name: "claude",
    app: "Claude Code",
    rss: 0.9 * GB,
    cpuPct: 12.3,
    elapsedSec: 3 * H,
    reasons: ["protected"],
    host: "vscode",
  },
  {
    pid: 3002,
    ppid: 3001,
    name: "node",
    app: "Claude Code",
    rss: 0.3 * GB,
    cpuPct: 0.0,
    elapsedSec: 2 * H,
    reasons: ["protected"],
  },
  {
    pid: 3003,
    ppid: 3001,
    name: "bun",
    app: "Claude Code",
    rss: 0.2 * GB,
    cpuPct: 0.1,
    elapsedSec: 2 * H,
    reasons: ["protected"],
  },
  {
    pid: 3100,
    ppid: 3002,
    name: "Google Chrome",
    app: HL,
    rss: 0.5 * GB,
    cpuPct: 48.0,
    elapsedSec: 40 * 60,
    reasons: ["protected", "headless"],
  },
  {
    pid: 3101,
    ppid: 3100,
    name: "Google Chrome Helper (Renderer)",
    app: HL,
    rss: 0.4 * GB,
    cpuPct: 31.0,
    elapsedSec: 40 * 60,
    reasons: ["protected", "headless"],
  },
  {
    pid: 3200,
    ppid: 1,
    name: "Google Chrome",
    app: "Headless browser · orphaned",
    rss: 0.6 * GB,
    cpuPct: 22.0,
    elapsedSec: 5 * H,
    reasons: ["orphan", "headless"],
  },
  {
    pid: 3201,
    ppid: 3200,
    name: "Google Chrome Helper (Renderer)",
    app: "Headless browser · orphaned",
    rss: 0.3 * GB,
    cpuPct: 9.0,
    elapsedSec: 5 * H,
    reasons: ["orphan", "headless"],
  },
  {
    pid: 5541,
    ppid: 1,
    name: "node",
    app: "node",
    rss: 0.8 * GB,
    cpuPct: 0.0,
    elapsedSec: 18 * H,
    reasons: ["orphan", "idle"],
  },
  {
    pid: 5602,
    ppid: 1,
    name: "esbuild",
    app: "esbuild",
    rss: 0.3 * GB,
    cpuPct: 0.0,
    elapsedSec: 26 * H,
    reasons: ["orphan", "idle"],
  },
  {
    pid: 5610,
    ppid: 1,
    name: "tsserver",
    app: "tsserver",
    rss: 1.1 * GB,
    cpuPct: 0.0,
    elapsedSec: 9 * H,
    reasons: ["orphan", "idle"],
  },
  {
    pid: 2210,
    ppid: 2200,
    name: "Slack Helper (Renderer)",
    app: "Slack",
    rss: 1.1 * GB,
    cpuPct: 0.3,
    elapsedSec: 4 * 86400,
    reasons: ["idle"],
  },
  {
    pid: 2200,
    ppid: 1,
    name: "Slack",
    app: "Slack",
    rss: 0.3 * GB,
    cpuPct: 0.2,
    elapsedSec: 4 * 86400,
    reasons: ["idle"],
  },
  {
    pid: 311,
    ppid: 1,
    name: "WindowServer",
    app: "WindowServer",
    rss: 0.9 * GB,
    cpuPct: 6.5,
    elapsedSec: 6 * 86400,
    reasons: ["protected"],
  },
  {
    pid: 220,
    ppid: 1,
    name: "mds_stores",
    app: "mds_stores",
    rss: 0.6 * GB,
    cpuPct: 0.9,
    elapsedSec: 6 * 86400,
    reasons: ["protected", "idle"],
  },
  {
    pid: 8812,
    ppid: 8800,
    name: "Figma Helper (Renderer)",
    app: "Figma",
    rss: 0.5 * GB,
    cpuPct: 0.1,
    elapsedSec: 7 * H,
    reasons: ["idle"],
  },
  { pid: 8800, ppid: 1, name: "Figma", app: "Figma", rss: 0.2 * GB, cpuPct: 0.1, elapsedSec: 7 * H, reasons: [] },
  { pid: 7000, ppid: 1, name: "Cursor", app: "Cursor", rss: 0.5 * GB, cpuPct: 0.4, elapsedSec: 2 * H, reasons: [] },
  {
    pid: 7001,
    ppid: 7000,
    name: "Cursor Helper (Plugin)",
    app: "Cursor",
    rss: 0.8 * GB,
    cpuPct: 0.2,
    elapsedSec: 2 * H,
    reasons: [],
  },
  {
    pid: 9999,
    ppid: 1,
    name: "jetsam",
    app: "Jetsam",
    rss: 0.1 * GB,
    cpuPct: 0.5,
    elapsedSec: 600,
    reasons: ["protected"],
  },
];
