# Contributing

Jetsam deletes files and ends processes, so the bar for a change is a test that proves it removes only what it should.

## Before you start

- Open an issue before a feature PR. A new cleaning rule needs agreement on where the files live, which tool regenerates them, and how Jetsam tells them apart from someone's data.
- Bug fixes and docs fixes can go straight to a PR.
- You must understand and have run what you submit, on a real Mac. A PR you cannot explain line by line will be closed.

## Setup

See [docs/building.md](docs/building.md). In short: bun 1.3.13 and Rust stable, then `bun install --frozen-lockfile` and `bun run tauri dev`.

## Checks

CI runs these, and a PR that fails them is not reviewed:

```sh
bun run tsc --noEmit
bun run lint
bun run build
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

## The safety rule

Any change that adds a delete path, a category, a matching rule, or changes which processes can be ended must:

1. Go through `rules.rs` for paths. The deny-list and protected-roots checks are the only gate, and every candidate passes them. Do not add a second path to the cleaner.
2. Come with a test that proves a deny-listed path is rejected by the new rule, and a test that proves the intended path is accepted.
3. Never match a directory on its name alone when the name is one a person might use for their own folder. `build` and `dist` count only inside a git repo or next to a project file (`MANIFEST_FILES` in `rules.rs`); `target` needs a `Cargo.toml` beside it. Put the check in `is_artifact_dir`, not in the scanner.
4. Never remove a directory with a `.git` file or folder directly inside it. The one exception is a stale worktree under `.claude/worktrees/`, which `worktrees::verify_removable` checks instead. The check runs at scan time and again in `cleaner::verify`; a new rule must not route around either.
5. For memory: never widen what Reclaim ends without asking. Only `orphan` is ended without a tick, and a new label that ends processes needs tests in `ram.rs` for the cases that must stay protected.
6. Add a row to [docs/safety.md](docs/safety.md) with the claim, the code and the test.

If you are unsure whether something is regenerable, it is not.

## Pull requests

- One change per PR. Keep refactors out of feature PRs.
- Fill in the template: what the user can now do, how you tested it, and the safety review when it applies.
- UI changes include a screenshot in dark and light, taken in sample-data mode so no real paths show.
- No new dependency without a sentence on why the standard library or an existing crate does not cover it.

## Changelog

Add a line under `## Unreleased` in [CHANGELOG.md](CHANGELOG.md); CI fails a PR that does not touch it (Dependabot PRs are exempt). Put it in one of the sections Safety, Added, Changed or Fixed. Safety is for any change to what can be removed or ended. Prefix the line with the area and end it with the PR number:

```markdown
- Clean: stale worktrees with an ignored `.env` are now skipped (#12).
```

## Naming

The app is **Jetsam**: in the UI, the window title, the Dock, docs and release titles. Use lowercase `jetsam` only for code, the repository name, the bundle identifier, the binary and CLI output.
