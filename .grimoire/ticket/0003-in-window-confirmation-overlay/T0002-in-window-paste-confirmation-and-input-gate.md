# In-Window Paste Confirmation and Input Gate

**Ticket ID:** T0002
**Source:** [Spec 0057](../../spec/0057-in-window-confirmation-overlay.md) (`R1`, `R4`, `R5`, `R6`, `E1`, `E4`, `E6`, `E7`), [ADR 0111](../../adr/0111-in-window-liquid-glass-confirmation-overlay.md).
**Status:** Done

## Goal

Migrate multiline paste confirmation from `ConfirmationWindow` to the `PopupSlot` in-window confirmation overlay (`T0001`) and enforce an application-owned confirmation input gate on the main window while preserving `PaneBinding`/weak-`Terminal` revalidation, approval-time `InputModes`, palette handoff, and source-retirement cancellation.

## Affected Surfaces

- `crates/harbor-app`: `src/workspace.rs`, `src/command.rs`, `src/confirmation.rs`, `src/palette.rs`.
- `src/`: `dialog.rs`, `shell.rs`, and paste/dialog unit & shell tests (`src/dialog/tests.rs`, `src/dialog/paste_command_tests.rs`, `src/shell/tests.rs`).

## Approach

- Update `PasteController` and `ActiveSession` so `PasteDisposition::Confirm` opens the paste confirmation overlay in `PopupSlot` (automatically closing any displaced Search, Command Palette, or Quick Save via real-close cleanup) instead of creating a secondary `ConfirmationWindow`.
- Activate the confirmation input gate while a confirmation overlay is open so main-window terminal PTY keyboard input, workspace shortcuts, and terminal pointer activations are blocked while overlay interaction, wheel scrolling, and background terminal output rendering continue.
- Wire generation-scoped confirmation outcomes (`Confirm` / `Cancel`) through Host command/action processing: revalidate `PaneBinding` + weak `Terminal`, check active pane/tab liveness and shared-GPU fault state, read current `InputModes` at approval time, perform at most one Host-owned PTY write of the unchanged raw payload, clear the input gate, close the popup, restore focus, and report `palette_completion` if invoked from the Command Palette.
- Cancel the pending paste confirmation and release the input gate if the active pane/tab changes or the target session exits/retires.

## Dependencies and Coordination

- **Blocked by:** T0001 (requires the `Popup`-backed confirmation overlay and generation-scoped actions).
- **Blocks:** T0003 (OSC 52 migration and `ConfirmationWindow` retirement build on the unified in-window confirmation gate and `PopupSlot` flow), T0004.
- **Coordination risks:** Shared files `src/dialog.rs` and `src/shell.rs` are also used by OSC 52 until T0003; keep `ConfirmationWindow` available strictly for OSC 52 during this ticket or stage helper extraction cleanly for T0003.

## Acceptance

- [x] Triggering multiline paste opens the in-window `PopupSlot` overlay with no secondary OS window created (`ConfirmationWindow` is no longer spawned for paste).
- [x] While paste confirmation is open, PTY keyboard input and new paste requests are rejected/blocked while overlay buttons, `Esc`/`n`/`y`/arrow/`PageUp`/`PageDown` shortcuts, and wheel scrolling work and terminal output keeps rendering.
- [x] Confirming writes the unchanged raw text once encoded with approval-time `InputModes`; cancelling writes zero bytes and restores terminal focus.
- [x] Opening paste confirmation while Search or Command Palette is open replaces and cleans up the displaced popup (clearing search query/highlights); palette-invoked paste returns `PendingConfirmation` then `Applied`/`Cancelled`/`Rejected`.
- [x] Active tab/pane switch, source session retirement, or shared-GPU fault cancels the pending paste without writing to the PTY and clears the input gate.

## Out of Scope

- Migrating OSC 52 confirmation or deleting `ConfirmationWindow` (T0003).
- Full interactive Windows visual acceptance (T0004).
