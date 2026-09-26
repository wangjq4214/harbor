# Local tooling and WASDK layout

- **Input:** Settled conversation: stop tracking `.pi/` and `.vscode/` while preserving local files; relocate `.tools/wasdk/` into `third_party/wasdk/`; provide a ConPTY-like pinned, hash-verified downloader for only the packages required by WASDK binding generation. Downloaded data stays untracked.
- **Date:** 2026-09-26
- **Status:** Completed

## Summary

Reorganize the tracked WASDK generator and pin metadata, keep local SDK downloads under ignored `third_party/wasdk/`, and untrack personal IDE/agent configuration without deleting it. No runtime WASDK packaging change.

## Implementation steps

1. Extend root `.gitignore` for `.pi/`, `.vscode/`, and ignored downloaded SDK material in `third_party/wasdk/`. Remove tracked personal settings from Git's index using `git rm --cached` and confirm on-disk copies remain.
2. Move `.tools/wasdk` to `third_party/wasdk` preserving local ignored data and the tracked generator/pins; update `src/backdrop/wasdk.rs`, interop comments, and generator usage/default output path. Keep the generator separate from the root workspace.
3. Add `scripts/fetch_wasdk.ps1` for the pinned InteractiveExperiences and Foundation NuGet packages only: hash-check both downloaded and supplied package files, use temporary downloads, extract safely into the ignored destination, and document use/versions/hashes in `third_party/wasdk/README.md` and root README.
4. Verify `git status`, `git check-ignore`, `git ls-files`, local file presence, paths and pin/hash consistency; run PowerShell script checks (offline using local packages where available), generator build/check and relevant repository formatting/static checks.

## Risks and verification

- `.gitignore` does not untrack existing files: explicitly remove index entries only; assert files still exist.
- Moving ~498 MB of ignored local SDK data risks data loss: preserve it with a directory move and check sample package hashes/metadata afterward; never commit payloads.
- Re-downloading corrupted packages risks partial overwrite: check SHA256 before extracting, extract to staging under `target/`, and only publish after both archives pass; ensure failed runs do not damage the existing SDK materials.
- Bindings are checked-in and runtime behavior is unchanged: ensure generator resolves both metadata directories and points at `src/backdrop/wasdk/generated.rs`; check repository references.
- No new tests are necessary if the downloader can be exercised offline against the pinned local archives and static Git/path assertions cover the rest.
