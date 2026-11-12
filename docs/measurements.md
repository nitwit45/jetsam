# Measurements

Every number Jetsam's README quotes, with the machine, the date and what produced it.

## The machine

MacBook Air, Apple M4, 10 cores, 16 GB RAM, 256 GB SSD, macOS 26.3.

## 3 Nov 2026: before Jetsam existed

I cleaned this Mac by hand on 3 Nov 2026, before Jetsam existed. None of these numbers came from it.

| Reading | Value |
|---|---|
| Data volume in use | 95.5% |
| Free on the Data volume | 11 GB |
| Regenerable build folders under the home folder | 62 GB |
| ...of which `node_modules` | 40.2 GB in 137 folders |
| Stale git worktrees in one repository | 105 worktrees, 29 GB |
| A copy of those worktrees inside one `.next` build folder | 4.3 GB |
| Swap used | 9.5 GB of 11 GB |
| Swap-outs since boot | 116 million |
| Uptime at the time | 6 days |

Each row was measured on its own, and the folders they cover can overlap, so the rows do not add up to a total.

After the manual cleanup:

| Reading | Value |
|---|---|
| Free on the Data volume | 33 GB |
| Container in use | 85.7% |

## Later: the memory incident

Nine headless Chrome instances, started by QA scripts under two coding agents, held about 5.5 GB resident between them and drove the load average to 56 on the 10-core machine.

## Jetsam's own run

| Command | Build | Result |
|---|---|---|
| `fresh --headless --mode deep --dry-run` | 3 Nov 2026, before the rename to Jetsam | Full scan of the home folder in 22 seconds |

One run on the same Mac after the cleanup above. The filesystem cache state was not recorded, and there is no spread because there was one run.

## Not measured yet

- What a clean costs the next day: how long the next install or build takes and how much comes back within a week.
- DMG size, installed size and idle memory use of the app.
- Scan time as a mean over several runs with a stated cache state.

These go here, with the same detail, once they are measured. Until then the README does not quote them.
