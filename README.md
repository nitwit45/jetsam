# Jetsam

Jetsam is a macOS disk and memory cleaner for developers. It removes files your tools can regenerate (node_modules, build output, package caches, stale git worktrees) in four modes from Light to Full. In the app, every path is listed with its size before it can be removed, and files go to the Trash by default. It also lists big files you have not opened in months, and finds memory held by orphaned helpers, leaking processes and headless browsers that coding agents left behind. It never ends a running Claude Code agent or the terminal or editor it runs in.

macOS 13 or later, tested on Apple silicon only. Version 0.1, not signed or notarized: see [Opening an unsigned build](#opening-an-unsigned-build).

<a href="https://github.com/nitwit45/jetsam/releases/latest">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/img/clean-dark.png">
    <img src="docs/img/clean-light.png" width="800" alt="Jetsam's Clean screen in Standard mode with sample data: 22 items and 36.3 GB selected, one /private/var/folders cache marked size only and unticked.">
  </picture>
</a>

Screenshots show the app's sample data, not a real machine.

## Install

Download [Jetsam.dmg](https://github.com/nitwit45/jetsam/releases/latest/download/Jetsam.dmg), open it and drag Jetsam to Applications. To check it, get `Jetsam.dmg.sha256` from the [release page](https://github.com/nitwit45/jetsam/releases/latest) and run:

```sh
shasum -a 256 -c Jetsam.dmg.sha256
gh attestation verify Jetsam.dmg --repo nitwit45/jetsam
```

### Opening an unsigned build

Jetsam is not signed with an Apple Developer ID, so macOS blocks the first launch.

1. Open Jetsam once and close the warning.
2. Open **System Settings > Privacy & Security**, scroll to Security and click **Open Anyway**.

Or in Terminal:

```sh
xattr -dr com.apple.quarantine /Applications/Jetsam.app
```

On macOS 15 and later, Control-click > Open no longer gets past this. If macOS says the app "is damaged and can't be opened", the quarantine flag is the cause and the command above clears it.

Then grant Full Disk Access, see [Permissions and network](#permissions-and-network).

## What it does

### Clean

Pick a mode on **Overview**, press **Scan**, and review the result under **Clean**. Each mode includes the ones above it.

| Mode | Adds |
|---|---|
| Light | `~/Library/Caches` (except what Standard lists) and `~/Library/Logs`, the Trash, `.dmg` and `.pkg` files in Downloads older than 14 days, Homebrew cleanup, Docker dangling images and build cache. |
| Standard | Package manager caches (npm, pnpm, bun, pip, cargo, Gradle, Playwright), Xcode and Android leftovers. |
| Deep | Build folders in inactive projects, stale merged worktrees, all unused Docker images, and AI session logs older than 45 days for you to review. |
| Full | Build folders in projects you are working on now. Nothing git-tracked is touched. |

A project is active when its newest commit or git index write is under 14 days old (**Settings > Recent window**). Deep lists its build folders but they cannot be ticked. A project outside git has no activity date, so Deep preselects its build folders. Every path and rule: [docs/modes.md](docs/modes.md).

### Aged files

**Aged Files** lists files in your home folder of 250 MB or more that you have not opened in 90 days (adjustable), by Spotlight's last-used date. Nothing is preselected, and a row's Trash button moves that file at once.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/img/aged-dark.png">
  <img src="docs/img/aged-light.png" width="800" alt="Jetsam's Aged Files screen listing eight sample files over 250 MB, 21.3 GB in total, largest first.">
</picture>

### Memory

**Memory** samples every process every 10 seconds and labels it orphan, leak, idle, headless browser, virtual machine or protected. **Reclaim now** ends the orphans, then flushes the file cache. Other unprotected processes you tick are ended after a confirmation: SIGTERM, then SIGKILL if still running 3 seconds later. Soft restart quits and reopens Chrome, VS Code or Cursor, and refuses while a Claude Code agent runs in the editor. Thresholds are in [docs/memory.md](docs/memory.md).

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/img/memory-dark.png">
  <img src="docs/img/memory-light.png" width="800" alt="Jetsam's Memory screen with sample data: Reclaim offering to end five orphan helpers holding 3.1 GB, and the VS Code restart disabled while a Claude Code agent runs in it.">
</picture>

### Freshen

Four one-off changes: thin local Time Machine snapshots (permanent), switch the hibernation sleep image, add VS Code watcher excludes, and purge inactive memory. See [docs/freshen.md](docs/freshen.md).

### Schedule and command line

**Settings > Weekly clean** runs a Light or Standard clean every Sunday at 10:00 with no preview. The same run by hand:

```sh
/Applications/Jetsam.app/Contents/MacOS/jetsam --headless --mode standard
/Applications/Jetsam.app/Contents/MacOS/jetsam --headless --mode deep --dry-run
```

`--dry-run` prints totals per category and removes nothing. Deep and Full run headless only with `--dry-run`. An argument Jetsam does not know, a misspelled `--dry-run` included, stops it with exit status 2 before anything runs.

## Safety

Jetsam deletes files and ends processes. The rules below are what limits it.

- In the app, every item is listed with its path and size before it can be removed, and unticking a row keeps it. The weekly job and `--headless` remove what a scan would preselect without asking, and skip anything marked for review.
- Files go to the Trash, and **Undo** restores what the most recent clean sent to your home Trash. Permanent: the **Permanent** switch, which needs you to type `delete`; `brew cleanup` and the Docker prunes; `git worktree prune` after a worktree is removed; and Freshen's snapshot thinning.
- If the Trash refuses an item, it stays put. A permanent delete is offered behind the same typed confirmation, never run on its own.
- Each path is checked again right before removal and skipped if it changed since the scan.
- Git-tracked files are skipped, even inside a build folder, and a git failure counts as tracked. A folder with `.git` directly inside it is never removed, except a stale worktree.
- `build`, `dist` and similar names count only inside a git repo or next to a project file such as `package.json`. `target` needs a `Cargo.toml` beside it.
- A worktree goes only when it is under `.claude/worktrees/`, has no uncommitted or ignored files other than build output, has no commits outside `main` or `master`, has seen no git activity for 24 hours, and no process works inside it.
- Never touched: Keychains, Mail, Messages, Photos libraries, iCloud Drive, iPhone backups, `~/.ssh`, `~/.gnupg`, `~/.aws`, `~/.kube`, `~/.docker`, Claude Code memory and the system folders. Docker volumes and containers are never pruned.
- Memory triage never ends a running Claude Code agent, anything it started, the terminal or editor hosting it, the app you were last using and its helpers, Jetsam itself, or [macOS processes](docs/memory.md#protected). Virtual machines are only labelled.
- Every clean is logged to `~/Library/Application Support/Jetsam/ops.log`, one JSON line per item.

Each claim, the code behind it and the test that checks it: [docs/safety.md](docs/safety.md).

## Measured on one machine

I built Jetsam after my MacBook Air (M4, 16 GB RAM, 256 GB SSD, macOS 26.3) ran out of room on 3 Nov 2026. I cleaned it by hand that day, before Jetsam existed, so these numbers are not Jetsam's.

| Before the cleanup | |
|---|---|
| Data volume | 95.5% full, 11 GB free |
| Regenerable build folders under home | 62 GB, 40.2 GB of it in 137 `node_modules` folders |
| Stale git worktrees in one repository | 105 worktrees, 29 GB, plus a 4.3 GB copy inside a `.next` folder |
| Swap | 9.5 GB of 11 GB used, 116 million swap-outs in 6 days of uptime |

Afterwards the volume had 33 GB free. Later, nine headless Chrome instances started by QA scripts under two coding agents held about 5.5 GB resident. One Deep dry run on the same Mac took 22 seconds.

I have not measured the cost side: whatever is removed downloads or rebuilds the next time a project needs it. This is one machine on one day. Method and caveats are in [docs/measurements.md](docs/measurements.md).

## Permissions and network

Without Full Disk Access, macOS hides folders such as `~/Library/Mail` from the scan and a banner says so. Grant it in **System Settings > Privacy & Security > Full Disk Access**; the banner's button opens that pane.

Jetsam runs as your user. The cache flush, Freshen's purge and the hibernation switch ask for your administrator password each time. **Restart now** opens the macOS restart dialog, which ends every process, running Claude Code agents included, unless you cancel it.

Jetsam makes no network requests. Its code has no HTTP client, and the window's content policy allows connections only to the app itself. Every command it runs is listed in [SECURITY.md](SECURITY.md).

## When not to use Jetsam

- You want a command line tool to script. Use [Mole](https://github.com/tw93/Mole) or [npkill](https://github.com/voidcosmos/npkill).
- You only want to see what fills the disk. GrandPerspective or DaisyDisk do that.
- You need to uninstall apps. Use AppCleaner.
- Your Mac is managed and blocks unsigned apps.
- You are on slow or metered internet. Everything Jetsam removes downloads again the next time you build.

## Uninstall

Turn off **Settings > Weekly clean** first, then quit Jetsam and run:

```sh
rm -rf /Applications/Jetsam.app
rm -rf ~/Library/Application\ Support/Jetsam ~/Library/Logs/Jetsam
rm -rf ~/Library/WebKit/dev.nith.jetsam ~/Library/Caches/dev.nith.jetsam
rm -f ~/Library/LaunchAgents/com.nith.jetsam.weekly.plist
```

Remove Jetsam from **System Settings > Privacy & Security > Full Disk Access**.

## Building from source

You need [bun](https://bun.sh) 1.3.13 and a stable Rust toolchain from `rustup`.

```sh
bun install --frozen-lockfile
bun run tauri dev
```

Checks, tests, the DMG build and the sample-data mode are in [docs/building.md](docs/building.md). Jetsam is built on [Tauri](https://tauri.app), [jwalk](https://crates.io/crates/jwalk) for parallel directory walks, [trash](https://crates.io/crates/trash) for the Trash, and [serde](https://serde.rs). To contribute, read [CONTRIBUTING.md](CONTRIBUTING.md) first.

## The name

Jetsam is the memory-pressure mechanism in Apple's XNU kernel. When the system runs short of memory, it terminates processes to get memory back.

## License

MIT, see [LICENSE](LICENSE).
