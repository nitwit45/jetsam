# Changelog

All notable changes to Jetsam. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## Unreleased

## 0.1.1 - 2026-11-15

Undo stays available after the toast is gone, and three things 0.1.0 could touch are now left alone.

### Safety

- Clean, Aged Files: the data folders of Docker Desktop, OrbStack, Colima, Lima, Podman and Rancher Desktop are on the deny-list, so a disk image holding volumes is never listed.
- Memory: a process started from a terminal window is never an orphan. A job left running with `nohup` is no longer ended by Reclaim after its window closes.
- Startup: removed the code that moved `~/Library/Application Support/fresh` and deleted a `com.fresh.weekly` launch agent. It was for builds from before the rename and could have touched another app's data.

### Changed

- Clean: Undo is offered after every clean that sent something to the Trash, also when other items were left behind.
- Overview: **Undo last clean** stays for as long as the items are still in the Trash.
- CLI: `--mode nuke`, an undocumented alias of `full` from before the rename, is no longer accepted.

## 0.1.0 - 2026-11-13

First public release: four clean modes with a preview, Aged Files, and memory triage.

### Safety

- Clean: files go to the Trash by default. A permanent delete needs you to type `delete`.
- Clean: an item the Trash refuses stays on disk and is reported as refused. Nothing falls back to a permanent delete.
- Clean: every path is checked again right before removal. A folder with `.git` directly inside is never removed except a stale worktree, and generic build-folder names count only inside a git repo or next to a project file.
- Clean: deny-list for Keychains, Mail, Messages, Photos libraries, iCloud Drive, device backups, `.git` internals, `~/.ssh`, `~/.gnupg`, `~/.aws`, `~/.kube`, `~/.docker`, Claude Code memory and system folders, in every mode.
- Clean: build folders in repos active within the recent window are listed but cannot be selected below Full.
- Clean: worktrees are removed only when clean, merged, idle for 24 hours and with no process inside.
- Clean: Docker runs only against a local socket, and prunes images and build cache, never containers or volumes.
- Memory: Reclaim never ends a running Claude Code agent, anything it started, the terminal or editor hosting it, the app you were last using, macOS processes or a virtual machine.
- CLI: an unknown argument, a misspelled `--dry-run` included, is refused with exit status 2 and nothing runs.

### Added

- Clean: Light, Standard, Deep and Full modes, each with a preview.
- Clean: Undo in the toast after a clean, restored from where each item landed in the Trash.
- Clean: ops log at `~/Library/Application Support/Jetsam/ops.log`.
- Aged Files: big files not opened for 30 days to a year, by Spotlight's last-used date.
- Memory: pressure, swap and a per-process view labelled orphan, leak, idle, headless browser, VM or protected. Reclaim, soft restart of Chrome, VS Code and Cursor, and Restart now through the macOS restart dialog.
- Freshen: thin local snapshots, hibernation sleep image, VS Code excludes and TypeScript server memory, purge.
- Settings: weekly Light or Standard clean through launchd.
- CLI: `jetsam --headless --mode light|standard|deep|full [--dry-run]` (or `--mode=<mode>`), with Deep and Full limited to `--dry-run`.
