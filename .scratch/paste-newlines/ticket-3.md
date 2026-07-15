## Parent

Issue #18 — Paste Newline Confirmation

## What to build

Platform polish and edge-case hardening for the paste confirmation dialog. Owned-window relationship on Windows, correct dialog positioning, mode-change handling, unified paste entry points, and main-window interaction during dialog.

## Acceptance criteria

- [ ] On Windows, the dialog window is an owned window (with_owner_window) of the main Harbor window. This keeps it above the main window in z-order and ties minimize/destroy together.
- [ ] Dialog is fixed-size (600x400 pixels) and positioned centered over the main window at creation time.
- [ ] Dialog does not move or re-center when the main window is resized while the dialog is open.
- [ ] Dialog is not draggable. It stays at its creation position.
- [ ] Dialog has no native title bar (decorations: false). Close is only via buttons or keyboard.
- [ ] When InputModes (bracketed paste) changes between dialog-open and user-confirm, the dialog stays open. On confirm, the current (latest) InputModes is used for bracketed-paste framing in send_paste.
- [ ] Pressing Ctrl+V (or Shift+Insert) again while the dialog is already open is a no-op. No second dialog is created.
- [ ] All paste entry points are unified under the same confirmation gate: Ctrl+V and Shift+Insert both trigger the dialog for multi-line non-BP paste. Future paste UI will also use the same path.
- [ ] While dialog is open, the main window remains interactive for: resize, redraw, PTY output rendering (scroll-to-bottom snap NOT suppressed), scrollback navigation (mouse wheel, PageUp/Down), and copy (Ctrl+C on selection).
- [ ] While dialog is open, main-window events that would produce PTY input or new selections are suppressed: character keys, Enter, Backspace, new mouse selection initiation.
- [ ] After dialog close (confirm or cancel), main-window input resumes immediately in the same frame.
- [ ] On non-Windows platforms, owned-window behavior gracefully degrades (dialog still opens as an independent window without z-order guarantee).
- [ ] Smoke test: open dialog, scroll main window scrollback with mouse wheel, verify PTY output continues rendering, resize main window (dialog stays put), confirm paste — all normal.

## Blocked by

- #21 — Paste gate + basic confirmation dialog
