# Memory

How the **Memory** screen reads the system, labels processes and what **Reclaim** does. Written from `src-tauri/src/ram.rs` and `memory.rs`; the code is the reference.

## Sampling

While Jetsam is open, a background thread takes a sample every 10 seconds:

- the process table from `ps -axo pid=,ppid=,pgid=,rss=,%cpu=,etime=,comm=` and the command lines from `ps -axwwo pid=,args=`
- free memory from `memory_pressure`, falling back to `vm_stat` and `hw.memsize`
- swap from `sysctl vm.swapusage`

It keeps the last 60 samples per process and for the free-memory chart, which is 10 minutes at this rate. The pressure level comes from `kern.memorystatus_vm_pressure_level` (normal, warn, critical). If that cannot be read, below 10% free is critical and below 25% is warn.

**A restart would help** appears when uptime is over 5 days or swap use is over 70%.

## Labels

Each process gets zero or more labels. Only `orphan` is acted on without you choosing the process.

### protected

Never ended, by Reclaim or by hand. A process is protected when it is any of these:

- pid 0 or 1, or Jetsam itself, or any process above Jetsam in the process tree (the terminal or launcher it was started from)
- a Claude Code agent, any descendant of one, or any process that was attributed to an agent in the last 120 seconds
- any ancestor of a running agent: the terminal, shell or editor extension host it runs in
- an executable under `/System/`, `/usr/libexec/`, `/usr/sbin/`, `/usr/bin/`, `/sbin/`, `/bin/` or `/Library/Apple/`. That includes anything run from `/usr/bin`, such as the system `ruby`.
- a running launchd job whose label starts with `com.apple.` (from `launchctl list`)
- `WindowServer`, `loginwindow`, `kernel_task` or `launchd`
- the frontmost app's process, or any process whose executable lives inside its bundle (from `lsappinfo`). While Jetsam is in front, or `lsappinfo` fails, the rule uses the last other app a sample saw in front, so the app you were using before you clicked Reclaim stays protected. Every process inside its bundle is protected, and its main pid only while the same process still holds it. Samples run every 10 seconds, so an app you had in front for less than that may not be the one remembered.

A Claude Code agent is a process whose executable is named `claude`, or lives under `~/.local/share/claude/versions/`, or a `node` process running the `@anthropic-ai/claude-code` package's `cli.js` or `cli.mjs`, or a `bin/claude` link to it. The Claude desktop app is not an agent.

A background server an agent starts (`nohup node server.js &` from the agent's shell) stays protected for as long as that agent runs, even after the shell exits. A process up to 120 seconds old whose process group or session leader is attributed to an agent is claimed by that agent too.

Other coding agents get no special protection. Check what Reclaim lists before you run it.

### orphan

All of these must hold:

- its parent is now launchd (ppid 1)
- some earlier sample saw it with a real parent, so it was handed to launchd after it started. A process that was launchd's child from the first sample is never an orphan.
- it is not a launchd job (`launchctl list`), such as a LaunchAgent or a `brew services` daemon
- its name starts with one of: `node`, `tsserver`, `esbuild`, `vite`, `next-server`, `bun`, `python`, `ruby`, `java`, `code helper`, `cursor helper` (case is ignored)
- it is not protected

Because orphan status needs an earlier sample, a process that was already orphaned when Jetsam opened is not labelled until Jetsam has seen it with a parent. A reused pid starts from scratch.

### headless

A browser root process is headless when its executable is Google Chrome, Chromium or `chrome-headless-shell`, it is not a `--type=` child process, and its arguments include `--headless`, `--remote-debugging-pipe` or `--remote-debugging-port`. That is what Playwright and Puppeteer launch. Your own Chrome window has none of these flags and is not labelled.

The root and all its child processes form one group, named after the script that drives it (for example `Headless browser · crawl.js · agent 70`). The group is:

- protected while its driving process is still alive, or when it belongs to a Claude Code agent
- `orphan` when its driver is gone and it was handed to launchd after starting
- labelled `headless` only, and left alone, when Jetsam never saw it with a driver

### idle

At least 200 MB resident and under 0.5% CPU in every sample from the last 15 minutes, with at least six samples. With fewer samples, it needs to have been running for more than 15 minutes and be under 0.5% CPU now. Informational only.

### leak

Resident memory grew by at least 150 MB and by at least 1.3 times between the oldest and newest kept sample. Informational only.

### vm

An executable whose path contains `com.apple.Virtualization.VirtualMachine` (used by OrbStack, UTM and Docker Desktop) or `qemu`. It is grouped as "OrbStack VM" only while the OrbStack app is running, otherwise "Virtual machine". Jetsam refuses to end a VM, even if you tick it: stop it from its own app.

## Reclaim

**Reclaim now** does this, in order:

1. Takes a new sample and re-labels every process.
2. Ends every process labelled `orphan`.
3. Runs `purge` through the macOS administrator prompt to flush the file cache. Cancelling the prompt skips this step and the result says "Cancelled at the password prompt".
4. Waits 3 seconds, samples again and reports the change in free memory and swap.

Ticking processes in the table and choosing **End selected** adds them to step 2, after a confirmation that lists each one. The orphans and the cache flush run as well. A ticked process that is protected or a VM is refused with "Refused: protected process" or "Refused: virtual machine. Stop it from its own app." A ticked process that has exited is reported as "Not running".

Ending a process means SIGTERM to each one, a 3-second wait, then SIGKILL to any still alive. The result lists one line per process: "Ended cleanly", "Did not exit in 3s, force-ended", "Would not exit" or "SIGTERM failed". Unsaved work in an ended process is lost.

The freed figure is the rise in free memory as a share of installed RAM, plus the drop in swap use. It is a before-and-after reading of the whole system, not a sum of what each process held.

## Soft restart

Quits an app with an AppleScript `quit`, waits up to 20 seconds for it to exit, then reopens it with `open -a`.

- Chrome: refused unless Chrome is set to "Continue where you left off" (`session.restore_on_startup` is 1 in the Default profile).
- VS Code and Cursor: refused while a Claude Code agent is running inside the editor, and refused when `window.restoreWindows` is `"none"`.
- If the app has not quit after 20 seconds (usually unsaved changes), nothing is relaunched.

## Restart now

Asks `loginwindow` for the standard macOS restart dialog. You can cancel it there. Left alone, the dialog restarts the Mac when its countdown ends, which ends every process, running Claude Code agents included.
