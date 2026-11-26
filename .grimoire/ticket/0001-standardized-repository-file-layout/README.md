# Standardized Repository File Layout

**Source:** [Spec 0058](../../spec/0058-standardized-repository-file-layout.md) and [ADR 0112](../../adr/0112-standardized-repository-file-layout.md)
**Ticket folder:** `.grimoire/ticket/0001-standardized-repository-file-layout/`

## Overview

Reorganize existing repository files to eliminate mixed `mod.rs` and `foo.rs` module styles, clear flat `*_tests.rs` files out of `src/` roots, apply moderate domain grouping in `harbor-app`, tidy `docs/` and `scripts/`, and enforce the final layout with an automated CI/local gate.

## Delivery Surfaces

- Documentation (`docs/`, `docs/widget/`, `docs/developer/architecture.md`, `scripts/README.md`, and cross-references across `docs/` and `.grimoire/`)
- Rust modules and tests in `crates/harbor-widget/`, `crates/harbor-terminal/`, `crates/harbor-app/`, and root `src/` / `tests/`
- Layout validation script (`scripts/ci/check_layout.py`, `scripts/tests/test_check_layout.py`), `justfile`, and `.github/workflows/windows-quality-gates.yml`

## Dependencies

| Producer | Consumer | Why consumer cannot proceed first |
| --- | --- | --- |
| T0001 | T0006 | `docs/` and `scripts/` must match the root-README-only invariant before the layout gate passes |
| T0002 | T0006 | Zero `mod.rs` and `harbor-widget` flat test cleanup must land before the layout gate passes |
| T0003 | T0006 | Root `src/*_tests.rs` relocation must land before the layout gate passes |
| T0004 | T0006 | `harbor-app` flat test relocation and submodule grouping must land before the layout gate passes |
| T0005 | T0006 | `harbor-terminal` flat test relocation must land before the layout gate passes |

## Coordination

| Tickets | Risk | Strategy |
| --- | --- | --- |
| T0002, T0005 | Shared file `crates/harbor-terminal/src/lib.rs` | Land T0002 (`render/mod.rs` -> `render.rs`) before T0005 or rebase `lib.rs` module declarations cleanly |

## Recommended Order

1. Run **T0001**, **T0002**, **T0003**, and **T0004** in parallel (disjoint directories).
2. Run **T0005** after **T0002** (or coordinate edits to `crates/harbor-terminal/src/lib.rs`).
3. Run **T0006** once T0001-T0005 are complete so the layout gate passes immediately across the whole repository.

## Ticket Index

| Ticket | File | Outcome |
| --- | --- | --- |
| T0001 | [T0001-docs-and-scripts-layout.md](./T0001-docs-and-scripts-layout.md) | Relocate stray top-level docs into `docs/widget/`, index `scripts/shell/`, and document layout rules |
| T0002 | [T0002-eliminate-mod-rs-and-widget-flat-tests.md](./T0002-eliminate-mod-rs-and-widget-flat-tests.md) | Replace all `mod.rs` files with `<module>.rs` and relocate `harbor-widget` flat tests |
| T0003 | [T0003-reorganize-binary-src-tests-and-modules.md](./T0003-reorganize-binary-src-tests-and-modules.md) | Relocate flat `src/*_tests.rs` from the root binary crate while preserving test filters |
| T0004 | [T0004-reorganize-harbor-app-submodules-and-tests.md](./T0004-reorganize-harbor-app-submodules-and-tests.md) | Relocate `harbor-app` flat tests and group `workspace` / `tab_view` submodules |
| T0005 | [T0005-reorganize-harbor-terminal-flat-tests.md](./T0005-reorganize-harbor-terminal-flat-tests.md) | Relocate `harbor-terminal` flat tests while preserving feature gates and filters |
| T0006 | [T0006-automated-layout-gate.md](./T0006-automated-layout-gate.md) | Add `scripts/ci/check_layout.py`, unit tests, `just layout-check`, and CI gate |
