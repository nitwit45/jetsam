# FAQ

- [I cleaned 20 GB but Finder shows no new free space](#i-cleaned-20-gb-but-finder-shows-no-new-free-space)
- [node_modules came back](#node_modules-came-back)
- [Why does it need Full Disk Access?](#why-does-it-need-full-disk-access)
- [Does Jetsam send anything over the network?](#does-jetsam-send-anything-over-the-network)
- [Why did it not end this Claude process?](#why-did-it-not-end-this-claude-process)
- [What counts as a stale worktree?](#what-counts-as-a-stale-worktree)
- [What does Full mode do to my current projects?](#what-does-full-mode-do-to-my-current-projects)
- [Why can I not tick this build folder?](#why-can-i-not-tick-this-build-folder)
- [Why does the Trash category end in "not cleaned"?](#why-does-the-trash-category-end-in-not-cleaned)
- [What does Undo cover?](#what-does-undo-cover)
- [Does it protect other coding agents?](#does-it-protect-other-coding-agents)

## I cleaned 20 GB but Finder shows no new free space

Files moved to the Trash still use disk space until the Trash is emptied. The toast after a clean says so: "Files are in the Trash until you empty it." Empty the Trash in Finder, or run a Light scan and delete the Trash category permanently.

If the Trash is empty and the space still has not come back, APFS may be holding it in local Time Machine snapshots or as purgeable space. **Freshen > Thin local Time Machine snapshots** asks macOS to thin them.

## node_modules came back

That is expected. Everything Jetsam removes in Clean is something a tool can regenerate, and the tool regenerates it the next time you install or build that project. Deep leaves alone build folders in git repositories active in the last 14 days, so the projects you are working on keep theirs. A project that is not a git repository has no activity date, and Deep takes its build folders whenever it runs.

## Why does it need Full Disk Access?

macOS blocks apps from reading some folders under `~/Library`, such as `~/Library/Mail` and `~/Library/Safari`, without it. Jetsam works without it, but the scan and Aged Files cannot see into those folders, so their sizes are missing. Jetsam checks by trying to read those two folders, shows a banner while it cannot, and checks again each time its window comes to the front.

Full Disk Access does not widen what Jetsam deletes: Mail, Messages and the rest of the deny-list are refused either way.

## Does Jetsam send anything over the network?

No. Its code has no HTTP client, and the window's content policy only allows connections to the app itself. Tauri, the framework it is built on, pulls in an HTTP library as a dependency; Jetsam does not call it. The commands Jetsam runs are listed in [SECURITY.md](../SECURITY.md).

## Why did it not end this Claude process?

Memory protects every running Claude Code agent, everything it started, and the terminal or editor it runs in. A process an agent started stays protected while that agent runs, even after the shell that launched it has exited, and for 120 seconds after the agent itself exits. The rules are in [memory.md](memory.md#protected).

If the agent is gone and you want the process ended, wait two minutes and refresh: it is then labelled like any other process.

## What counts as a stale worktree?

A worktree under `.claude/worktrees/` in a repository Deep finds, where all of these hold:

- no uncommitted files, and no ignored files other than build folders and `.DS_Store`
- no commits that are not on `main` (or `master`)
- no git write to its `HEAD`, index or reflog in the last 24 hours
- no running process has its working directory inside it

It goes to the Trash, and Jetsam then runs `git worktree prune` in the repository, which is permanent: Undo brings the folder back, but not as a registered worktree. Details in [modes.md](modes.md#worktrees).

## What does Full mode do to my current projects?

Full removes build folders (`node_modules`, `.next`, `target` and the rest) from projects you are working on now, so the next build there reinstalls and rebuilds them. It does not touch anything git tracks, your source files, or anything on the deny-list, and the worktree rules are the same as in Deep. If you have a dev server running from a `node_modules` folder that Full removes, expect it to break until you reinstall.

## Why can I not tick this build folder?

The badge reads "recent", "tracked" or "size only", and hovering the row shows the flag's own name (`recent`, `git-tracked`, `report-only`). `recent` means the project was active within the recent window (Deep only; Full clears it). `git-tracked` ("tracked") means git tracks something inside it, or git could not answer. `report-only` ("size only") items are listed for their size and are never removed. See [modes.md](modes.md#how-an-item-is-marked).

## Why does the Trash category end in "not cleaned"?

Moving something that is already in the Trash to the Trash is refused, so those items come back as "The Trash refused this item". Choose **Delete these permanently** and type `delete` to empty them. A folder in the Trash with a `.git` directly inside, such as a deleted repository, is refused with "contains a git repository" and gets no permanent delete; empty it from Finder. In a headless Light run, which never deletes files permanently, Trash items count as failed.

## What does Undo cover?

The most recent batch in the ops log, and only items that went to the Trash in your home folder's `~/.Trash`. That batch can be the weekly job's: if it ran after your last clean in the app, Undo restores the weekly job's items, and your own clean can no longer be undone. Each item is put back from the exact place it landed, recorded in the ops log. It is skipped if it is no longer in the Trash, if something now sits at its original path, or if the folder it came from is gone. Permanent deletes, `brew` or `docker` commands and `git worktree prune` cannot be undone.

## Does it protect other coding agents?

No. Only Claude Code gets agent protection. Other agents and the processes they start are labelled like any other process, so read the Reclaim list before you run it.
