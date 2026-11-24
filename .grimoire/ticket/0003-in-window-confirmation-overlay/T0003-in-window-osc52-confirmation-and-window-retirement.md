# In-Window OSC 52 Confirmation and Window Retirement

**Ticket ID:** T0003
**Source:** [Spec 0057](../../spec/0057-in-window-confirmation-overlay.md) (`R1`, `R3`, `R4`, `R5`, `R6`, `R7`, `E3`, `E5`, `E6`), [Spec 0017](../../spec/0017-host-authorized-osc52-clipboard-writes.md), [ADR 0111](../../adr/0111-in-window-liquid-glass-confirmation-overlay.md).
**Status:** Todo

## Goal

Migrate OSC 52 clipboard-write confirmation into the shared `PopupSlot` in-window confirmation overlay under strict main-window foreground eligibility, enforce mutual exclusion across paste and OSC 52 confirmations, and retire `ConfirmationWindow` plus obsolete secondary-window routing.

## Affected Surfaces

- `src/`: `shell.rs`, `clipboard.rs`, `dialog.rs`, and their tests (`src/shell/tests.rs`, `src/dialog/tests.rs`).
- `crates/harbor-app`: `src/confirmation.rs`, `src/workspace.rs` (OSC 52 confirmation mode wiring if needed).

## Approach

- Route `Admission::Confirm` in `ActiveSession::handle_clipboard_request` to open the shared `PopupSlot` confirmation overlay using `clipboard::confirmation_presentation` (`PREVIEW_CHARS` 1024-char truncation with `[preview truncated]` and `[clear clipboard]` empty-payload marker), keeping the confirmation input gate active while open.
- Enforce unified single-confirmation admission across both flows: if either a paste confirmation or an OSC 52 confirmation is active, reject incoming paste requests and incoming OSC 52 confirmation requests without queuing, opening another overlay, or altering the active preview.
- Remove the secondary-window focus continuation exception (`confirmation_foreground` / `continuation` secondary-window checks in `src/shell.rs` and `src/clipboard.rs`) so OSC 52 confirmation requires strict main-window foreground focus from admission through resolution. Cancel pending OSC 52 confirmation on source closure, active tab/pane change, external-application focus loss, or window minimization, and emit only content-free failure diagnostics.
- Retire `ConfirmationWindow`, its secondary-window event loop/redraw/fatal-exit branches in `src/shell.rs`, and obsolete secondary-window-only helpers/tests while preserving all platform/headless unit coverage under the new in-window path.

## Dependencies and Coordination

- **Blocked by:** T0002 (requires in-window `PopupSlot` paste confirmation and input gate).
- **Blocks:** T0004 (Windows runtime acceptance and documentation reconciliation).
- **Coordination risks:** Carefully update `apply_gpu_fatal_policy`, `reconcile_clipboard`, and `FocusChanged` / `Occluded` handling in `src/shell.rs` so external focus loss cancels pending OSC 52 confirmation cleanly without leaking stale `ClipboardPolicy` state.

## Acceptance

- [ ] Under OSC 52 `confirm` policy, eligible requests open the shared in-window `PopupSlot` overlay showing source tab, decoded byte size, and bounded preview (`[preview truncated]` / `[clear clipboard]`), displacing any open Search/Palette.
- [ ] `Allow this request` (`y`/`Y` or button) revalidates source liveness and strict main-window foreground focus, executes the Host clipboard write once without creating a permanent grant, and restores terminal focus.
- [ ] `Deny this request` (`Esc`/`n`/`N` or button), source session closure, active tab/pane change, window minimization, or main-window focus loss cancels the pending OSC 52 confirmation with zero clipboard writes and content-free diagnostics.
- [ ] While either paste confirmation or OSC 52 confirmation is active, new paste and new OSC 52 requests are both rejected without altering the active preview.
- [ ] `ConfirmationWindow` and secondary-window confirmation routing are removed from `src/`, and scoped `cargo test` and `cargo clippy --locked --all-targets --all-features -- -D warnings` pass.

## Out of Scope

- Changing OSC 52 protocol subset, 4 MiB capacity limits, or default `allow` / `deny` semantics ([Spec 0017](../../spec/0017-host-authorized-osc52-clipboard-writes.md)).
- Native desktop visual sign-off (T0004).
