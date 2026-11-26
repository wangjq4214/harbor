# Standardized Repository File Layout

**Spec ID:** 0058
**Status:** Draft
**Date:** 2026-11-26
**Sources:** Conversation refinement contract and [ADR 0112](../adr/0112-standardized-repository-file-layout.md)

## Requirements

1. **Top-level repository layout:**
   - The root package (`Cargo.toml`, `build.rs`, `src/`, `tests/`) remains the `harbor` binary crate; workspace member crates remain under `crates/*`.
   - Tracked top-level directories are restricted to `.github`, `.grimoire`, `assets`, `crates`, `docs`, `fuzz`, `installer`, `scripts`, `src`, `tests`, and `third_party`.
   - Local runtime and tooling artifacts (`dhat-heap.json`, `harbor.log`, `.codegraph/`, `.pi/`, `.vscode/`) remain excluded from Git tracking via `.gitignore`.
2. **Rust module file convention:**
   - Every workspace crate and the root binary package uses Rust 2018+ module layout (`<module>.rs` paired with `<module>/`).
   - Zero `mod.rs` files exist anywhere under `src/` or `crates/*/src/`.
3. **Rust test placement:**
   - Zero `*_tests.rs` files exist directly at `src/` or `crates/*/src/` roots.
   - Module-scoped unit tests live either inline (`#[cfg(test)] mod tests`) or in `<module>/tests.rs` (with `<module>/tests/*.rs` submodules when split).
   - Cross-module, public-API, and host/GPU integration tests live under `<crate>/tests/` or root `tests/`, with shared helpers under `tests/support/`.
   - Relocated tests preserve module path segments required by `justfile` and CI filters (`architecture::`, `gpu::`, `pty::`, `native::`, `perf::`).
4. **Moderate internal module grouping with API compatibility:**
   - Cohesive internal modules in `harbor-app` that belong to an existing domain folder (`palette`, `quick_save`, `quick_save_project` owned by `workspace`; `tab_view_ui` owned by `tab_view`) are grouped under their owning folder while keeping all public `harbor_app::*` re-exports compatible in `crates/harbor-app/src/lib.rs`.
   - Non-target production modules keep their existing locations unless explicitly required by a test/submodule move.
5. **Documentation and scripts organization:**
   - `docs/` contains only `README.md` and domain subdirectories (`app/`, `developer/`, `terminal/`, `widget/`) at its root; `docs/flex-layout.md` and `docs/widget-input.md` move into `docs/widget/` with all internal references repaired across `docs/` and `.grimoire/`.
   - `scripts/` contains only `README.md` and purpose subdirectories (`ci/`, `dev/`, `docs/`, `packaging/`, `profiling/`, `setup/`, `shell/`, `terminal/`, `tests/`) at its root, and `scripts/README.md` indexes `shell/`.
   - `docs/developer/architecture.md` documents the fixed repository layout conventions.
6. **Automated layout gate:**
   - A standard-library Python script `scripts/ci/check_layout.py` validates rules 1, 2, 3, and 5 against Git-tracked (and non-ignored working-tree) files and exits non-zero with actionable paths on any violation.
   - `just layout-check` invokes `python scripts/ci/check_layout.py`, and the `quick` job in `.github/workflows/windows-quality-gates.yml` runs it alongside `scripts/docs/check_docs.py`.

## Solution

The migration aligns existing files in independent slices before turning on the automated layout gate:

1. Move stray top-level docs (`docs/flex-layout.md`, `docs/widget-input.md`) into `docs/widget/`, repair links, document `scripts/shell/` in `scripts/README.md`, and record the layout map in `docs/developer/architecture.md`.
2. Replace all nine `mod.rs` files (eight in `crates/harbor-widget/src/*/mod.rs` and `crates/harbor-terminal/src/render/mod.rs`) with `<module>.rs` counterparts and move `crates/harbor-widget/src/layout_component_tests.rs` and `macro_tests.rs` into owning submodules or `crates/harbor-widget/tests/`.
3. Relocate flat `*_tests.rs` files from root `src/`, `crates/harbor-app/src/`, and `crates/harbor-terminal/src/` into owning `<module>/tests.rs` (`<module>/tests/*.rs`) or `<crate>/tests/`, and group `harbor-app` `workspace` / `tab_view` submodules while preserving `lib.rs` public re-exports and `justfile` test filter compatibility.
4. Land `scripts/ci/check_layout.py` and `scripts/tests/test_check_layout.py`, wire `just layout-check`, and enable the check in the CI `quick` lane.

