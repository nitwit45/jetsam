# Security

## Supported versions

Only the latest release gets fixes.

## Reporting a vulnerability

Use GitHub's private vulnerability reporting on this repository (**Security > Report a vulnerability**). Include the Jetsam version, macOS version and the steps to reproduce. Please do not open a public issue until a fix is released.

One person maintains Jetsam. You will get a reply within 7 days.

A bug that makes Jetsam delete or end something it should not is treated as a security issue, whether or not anyone could exploit it. That includes anything that gets past the deny-list, removes a git-tracked file, removes a worktree with unmerged work, prunes a Docker volume or container, or ends a running Claude Code agent or a protected process. For those, also attach the matching lines from `~/Library/Application Support/Jetsam/ops.log`.

## What Jetsam can do on your Mac

Jetsam runs as your user and never as root. These are the privileges it uses:

- Deleting files you can delete. In the app every item is listed before it can be removed; the weekly job and `--headless` remove what a scan would preselect without asking. Files go to the Trash unless you choose a permanent delete and type `delete`. `brew cleanup`, the Docker prunes, `git worktree prune` and Freshen's snapshot thinning are permanent. The rules are in [docs/modes.md](docs/modes.md) and the checks in [docs/safety.md](docs/safety.md).
- Ending processes you own, with `kill`: SIGTERM, then SIGKILL after 3 seconds. Protected processes are refused. See [docs/memory.md](docs/memory.md).
- Full Disk Access, if you grant it in System Settings. Jetsam never asks for it through a prompt of its own; it shows a banner and a button that opens the settings pane.
- The administrator prompt, for three commands, each time you use them: `purge` (Reclaim's cache flush and Freshen's purge) and `pmset -a hibernatemode 0` or `pmset -a hibernatemode 3` (Freshen's hibernation switch). Cancelling skips that command.
- A launchd job in your user domain, only when you turn on the weekly clean: `~/Library/LaunchAgents/com.nith.jetsam.weekly.plist`, running Jetsam's own binary with `--headless` in Light or Standard.
- Writing VS Code's `settings.json`, only when you apply Freshen's excludes, and only if the file parses as plain JSON. It adds watcher and search excludes, sets `typescript.tsserver.maxTsServerMemory` to 2048 over any existing value, and writes the file back pretty-printed. See [docs/freshen.md](docs/freshen.md).

## Network

Jetsam makes no network requests. It sends no telemetry or crash reports and does not check for updates. Its Rust and TypeScript code contains no HTTP client and no URL fetch, and the window's Content Security Policy limits connections to the app itself (`connect-src 'self' ipc: http://ipc.localhost`). Tauri depends on an HTTP library; Jetsam does not call it.

## Commands Jetsam runs

Every external program the backend starts, and what for:

| Program | Used for |
|---|---|
| `git` | Repository activity, tracked files, worktree list, status and merge checks, `git worktree prune` after removing a worktree |
| `lsof` | Working directories of running processes, before listing or removing a worktree |
| `brew` | `brew cleanup -n` to size, `brew cleanup --prune=all -s` to clean |
| `docker` | `context show` and `context inspect` to check the engine is local, `system df` to size, `image prune` and `builder prune` to clean |
| `ps`, `launchctl list`, `lsappinfo`, `pgrep` | The process table, launchd jobs, the frontmost app, whether Chrome, an editor or OrbStack is running |
| `kill` | Ending processes |
| `sysctl`, `vm_stat`, `memory_pressure` | Memory, swap, pressure level, uptime |
| `df` | Disk size and free space |
| `mdls` | Spotlight's last-used date for Aged Files |
| `mdutil -s` | Whether Spotlight indexes the Data volume |
| `pmset -g` | Current hibernation mode |
| `tmutil thinlocalsnapshots` | Freshen's snapshot thinning |
| `osascript` | The administrator prompt, asking Chrome, VS Code or Cursor to quit for a soft restart, and the macOS restart dialog |
| `open` | Reopening an app after a soft restart, Reveal in Finder, opening System Settings |
| `launchctl bootstrap`, `launchctl bootout` | Installing and removing the weekly job |
| `which` | Checking whether `brew` and `docker` are installed |

Queries time out after 8 seconds. The clean commands (`brew cleanup`, `docker ... prune`), `tmutil`, `df`, `open` (Reveal in Finder and opening System Settings), the administrator prompt and the restart dialog run without one.
