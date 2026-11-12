## Summary

One or two sentences on what the user can now do, or what is fixed. Link the issue.

## How I tested it

Commands run, the Mac and macOS version it ran on, and what you saw. UI changes need a screenshot in dark and light, taken in sample-data mode.

- [ ] `bun run tsc --noEmit`, `bun run lint` and `bun run build` pass.
- [ ] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test` pass in `src-tauri/`.

## Safety review

Required for any change to what can be removed, what a scan lists, which commands run, or which processes can be ended. Write "Not applicable" otherwise.

- What can now be removed or ended that could not before?
- What can no longer be removed or ended?
- Which test proves the intended case is accepted, and which proves a deny-listed or protected case is still refused?
- [ ] New delete paths go through `rules.rs` and `cleaner::verify`, with no second path to the cleaner.
- [ ] Generic folder names need a repo or a project file beside them, and a folder with `.git` directly inside is still never removed (stale worktrees excepted).
- [ ] [docs/safety.md](https://github.com/nitwit45/jetsam/blob/main/docs/safety.md) has a row for each new claim.

## Changelog

The line you added under `## Unreleased` in `CHANGELOG.md`:

```markdown
- Area: what changed for the user (#PR).
```
