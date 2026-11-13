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
8. Publish the draft. Nothing is published without this manual step. Releases in this repository are immutable: after publishing, the assets and the tag cannot be changed, and a deleted tag cannot be used again. A mistake means a new version.
9. Check the published release from a clean folder:

   ```sh
   gh release download --repo nitwit45/jetsam --pattern 'Jetsam.dmg*'
   shasum -a 256 -c Jetsam.dmg.sha256
   gh release verify-asset Jetsam.dmg --repo nitwit45/jetsam
   ```

`Jetsam.dmg` keeps the same name in every release, so `releases/latest/download/Jetsam.dmg` in the README always points at the newest one.

## Releasing without the workflow

0.1.0 was released this way, from one Mac. The result has a checksum and GitHub's release attestation, but no build provenance attestation, and the notes have to say so.

1. Do steps 1 to 3 with the Release workflow disabled (`gh workflow disable release.yml`), so the tag does not start it.
2. Run the checks in [building.md](building.md#checks). Then build with your home folder remapped, or the binary carries your account name in the source paths of its panic messages:

   ```sh
   RUSTFLAGS="--remap-path-prefix=$HOME=/home --remap-path-prefix=$PWD=/jetsam --remap-path-prefix=$HOME/.rustup=/rustup --remap-path-prefix=$HOME/.cargo=/cargo" \
     bun run tauri build --target universal-apple-darwin --bundles dmg
   ```

   When two prefixes match, the later one wins, so the general one comes first.
3. Copy the DMG to `Jetsam.dmg` in an empty folder outside the repository. Write the checksum and confirm that no local path is left in the binary:

   ```sh
   shasum -a 256 Jetsam.dmg > Jetsam.dmg.sha256
   hdiutil attach -nobrowse -readonly Jetsam.dmg
   strings -a /Volumes/Jetsam/Jetsam.app/Contents/MacOS/jetsam | grep -c /Users/
   hdiutil detach /Volumes/Jetsam
   ```

   The count must be 0. Run the app from the mounted image once, and run `jetsam --headless --dry-run` natively and under `arch -x86_64`.
4. Write `notes.md` the way the workflow does: the CHANGELOG section, then the install block with the SHA-256 table. Leave out `gh attestation verify` and say the build was local.
5. Create the draft, then follow steps 5 to 9:

   ```sh
   gh release create v0.1.0 --draft --verify-tag --title "Jetsam 0.1.0" \
     --notes-file notes.md Jetsam.dmg Jetsam.dmg.sha256
   ```

## Changelog sections

Safety (any change to what can be removed or ended), Added, Changed, Fixed. Each line starts with the area and ends with the PR number, and says what changed for the user in the past tense or as a plain fact. No emoji, and no auto-generated list of every merged PR.
