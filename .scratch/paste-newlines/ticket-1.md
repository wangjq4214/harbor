## Parent

Issue #18 — Paste Newline Confirmation

## What to build

The end-to-end confirmation gate for multi-line paste. When bracketed paste is OFF and the clipboard text contains meaningful newlines, a wgpu-rendered dialog window opens to confirm before sending to the PTY.

This ticket delivers: pure detection logic, the confirmation gate wired into the paste path, and a minimal dialog window that shows the line count + two buttons. No text preview body yet — just the header "Paste N lines?" and [Paste] [Cancel] buttons.


## Acceptance criteria

- [ ] `should_confirm_multiline(text) -> bool` trims trailing newline sequences (CRLF, LF, CR) recursively, returns true if any newline remains. Covered by table-driven unit tests (empty, single-line, single-line+trailing, multi-line, Windows CRLF, mixed endings, all-newlines).
- [ ] `safe_preview_line(line) -> String` escapes C0 control characters (U+0000-U+001F, U+007F) to visible markers. Covered by unit tests (plain text, tab, ESC, CR, LF, null, DEL, multiple controls, CJK).
- [ ] `PasteDisposition` enum with variants `SendDirect` and `Confirm { raw_text }`.
- [ ] `PasteDialog` struct creates a secondary winit Window + wgpu Surface using the main window's shared Device/Queue. No second GPU context.
- [ ] Dialog window renders a header label ("Paste N lines?") and two text-button labels ([Paste] [Cancel]) using the terminal's font and colors. No text preview body in this ticket.
- [ ] Dialog uses the terminal's glyph atlas and font metrics (UiRoot exposes necessary accessors).
- [ ] Confirmation gate: upon paste, obtain clipboard text, compute PasteDisposition. SendDirect calls send_paste as before. Confirm opens the PasteDialog with raw text stored.
- [ ] App holds Option<PasteDialog>. window_event dispatches to dialog by window_id when active.
- [ ] Keyboard handling in dialog: Esc = cancel/close. Enter = activates the focused button. Default button focus on open: Cancel (so initial Enter = cancel).
- [ ] On confirm: calls send_paste with current InputModes. Destroys dialog window. Resumes main-window input.
- [ ] On cancel: discards text. Destroys dialog window. Resumes main-window input.
- [ ] While dialog is open: PTY output continues draining and rendering. Main-window keyboard input (character keys, Enter, Backspace) is blocked from reaching the PTY.
- [ ] `should_confirm_multiline` and `safe_preview_line` unit tests pass. The dialog can be smoke-tested by pasting multi-line text and observing the dialog open, then confirming that text appears in the shell.

## Blocked by

None — can start immediately.
