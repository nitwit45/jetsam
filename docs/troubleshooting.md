# Troubleshooting

Messages Jetsam shows, word for word, what causes each and what to do.

## Opening the app

**"Jetsam" can't be opened because Apple cannot check it for malicious software**, or on macOS 15 and later **Apple could not verify "Jetsam" is free of malware**. The build is not signed or notarized. Follow [Opening an unsigned build](../README.md#opening-an-unsigned-build).

**"Jetsam" is damaged and can't be opened.** The quarantine flag macOS sets on downloads. Run:

```sh
xattr -dr com.apple.quarantine /Applications/Jetsam.app
```

## Clean

When a clean leaves items behind, the toast offers **Details**. Each line there gives one of these reasons:

| Reason | Cause | What to do |
|---|---|---|
| The Trash refused this item | macOS would not move it to the Trash. Items already in the Trash always get this. | **Delete these permanently**, then type `delete`, or leave it. |
| `modified since scan` | The item's modified time changed after the scan. | Rescan. Something is still writing to it. |
| `size changed since scan` | A file's size changed after the scan. | Rescan. |
| `no longer exists` | It was removed after the scan. | Nothing. |
| `is a symlink` | The path is now a symbolic link. Jetsam never follows or removes links. | Nothing. |
| `not a directory any more`, `not a file any more` | It changed type after the scan. | Rescan. |
| `contains a git repository` | A `.git` file or folder appeared inside it. | Nothing. It is a repository now. |
| `deny-listed`, `protected root` | The path is on the deny-list or is a protected folder. | Nothing. These are never removed. |
| `a running process is working in it` | A worktree that some process has as its working directory, or Jetsam could not read the process list. | Stop that process, or leave the worktree. |
| `has uncommitted or ignored files` | A worktree gained changes, or git failed on it. | Commit or remove the changes. |
| `has commits not merged into main` | A worktree has commits not on `main` or `master`. | Merge the branch or leave it. |
| `docker ... failed: ...`, `brew ... failed: ...` | The command ran and exited with an error; the last lines of its error output follow. | Run the same command in Terminal to see why. |

Other failures (often a file another app has open, or one that needs Full Disk Access) show the underlying error. Quit the app holding the file and clean again.

**unknown scan ...; rescan first.** Jetsam keeps only the latest scan in memory. Rescan with **Cmd+R** and clean again.

## Aged Files

**Could not look for aged files.** The home-folder walk failed; the message under it is the cause. **Retry** runs it again.

## Memory

Each Reclaim action is listed in the result:

| Line | Meaning |
|---|---|
| Ended cleanly | It exited within 3 seconds of SIGTERM. |
| Did not exit in 3s, force-ended | It needed SIGKILL. |
| Would not exit | SIGKILL could not be sent either. |
| SIGTERM failed | The signal could not be sent. |
| Refused: protected process | You ticked a process that became protected before Reclaim ran. |
| Refused: virtual machine. Stop it from its own app. | Jetsam never ends a VM. Quit OrbStack, UTM or Docker Desktop instead. |
| Not running | It exited before Reclaim got to it. |
| Cancelled at the password prompt | The file-cache flush was skipped. Everything else still ran. |

Soft restart messages:

- **Chrome is not set to restore tabs. Turn on "Continue where you left off" at chrome://settings/onStartup first; nothing was quit.** Change that setting in Chrome and try again.
- **1 Claude Code agent is running inside Visual Studio Code. Restarting would stop it, so finish first.** (or Cursor) The card shows this and its Restart button is disabled. Let the agent finish, then restart.
- **1 Claude Code agent is running in Visual Studio Code. Restarting Visual Studio Code would stop it. Let it finish first.** The same check made again when you press Restart, for an agent the card's count missed. Let the agent finish, then restart.
- **Visual Studio Code has window.restoreWindows set to "none", so windows would not come back. Change it first; nothing was quit.** Change `window.restoreWindows` in the editor's settings.
- **... did not quit within 20s (unsaved changes?). Nothing was relaunched.** The app is waiting on a save dialog. Answer it and try again.
- **... quit but `open -a` failed; launch it by hand.** Open the app from Finder.

**Could not open the restart dialog.** The message under it is the error from macOS. Use the Apple menu > Restart instead.

## Freshen

**settings.json is not strict JSON (comments or trailing commas); add the excludes by hand.** VS Code accepts comments in `settings.json`, but Jetsam only rewrites a file it can parse as plain JSON, so it leaves yours alone. Add `files.watcherExclude` and `search.exclude` entries for `**/node_modules/**`, `**/.next/**`, `**/dist/**` and `**/.git/objects/**` yourself.

## Schedule

**scheduled runs are light or standard only, not deep** (or `full`). Pick Light or Standard.

**launchctl bootstrap failed.** launchd would not load the job. Turn **Weekly clean** off and on again. The job's output goes to `~/Library/Logs/Jetsam/weekly.log`.

## Command line

The binary prints `jetsam: ` and one of these, then exits with status 2:

- `unknown mode <name> (expected light, standard, deep or full)`
- `deep mode only runs headless with --dry-run; headless cleaning is light or standard` (or `full`)
- `--mode needs a value; see jetsam --help`
- `--mode given more than once`
- `--mode and --dry-run only apply with --headless; see jetsam --help`
- `unknown argument <arg>; see jetsam --help`, for anything else, a misspelled `--dry-run` included. Nothing runs.

A headless Light run that reports failed items usually means items in the Trash: the headless run never deletes files permanently, so it cannot empty the Trash.
