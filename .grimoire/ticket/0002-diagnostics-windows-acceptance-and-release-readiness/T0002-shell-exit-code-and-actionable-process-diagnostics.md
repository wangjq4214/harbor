# Shell Exit Code and Actionable Process Diagnostics

**Ticket ID:** T0002
**Source:** [Spec 0059](../../spec/0059-diagnostics-windows-acceptance-and-release-readiness.md) (R2, E3), [ADR 0113](../../adr/0113-diagnostics-crash-reporting-and-release-readiness.md), and [ADR 0071](../../adr/0071-three-second-notifications-including-errors.md)
**Status:** Open

## Goal

Surface child shell exit codes across the PTY boundary so normal exits (`0`) continue closing quietly while non-zero exits produce redacted structured logs and an actionable 3-second floating notice when the main window remains open, alongside clear configuration and launch failure diagnostics.

## Affected Surfaces

- `crates/harbor-pty/src/windows.rs`, `crates/harbor-pty/src/unix.rs`, and `crates/harbor-pty/src/lib.rs` (exit-code query on exited child handles)
- `crates/harbor-terminal/src/io.rs` and `crates/harbor-terminal/src/lib.rs` (expose observed shell exit status without mutating existing lifecycle assumptions)
- `src/tab_coordinator.rs` and `src/shell.rs` (structured redacted logging on non-zero exit and 3-second `NotificationCard` emission when window stays open)

## Approach

- Extend `PtyControl` in `harbor-pty` to retrieve `GetExitCodeProcess` once the child process is observed exited (`STILL_ACTIVE` mapped to `None`, exited `Some(u32)`), keeping `unix.rs` stub signature consistent.
- Expose `shell_exit_code(&self) -> Option<u32>` (or equivalent status method) on `TerminalIo` and `Terminal`.
- In `src/tab_coordinator.rs` where exited panes/tabs are retired, preserve existing automatic close semantics; when exit code is `Some(code)` with `code != 0`, emit a structured redacted tracing warning/error (excluding env values, full custom args, and terminal text) and enqueue a 3-second floating error notice if tabs remain and the main window stays open.
- Verify existing shell-launch diagnostics (`conpty/` payload mismatch, missing `shell.directory`, configured program fallback vs named-profile failure) emit clear log + notice messages without leaking secrets.

## Dependencies and Coordination

- **Blocked by:** None.
- **Blocks:** T0004 (resilience tests), T0005 (release acceptance).
- **Coordination risks:** Coordinate `src/shell.rs` touches with T0001; keep T0002 focused on PTY exit observation and `tab_coordinator` exit-event reporting.

## Acceptance

- [ ] Child process exit `0` closes the pane/tab quietly with zero error notices.
- [ ] Non-zero child exit code is retrieved through ConPTY, logged as a redacted structured event, and emits a 3-second floating notice showing the exit code when the main window remains open.
- [ ] Focused `harbor-pty`, `harbor-terminal`, and `harbor` ConPTY tests (`scripts/dev/test_pty.ps1`) pass for both `0` and non-zero exit scenarios, plus workspace `fmt`/`clippy`.

## Out of Scope

- Keeping crashed panes open in a frozen state or changing automatic close behavior.
- Capturing terminal scrollback or command output in exit logs.
