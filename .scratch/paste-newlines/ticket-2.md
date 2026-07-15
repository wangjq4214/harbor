## Parent

Issue #18 — Paste Newline Confirmation

## What to build

Render the full paste text body inside the confirmation dialog with C0 control-character escaping, virtualized scrolling, line wrapping, and rich keyboard shortcuts. Builds on the dialog scaffold from the gate ticket.

## Acceptance criteria

- [ ] Dialog renders the paste text body using the terminal's glyph atlas (shared reference). Text inherits terminal font, foreground/background colors.
- [ ] Virtualized rendering: only visible rows (plus a small scroll-buffer margin) are converted to glyph quads and uploaded. No eager all-at-once rendering for large pastes.
- [ ] Default visible area: 5 lines of text. Remaining lines accessible via scrolling.
- [ ] Mouse wheel scrolls the preview content up/down.
- [ ] Keyboard scrolling: Arrow Up/Down, Page Up/Down scroll the preview.
- [ ] A vertical scrollbar is rendered on the side, reflecting the current scroll position and total content size.
- [ ] Long lines wrap automatically within the preview area width. No horizontal scrolling.
- [ ] `safe_preview_line` is called lazily per visible line by the renderer (never eagerly for the entire text). C0 control characters (U+0000-U+001F, U+007F) are displayed as visible markers. Newlines are preserved as line breaks. Tab is a visible marker.
- [ ] Keyboard shortcuts: `y` = confirm paste, `n` = cancel, `Esc` = cancel.
- [ ] `Tab` toggles focus between the [Paste] and [Cancel] buttons. Focused button is visually distinct (e.g., brackets or underline).
- [ ] Default button focus on open remains Cancel (from the gate ticket). Tab can switch to Paste.
- [ ] Confirm sends the raw (unescaped) paste text to the PTY using current InputModes.
- [ ] Smoke test: paste text containing control characters (tab, ESC) and multiple lines. Verify preview shows visible markers, scrolling works, `y` confirms, `n`/`Esc` cancels.

## Blocked by

- #21 — Paste gate + basic confirmation dialog
