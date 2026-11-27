# Failure-Mode, Redaction, and Resilience Test Suite

**Ticket ID:** T0004
**Source:** [Spec 0059](../../spec/0059-diagnostics-windows-acceptance-and-release-readiness.md) (R4, E1–E6) and [Testing](../../../docs/developer/testing.md)
**Status:** Open

## Goal

Deliver automated test coverage validating Harbor's behavior under failure, resource pressure, shutdown, GPU presentation faults, and strict diagnostic redaction invariants.

## Affected Surfaces

- Binary crate unit/integration tests (`src/` module tests and `tests/` integration tests using `tests/support/`)
- `crates/harbor-pty` / `crates/harbor-terminal` / `crates/harbor-widget` fault-injection tests (where boundary coverage belongs)
- `justfile` / `scripts/dev/` (only if existing resource-filtered recipes need verification coverage; preserve existing filter prefixes)

## Approach

- Add deterministic tests for:
  1. Log rotation size caps, crash file count pruning, and unwritable log-directory fallback without panics.
  2. Fatal/panic report formatting and dialog suppression (`HARBOR_SUPPRESS_FATAL_DIALOG=1`).
  3. Non-zero vs zero child process exit propagation through ConPTY (`test_pty.ps1`), including final-tab exit and multi-tab window-open notice generation.
  4. Strict redaction end-to-end checks seeding sentinel terminal text, clipboard text, secret environment variables, and secret `config.toml` comments/values, then verifying none appear in `harbor --diagnostics` or Command Palette bundle outputs.
  5. Sustained high-volume PTY output, bounded history eviction under pressure, clean shutdown with active split panes/tabs, and `HostFrameOutcome::Fatal` shared-GPU fault exit handling (`dx12-warp` where GPU is exercised).

## Dependencies and Coordination

- **Blocked by:** T0001 (logging/crash/fatal feedback), T0002 (exit-code diagnostics), T0003 (diagnostic bundle export).
- **Blocks:** T0005 (Daily-use release acceptance requires passing resilience verification).
- **Coordination risks:** Respect repository layout rules ([ADR 0112](../../adr/0112-standardized-repository-file-layout.md)): no flat `src/*_tests.rs` files; keep resource-dependent tests under `gpu::`, `pty::`, or `native::` with `#[ignore]`.

## Acceptance

- [ ] Automated unit, ConPTY (`scripts/dev/test_pty.ps1`), and `dx12-warp` GPU fault tests (`scripts/dev/test_cargo.ps1`) pass for every covered failure scenario without hangs or panics.
- [ ] Sentinel redaction tests prove terminal content, clipboard content, environment values, and raw `config.toml` contents do not leak into diagnostic bundles or crash records.
- [ ] `python scripts/ci/check_layout.py`, `cargo fmt --all -- --check`, and `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` pass.

## Out of Scope

- Live GitHub Release download smoke or manual UAC installer testing (T0005).
- Interactive Windows dogfood sessions (T0005).
