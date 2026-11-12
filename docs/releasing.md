# Releasing

How a version goes out. The workflow is `.github/workflows/release.yml`; if this page and the workflow disagree, the workflow is right.

## Steps

1. Set the same version in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and `package.json`. Run `cargo check` in `src-tauri/` so `Cargo.lock` picks it up.
2. In `CHANGELOG.md`, rename `## Unreleased` to `## 0.1.0 - <date>` (or `## [0.1.0] - <date>`), add one plain sentence under it saying what changed for the user, and put a new empty `## Unreleased` on top. The workflow fails if it cannot find that version's section.
3. Commit, then tag and push:

   ```sh
   git tag v0.1.0
   git push origin v0.1.0
   ```

4. The workflow runs on `v*.*.*` tags only:
   - `version` fails unless the tag, without its `v`, equals the version in all three files.
   - `check` runs the CI workflow.
   - `build` runs only when both pass. It builds a universal DMG with no build caches, copies it to `Jetsam.dmg`, writes `Jetsam.dmg.sha256`, records a build provenance attestation, and creates a draft release titled `Jetsam <version>`. The notes are the CHANGELOG section followed by an install block with the SHA-256 table and the `shasum` and `gh attestation verify` commands.
5. Open the draft and read the notes.
6. Download `Jetsam.dmg` from the draft and check it:

   ```sh
   shasum -a 256 -c Jetsam.dmg.sha256
   gh attestation verify Jetsam.dmg --repo nitwit45/jetsam
   ```

7. Install it on a second macOS user account by following the README's "Opening an unsigned build" steps exactly. Open Jetsam, run a Light scan and check the preview.
8. Publish the draft. Nothing is published without this manual step.

`Jetsam.dmg` keeps the same name in every release, so `releases/latest/download/Jetsam.dmg` in the README always points at the newest one.

## Changelog sections

Safety (any change to what can be removed or ended), Added, Changed, Fixed. Each line starts with the area and ends with the PR number, and says what changed for the user in the past tense or as a plain fact. No emoji, and no auto-generated list of every merged PR.