### Seams

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Layout checker CLI | `scripts/ci/check_layout.py` -> `justfile` / CI `quick` job | Clean exit code `0` on compliant repo; deterministic non-zero exit with offending paths on violation | Automated enforcement of allowlist, zero `mod.rs`, zero flat `*_tests.rs`, and `docs/`/`scripts/` root discipline |
| Crate public exports | Reorganized internal modules -> `crates/harbor-app/src/lib.rs`, `crates/harbor-terminal/src/lib.rs` | Unchanged public item paths (`pub mod` / `pub use`) for downstream binaries, `harbor-app-ui`, and integration tests | Internal reorganization without breaking consumers |
| Test filter discovery | Relocated test modules -> `justfile` and `.github/workflows/windows-quality-gates.yml` | Module paths retain `architecture::`, `gpu::`, `pty::`, `native::`, `perf::` segments | Unbroken selective test runs across unit and integration suites |
| Documentation links | Relocated `docs/widget/` files -> `scripts/docs/check_docs.py` | Valid relative file links and heading fragments across `docs/` and `.grimoire/` | Zero broken Markdown references |

## End-to-End Tests

### Clean repository passes the layout gate
- **Given:** All migration slices (`docs/`, `scripts/`, `harbor-widget`, `harbor-terminal`, `harbor-app`, root `src/`) have been applied.
- **When:** Running `python scripts/ci/check_layout.py`.
- **Then:** The command exits `0` and reports zero layout violations.

### Layout gate rejects prohibited file placements
- **Given:** A temporary test fixture repository (or unit test directory) containing a `crates/example/src/foo/mod.rs`, a `src/example_tests.rs`, a `docs/stray.md`, or an unlisted top-level tracked directory.
- **When:** Running the layout checker's validation functions via `python -B -m unittest discover -s scripts/tests -v`.
- **Then:** Each violation is explicitly reported by rule and relative path and fails the check.

### Existing quality gates remain green after migration
- **Given:** All source and documentation relocations are complete.
- **When:** Running `python scripts/docs/check_docs.py`, `just qa`, `just test-architecture`, and `cargo test --locked -p harbor-terminal --no-default-features`.
- **Then:** All checks pass with no broken links, compilation errors, or lost test discovery.

## Decisions

- **Choice:** Retain root `src/` as the `harbor` binary package rather than converting to a pure virtual workspace.
  - **Reason/source:** Minimizes CI, packaging, and script path churn while enforcing an explicit top-level directory allowlist; [ADR 0112](../adr/0112-standardized-repository-file-layout.md).
- **Choice:** Standardize on Rust 2018+ `<module>.rs` + `<module>/` and forbid `mod.rs` and flat `src/*_tests.rs`.
  - **Reason/source:** Eliminates mixed conventions between `harbor-widget` and the rest of the workspace and removes test clutter from `src/` roots; [ADR 0112](../adr/0112-standardized-repository-file-layout.md).
- **Choice:** Apply moderate internal module grouping (`harbor-app` `workspace` and `tab_view` submodules) while keeping public APIs intact, rather than moving every production file.
  - **Reason/source:** Balances cleaner crate organization against git history stability; [ADR 0112](../adr/0112-standardized-repository-file-layout.md).
- **Choice:** Enforce the layout with `scripts/ci/check_layout.py` in `justfile` and CI `quick`.
  - **Reason/source:** Prevents gradual layout drift after migration; [ADR 0112](../adr/0112-standardized-repository-file-layout.md).

## Test Plan

- `python scripts/ci/check_layout.py`
- `python -B -m unittest discover -s scripts/tests -v`
- `python scripts/docs/check_docs.py`
- `just qa`
- `just test-architecture`
- `cargo test --locked -p harbor-terminal --no-default-features`

## Out of Scope

- Converting the repository root into a pure virtual Cargo workspace (`crates/harbor/`).
- Breaking or renaming public Rust crate APIs (`harbor-widget`, `harbor-terminal`, `harbor-app`, `harbor-config`, `harbor-parser`, `harbor-pty`, `harbor-text`).
- Changing runtime output paths for `harbor.log` or `dhat-heap.json`.
- Modifying behavioral application, terminal, or widget logic.
