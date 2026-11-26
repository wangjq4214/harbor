# Reorganize Root Binary src/ Tests

**Ticket ID:** T0003
**Source:** [Spec 0058](../../spec/0058-standardized-repository-file-layout.md), [ADR 0112](../../adr/0112-standardized-repository-file-layout.md)
**Status:** Todo

## Goal

Remove all flat `*_tests.rs` files directly under root `src/` by placing module-scoped tests inside `<module>/tests.rs` (with `<module>/tests/*.rs` submodules) or root `tests/`, without breaking `justfile` / CI filter paths.

## Affected Surfaces

- Root `src/main.rs`
- Flat test files at `src/*_tests.rs`:
  - `src/action_batch_tests.rs`
  - `src/configuration_notice_tests.rs`
  - `src/configuration_reload_tests.rs` and `src/configuration_reload_tests/`
  - `src/configuration_watch_tests.rs`
  - `src/pane_lifecycle_tests.rs`
  - `src/paste_command_tests.rs`
  - `src/session_launch_tests.rs` and `src/session_launch_tests/`
  - `src/session_profile_tests.rs`
- Owning modules under `src/` (such as `src/configuration_watch/`, `src/pane_commands/`, `src/profile_launch/`, `src/shell/`, `src/tab_coordinator/`) and/or `tests/`

## Approach

- Audit each flat `src/*_tests.rs` file to check whether it accesses private binary-crate items (`super::*`, `crate::shell::*`, etc.) or can be hosted under an owning module's `tests.rs` (for example `configuration_watch/tests.rs`, `shell/tests/`, `profile_launch/tests/`).
- Preserve all `#[cfg(test)]`, `#[ignore]`, and module-path filter segments (`architecture::`, `gpu::`, `pty::`, `native::`) used by `just test-architecture`, `just test-gpu`, `just test-pty`, and `just test-native`.
- Update relative `#[path = ...]` references to `tests/support/gpu.rs` and `tests/support/launch_probe.rs` where needed.

## Dependencies and Coordination

- **Blocked by:** None.
- **Blocks:** T0006 (automated layout gate forbids `src/*_tests.rs`).
- **Coordination risks:** Binary-crate integration tests in root `tests/` cannot import private items from `src/main.rs`; keep tests that rely on binary-private internals under `<module>/tests.rs` inside `src/`.

## Acceptance

- [ ] Zero `*_tests.rs` files remain directly under root `src/`.
- [ ] `cargo test --locked -p harbor` passes.
- [ ] `just test-architecture` discovers and runs all root binary architecture tests.
- [ ] `cargo clippy --locked -p harbor --all-targets --all-features -- -D warnings` passes.

## Out of Scope

- Moving root `src/` into `crates/harbor/`.
- Relocating production modules in root `src/` that do not own relocated tests.
