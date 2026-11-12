# Building from source

## Requirements

- macOS 13 or later. Jetsam is tested on Apple silicon only. Release DMGs are universal (`aarch64` and `x86_64`).
- [bun](https://bun.sh) 1.3.13. CI pins this version because it wrote `bun.lock`; a different bun can rewrite the lockfile, and `bun install --frozen-lockfile` then fails on an unchanged repo. Bump the pin and regenerate `bun.lock` in the same PR.
- Rust stable from `rustup`, with `clippy` and `rustfmt`. CI uses `dtolnay/rust-toolchain@stable`; there is no `rust-toolchain.toml`.
- The Tauri 2 CLI comes from `@tauri-apps/cli` in `package.json`, run as `bun run tauri`.
- `git` on your `PATH`. Several tests create real repositories in a temporary folder.

## Run the app

```sh
bun install --frozen-lockfile
bun run tauri dev
```

`bun run tauri dev` starts Vite on port 5173 (`bun run dev`) and opens the app window against it.

## Sample data

The UI can run with no backend. Open `http://localhost:5173` in a browser while `bun run dev` is running, or build with `VITE_MOCK=1`, and every command is answered by `src/mock/` with made-up paths and sizes. Add `?autoscan` to the URL to start a scan on load. The README screenshots come from this mode. A production build contains none of the mock code.

## Checks

CI runs these on every push and pull request to `main` (`.github/workflows/ci.yml`), then builds the universal DMG so packaging breaks show up before a tag. Run the checks before you open a PR:

```sh
bun run tsc --noEmit
bun run lint
bun run build
cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

The repository has no `justfile`, `typos.toml`, `.nvmrc`, `clippy.toml`, `scripts/` folder or `src-tauri/tests/`. CI runs the commands above directly, the bun version is pinned in the workflows rather than in `.nvmrc`, the tag version check is inline in `release.yml`, and the Rust tests sit in a `tests` module beside the code they check.

`bun run lint` is ESLint plus `prettier --check`. `bun run format` rewrites files with Prettier. Rust formatting follows `src-tauri/rustfmt.toml` (110 columns).

One test moves a probe file through your real `~/.Trash` and is ignored by default. Run it by hand when you touch the Trash code:

```sh
cd src-tauri
cargo test -- --ignored
```

## Build a DMG

```sh
bun run tauri build
```

The DMG lands in `src-tauri/target/release/bundle/dmg/` and the app in `src-tauri/target/release/bundle/macos/`, for your Mac's architecture only. The universal build the release uses is:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
bun run tauri build --target universal-apple-darwin --bundles dmg
```

Its output is under `src-tauri/target/universal-apple-darwin/release/bundle/`. Builds are ad-hoc signed (`signingIdentity` `"-"` in `tauri.conf.json`) so Apple silicon runs them. Neither is signed with a Developer ID or notarized, so the steps in the README's "Opening an unsigned build" apply to your own build too once it has been downloaded.

## Layout

| Path | What is in it |
|---|---|
| `src/` | React UI. `api.ts` is the only module that calls the backend; `types.ts` is the shared contract. |
| `src/mock/` | Sample data for the browser and screenshot mode. |
| `src-tauri/src/rules.rs` | Categories, the deny-list, protected roots and the build-folder rules. |
| `src-tauri/src/categories.rs` | What each category lists for a scan. |
| `src-tauri/src/worktrees.rs` | Worktree checks, at scan time and at delete time. |
| `src-tauri/src/cleaner.rs` | The only module that removes anything, the ops log and Undo. |
| `src-tauri/src/aged.rs` | Aged Files. |
| `src-tauri/src/ram.rs`, `memory.rs` | Memory sampling, labels, Reclaim and soft restart. |
| `src-tauri/src/system.rs` | Freshen actions, the schedule, Full Disk Access, the administrator prompt. |
| `src-tauri/src/main.rs` | The binary entry point and the `--headless` flags. |
| `docs/img/` | README and docs screenshots, captured in sample-data mode at 2x. |
| `docs/icon.svg` | The app icon as a 1024 px SVG. `src-tauri/icons/` holds the sizes the bundle uses. |
