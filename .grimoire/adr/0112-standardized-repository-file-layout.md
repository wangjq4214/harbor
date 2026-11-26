# Standardized Repository File Layout

**Status:** Proposed
**Date:** 2026-11-26

## Context

Harbor's repository has grown across a root binary package, nine workspace crates under `crates/`, documentation under `docs/`, and automation under `scripts/`. Without an explicit structural contract, four layout inconsistencies have accumulated:

1. **Mixed Rust module styles:** `crates/harbor-widget` uses `dir/mod.rs` across eight subsystems and `crates/harbor-terminal` uses `src/render/mod.rs`, while all other modules in `harbor-terminal`, `harbor-app`, `harbor-text`, `harbor-config`, `harbor-parser`, `harbor-pty`, and root `src/` use the Rust 2018+ `foo.rs` plus `foo/` layout.
2. **Flat `*_tests.rs` proliferation in `src/`:** Root `src/` (47 tracked files), `crates/harbor-app/src/`, and `crates/harbor-terminal/src/` mix numerous `*_tests.rs` files directly at the `src/` root alongside production modules, while simultaneously using `<module>/tests.rs` and `tests/` integration directories.
3. **Unstructured top-level docs and undocumented scripts:** `docs/flex-layout.md` and `docs/widget-input.md` sit directly under `docs/` instead of `docs/widget/`, and `scripts/shell/` is absent from the directory index in `scripts/README.md`.
4. **No automated layout enforcement:** Existing checks (`scripts/docs/check_docs.py`) verify Markdown language and links, but nothing prevents new `mod.rs` files, flat `src/*_tests.rs` files, stray top-level docs, or unexpected tracked root directories.

Converting the repository to a pure virtual Cargo workspace (relocating root `src/` into `crates/harbor/`) was considered and rejected because it forces widespread path churn across CI workflows, Windows packaging scripts, `justfile`, and documentation for marginal visual gain.

## Decision

Adopt a single fixed repository file layout enforced across the workspace:

1. **Top-level tracked directories:** Retain root `src/` (and `tests/`, `build.rs`, `Cargo.toml`) as the `harbor` binary package. Tracked top-level directories are limited to the explicit allowlist: `.github`, `.grimoire`, `assets`, `crates`, `docs`, `fuzz`, `installer`, `scripts`, `src`, `tests`, and `third_party`. Local runtime outputs (`dhat-heap.json`, `harbor.log`) and local tooling directories (`.codegraph`, `.pi`, `.vscode`) remain gitignored.
2. **Rust 2018+ module style everywhere:** Every multi-file Rust module uses `<name>.rs` plus `<name>/` (or `lib.rs` / `main.rs` at crate roots). Zero `mod.rs` files are permitted anywhere in `src/` or `crates/*/src/`.
3. **Test placement boundary:** Flat `*_tests.rs` files are forbidden directly under `src/` or `crates/*/src/`.
   - Unit and module-private tests live inline (`#[cfg(test)] mod tests`) or in `<module>/tests.rs` (with `<module>/tests/*.rs` submodules when large).
   - Cross-module, black-box, public-contract, and native/GPU integration tests live in `<crate>/tests/` or root `tests/`, with shared harnesses under `tests/support/`.
   - Existing CI and `justfile` test-filter prefixes (`architecture::`, `gpu::`, `pty::`, `native::`, `perf::`) remain intact across relocations.
4. **Moderate internal module grouping:** Apply cohesive internal grouping only where flat sibling files create obvious sprawl (notably `harbor-app` workspace-owned modules such as `palette`, `quick_save`, and `quick_save_project` under `workspace/`, and `tab_view_ui` under `tab_view/`), while preserving all public crate API paths via `lib.rs` re-exports to minimize churn.
5. **Root-README-only `docs/` and `scripts/`:** `docs/` permits only `README.md` at its root; all topical documents reside under `docs/app/`, `docs/developer/`, `docs/terminal/`, or `docs/widget/` (relocating `docs/flex-layout.md` and `docs/widget-input.md` into `docs/widget/`). `scripts/` permits only `README.md` at its root; all scripts live in purpose-based subdirectories indexed in `scripts/README.md`.
6. **Automated layout gate:** Add `scripts/ci/check_layout.py` (with unit tests in `scripts/tests/test_check_layout.py`), expose it via `just layout-check`, and run it in the `quick` job of `.github/workflows/windows-quality-gates.yml`.

## Consequences

- Contributors and automation have an unambiguous, mechanically verified place for every source file, test module, doc, and script.
- Completing the migration requires phased relocations across `harbor-widget`, `harbor-terminal`, `harbor-app`, root `src/`, and `docs/`, while preserving public crate APIs and existing `justfile` / CI test filters.
- Future additions violating module style, test placement, `docs/`/`scripts/` organization, or the top-level allowlist fail fast in local checks and CI.
