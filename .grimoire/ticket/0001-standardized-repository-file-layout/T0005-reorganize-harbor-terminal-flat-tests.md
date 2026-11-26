# Reorganize harbor-terminal Flat Tests

**Ticket ID:** T0005
**Source:** [Spec 0058](../../spec/0058-standardized-repository-file-layout.md), [ADR 0112](../../adr/0112-standardized-repository-file-layout.md)
**Status:** Todo

## Goal

Remove all flat `*_tests.rs` files from `crates/harbor-terminal/src/` by relocating them into owning `<module>/tests.rs` / `<module>/tests/` or `crates/harbor-terminal/tests/`, preserving all feature gates (`renderer` vs `--no-default-features`) and CI test-filter segments.

## Affected Surfaces

- `crates/harbor-terminal/src/lib.rs`
- Flat tests in `crates/harbor-terminal/src/`:
  - `async_resize_tests.rs`
  - `clipboard_attachment_tests.rs`
  - `core_tests.rs`
  - `engine_tests.rs`
  - `hot_path_tests.rs`
  - `mouse_tests.rs`
  - `palette_tests.rs`
  - `terminal_tests.rs` and `terminal_tests/`
  - `text_admission_tests.rs`
  - `update_tests.rs`
- Owning submodules under `crates/harbor-terminal/src/` and/or `crates/harbor-terminal/tests/`

## Approach

- Relocate module-private unit tests into their owning module directories (for example `resize/tests.rs`, `update/tests.rs`, `input/tests.rs`, `model/tests.rs`) and cross-module / public-API / renderer-gated integration suites into `crates/harbor-terminal/tests/` (preserving `#[cfg(feature = "renderer")]` and `#[cfg(not(feature = "renderer"))]` gating as needed).
- Preserve `perf::`, `gpu::`, and `pty::` module path segments used by `justfile` and CI workflows.
- Adjust relative `#[path = "../../../tests/support/gpu.rs"]` declarations to match new module depths.

## Dependencies and Coordination

- **Blocked by:** T0002 (coordinates edits to `crates/harbor-terminal/src/lib.rs` where `render/mod.rs` becomes `render.rs`).
- **Blocks:** T0006 (automated layout gate forbids `crates/harbor-terminal/src/*_tests.rs`).
- **Coordination risks:** Verify both default (`renderer` enabled) and `--no-default-features` compilations so no test module loses its feature gate.

## Acceptance

- [ ] Zero `*_tests.rs` files remain directly under `crates/harbor-terminal/src/`.
- [ ] `cargo test --locked -p harbor-terminal` passes.
- [ ] `cargo test --locked -p harbor-terminal --no-default-features` passes.
- [ ] `cargo clippy --locked -p harbor-terminal --all-targets --all-features -- -D warnings` passes.

## Out of Scope

- Reorganizing production modules inside `crates/harbor-terminal/src/` beyond `render/mod.rs` (covered by T0002).
