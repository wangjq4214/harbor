# Bounded Logging, Panic Hook, and Fatal Feedback

**Ticket ID:** T0001
**Source:** [Spec 0059](../../spec/0059-diagnostics-windows-acceptance-and-release-readiness.md) (R1, E1, E2) and [ADR 0113](../../adr/0113-diagnostics-crash-reporting-and-release-readiness.md)
**Status:** Open

## Goal

Ensure Harbor writes size-bounded rolling logs and count-bounded crash records without panicking on unwritable paths, and gives interactive Windows users visible feedback when a panic, startup failure, or fatal GPU error terminates the process without a usable UI window.

## Affected Surfaces

- `src/main.rs` (tracing initialization, log directory resolution, panic hook setup)
- Host diagnostics module (e.g., `src/diagnostics.rs` for rolling writer, crash persistence, retention pruning, and Windows `MessageBoxW` wrapper)
- `src/shell.rs` (`ShellError`, `try_resume`, `exit_gpu_fault`, `HostFrameOutcome::Fatal` handling)
- [Troubleshooting](../../../docs/app/troubleshooting.md) (update log rotation and crash-file details if needed)

## Approach

- Replace `tracing_appender::rolling::never` with a size-bounded rolling log writer (default reversible parameters: `5 MiB` per file, up to `3` rotated files) in the existing log directory (`cwd` in debug, `~/.harbor/` in release, unchanged by `harbor.portable`).
- Make log directory creation and writer initialization fallible/graceful so unwritable paths degrade cleanly without panicking.
- Implement a shared fatal/crash reporter that writes a redacted `crash-<utc-timestamp>.log` (pruning older crash files past the retention limit of `10`), flushes logs where possible, and invokes a Windows `MessageBoxW` summary when no usable Harbor UI window is available and dialogs are not suppressed (e.g., `HARBOR_SUPPRESS_FATAL_DIALOG=1` or `#[cfg(test)]`).
- Wire a `std::panic::set_hook` in `src/main.rs` and route fatal `ShellError` / `HostFrameOutcome::Fatal` exits in `src/shell.rs` through the shared reporter.

## Dependencies and Coordination

- **Blocked by:** None.
- **Blocks:** T0003 (diagnostic bundle export consumes the rolling log and crash-file layout), T0004 (resilience tests), T0005 (release acceptance).
- **Coordination risks:** Coordinate `src/shell.rs` edits with T0002; keep T0001 scoped to fatal startup/GPU/panic reporting.

## Acceptance

- [ ] `harbor.log` rotates deterministically under configured size and file-count bounds; unwritable log directories do not panic.
- [ ] Panics, fatal startup errors, and `HostFrameOutcome::Fatal` persist redacted `crash-*.log` files within the retention count bound without logging terminal contents, clipboard data, or environment variable values.
- [ ] Interactive Windows fatal exits without a usable UI window show a `MessageBoxW` naming the failure category, crash/log path, and non-inclusion of terminal contents, while automated tests suppress the dialog cleanly.
- [ ] `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`, and focused unit/host tests pass via `scripts/dev/test_cargo.ps1`.

## Out of Scope

- Diagnostic bundle CLI or Command Palette export (T0003).
- Child shell exit-code inspection or per-pane exit notices (T0002).
- Relocating release logs into `harbor.portable` directories.
