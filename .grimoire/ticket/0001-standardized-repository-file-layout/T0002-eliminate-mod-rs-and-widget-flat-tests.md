# Eliminate mod.rs and Relocate harbor-widget Flat Tests

**Ticket ID:** T0002
**Source:** [Spec 0058](../../spec/0058-standardized-repository-file-layout.md), [ADR 0112](../../adr/0112-standardized-repository-file-layout.md)
**Status:** Done

## Goal

Convert every `mod.rs` file in the workspace to Rust 2018+ `<module>.rs` + `<module>/` layout and clear flat test files out of `crates/harbor-widget/src/`.

## Affected Surfaces

- `crates/harbor-widget/src/{fiber,input,layout,renderer,runtime,scene,widgets,winit}/mod.rs` -> `crates/harbor-widget/src/{fiber,input,layout,renderer,runtime,scene,widgets,winit}.rs`
- `crates/harbor-terminal/src/render/mod.rs` -> `crates/harbor-terminal/src/render.rs`
- `crates/harbor-widget/src/layout_component_tests.rs` and `crates/harbor-widget/src/macro_tests.rs`
- `crates/harbor-widget/src/lib.rs`, `crates/harbor-terminal/src/lib.rs`, and `crates/harbor-widget/tests/`

## Approach

- Rename each `mod.rs` to its parent module's `.rs` sibling using `git mv` (adjusting any `#[path = ...]` or relative `include!` / `include_str!` / `include_bytes!` paths if present).
- Relocate `crates/harbor-widget/src/layout_component_tests.rs` into `crates/harbor-widget/src/layout/tests.rs` (or `crates/harbor-widget/tests/`) and `crates/harbor-widget/src/macro_tests.rs` into `crates/harbor-widget/tests/macro_tests.rs` (or an owning module test file), preserving all existing test cases and `lib.rs` behavior.

## Dependencies and Coordination

- **Blocked by:** None.
- **Blocks:** T0006 (automated layout gate enforces zero `mod.rs` and zero flat `*_tests.rs`).
- **Coordination risks:** Shares `crates/harbor-terminal/src/lib.rs` with T0005; land T0002 before T0005 or rebase `lib.rs` cleanly.

## Acceptance

- [x] Zero `mod.rs` files exist anywhere in `src/` or `crates/*/src/`.
- [x] Zero `*_tests.rs` files exist directly under `crates/harbor-widget/src/`.
- [x] `cargo test --locked -p harbor-widget -p harbor-widget-macros -p harbor-terminal` passes.
- [x] `cargo clippy --locked -p harbor-widget -p harbor-terminal --all-targets --all-features -- -D warnings` passes.

## Out of Scope

- Changing public `harbor-widget` or `harbor-terminal` API signatures.
- Relocating `crates/harbor-terminal/src/*_tests.rs` (handled by T0005).
