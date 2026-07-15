# Paste Newline Confirmation

## Problem Statement

When bracketed paste mode is OFF, pasting multi-line text into the terminal sends literal newline characters to the PTY. The shell interprets each newline as Enter, immediately executing each line. Users can accidentally execute dangerous commands by pasting a multi-line block without realizing it contains newlines.

## Solution

Before delivering paste text to the PTY, Harbor checks whether the text contains newlines and whether bracketed paste is enabled. If bracketed paste is OFF and the text contains meaningful newlines (more than one line after trimming trailing newlines), Harbor opens a confirmation dialog rendered via wgpu in a separate winit window. The dialog shows a scrollable preview of the paste content with control characters escaped. The user must explicitly confirm (click, `y`, or Tab+Enter on "Paste") before the text is sent to the PTY. If they cancel, the text is discarded.

When bracketed paste is ON, or the text contains at most one meaningful line, paste proceeds without interruption — preserving the existing behavior.

## User Stories

1. As a terminal user, I want Harbor to warn me when I paste multi-line text and bracketed paste is OFF, so that I don't accidentally execute commands in my shell.
2. As a terminal user, I want to see a preview of what I'm about to paste, so that I can verify the content before confirming.
3. As a terminal user, I want to scroll through the full paste preview, so that I can inspect long pastes before deciding.
4. As a terminal user, I want single-line pastes (or single line with trailing newline) to paste immediately without interruption, so that I'm not bothered by confirmations for trivial pastes.
5. As a terminal user, I want bracketed-paste-aware applications to receive multi-line pastes without interruption, so that shells and editors with bracketed paste support work normally.
6. As a terminal user, I want to confirm the paste by pressing `y`, so that I can use the keyboard without reaching for the mouse.
7. As a terminal user, I want to cancel the paste by pressing `n` or `Esc`, so that I can quickly dismiss the dialog.
8. As a terminal user, I want the default button focus to be "Cancel", so that accidentally pressing Enter doesn't execute the paste.
9. As a terminal user, I want to Tab between buttons and press Enter to activate the focused one, so that the dialog follows standard keyboard navigation.
10. As a terminal user, I want the main terminal to remain readable (PTY output continues rendering, scrollback navigable) while the dialog is open, so that I can check terminal state before deciding.
11. As a terminal user, I want the dialog to prevent keyboard input from reaching the PTY while open, so that I can't accidentally type commands while deciding.
12. As a terminal user, I want control characters in the preview to be displayed as visible markers, so that I can audit the actual content (including hidden bytes) before pasting.
13. As a terminal user, I want tabs to appear as visible markers in the preview, so that I can distinguish whitespace types.
14. As a terminal user, I want long lines in the preview to wrap automatically, so that I can read full content without horizontal scrolling.
15. As a terminal user, I want the dialog to appear centered over the main window, so that it's visually associated with Harbor.
16. As a terminal user, I want the dialog to inherit the terminal's font and color scheme, so that it feels visually consistent.
17. As a terminal user, I want pressing Ctrl+V again while the dialog is open to be ignored, so that I don't end up with multiple overlapping dialogs.
18. As a terminal user, I want pasting via any shortcut (Ctrl+V, Shift+Insert, future paste UI) to go through the same confirmation gate, so that no paste path can bypass the safety check.
19. As a terminal user, I want the dialog to use the latest InputModes for bracketed-paste framing at send time, so that mode changes during the dialog don't cause stale behavior.

## Implementation Decisions

### Multi-line detection

A paste is considered "multi-line" when, after recursively trimming all trailing newline sequences (`\r\n`, `\n`, `\r`) from the text, at least one newline character remains. Text that is empty after trimming is not multi-line.

### Confirmation gate

A pure decision function takes the pasted text and returns whether it is multi-line. The caller combines this with `InputModes` to produce a `PasteDisposition`: `SendDirect` (bracketed paste ON, or text is not multi-line) or `Confirm { raw_text }` (bracketed paste OFF and text is multi-line). This gate sits before `send_paste` and is shared by all paste entry points.

### Dialog architecture

- A new `PasteDialog` struct owns a secondary winit `Window` and a `wgpu::Surface` created from it.
- The dialog window reuses the main window's `wgpu::Device`, `Queue`, and font/glyph pipeline. No second GPU context is created.
- On Windows, the dialog is an owned window (`with_owner_window`), keeping it above the main window in z-order and minimizing/destroying together.
- The dialog window has no native title bar. Buttons and keyboard shortcuts provide the only close mechanisms.
- The dialog is fixed-size (600x400 px) and positioned centered over the main window. It does not move with main-window resize after creation.
- The dialog is not draggable.

### Event loop integration

`App` holds `Option<PasteDialog>`. `window_event` dispatches by `window_id`: events for the dialog go to `paste_dialog.handle_event()`. Main-window events are processed normally while a dialog is active, with the following exceptions suppressed:
- Keyboard events that would produce PTY input (character keys, Enter, Backspace, etc.).
- Mouse events that would initiate a new selection.
- New paste attempts (Ctrl+V, Shift+Insert, etc.).

Events that remain active: resize, redraw, PTY output processing, scrollback navigation (mouse wheel, PageUp/Down), and copy (Ctrl+C).

### Rendering

- Preview text is rendered using the terminal's glyph cache and font. Only visible rows (plus scroll buffer margin) are rendered — virtualized rendering, not all-at-once.
- Long lines wrap automatically within the preview area.
- Default visible area is 5 lines of text.
- Buttons are rendered as plain text labels, inheriting terminal font and colors.
- The entire dialog inherits the terminal's background color, foreground color, and font family/size.

### Preview escaping

Only control characters in the C0 range (U+0000-U+001F, plus U+007F DEL) are escaped to visible markers. The escaping is performed lazily per visible line by the dialog renderer — never eagerly for the entire text. Newline characters are preserved as line breaks in the preview display. Tab is rendered as a visible marker.

### Keyboard handling in dialog

- `y` — confirm paste (same as clicking the confirm button)
- `n` — cancel
- `Esc` — cancel
- `Enter` — activates the currently focused button
- `Tab` — toggles focus between the confirm and cancel buttons
- Default focus on open: the cancel button

### Dialog lifecycle

- **Opening**: clipboard text is obtained, multi-line detection runs, `InputModes` is checked. If disposition is `Confirm`, create `PasteDialog` with the raw text. Do not send to PTY.
- **While open**: main-window keyboard and mouse input is blocked from reaching the PTY. PTY output continues draining and rendering. Scroll-to-bottom snap on new output is NOT suppressed.
- **Confirm**: call `send_paste` with the current `InputModes` (which may have changed since dialog opened). Destroy dialog window. Resume main-window input immediately.
- **Cancel**: discard raw text. Destroy dialog window. Resume main-window input immediately. The clipboard is not re-read on next paste unless the user triggers it again.
- **Duplicate paste while dialog is open**: ignored (no-op).
- **Main-window resize while dialog is open**: dialog stays at its original position, does not re-center.

### Mode-change during dialog

The bracketed-paste mode may change between dialog-open and user-confirm. The decision to show the dialog is made once at paste time. If the mode becomes ON while the dialog is open, the dialog stays. At confirm time, the current `InputModes` is used to determine whether to wrap in bracketed-paste markers — which may differ from the mode that triggered the dialog.

## Testing Decisions

### What makes a good test

- Test external observable behavior: the multi-line detection rules, and the escaping rules for preview display.
- Do NOT test rendering, window creation, GPU state, or platform-specific owned-window behavior.
- Match existing test conventions: table-driven tests in the terminal module, focused assertions on pure logic.

### Unit test seams (pure functions)

**`should_confirm_multiline(text: &str) -> bool`**

Trims trailing newline sequences (`\r\n`, `\n`, `\r`) recursively, then returns `true` if any newline character remains. Test with:
- Empty string
- Single line, no trailing newline
- Single line with trailing `\n`
- Single line with trailing `\r\n`
- Single line with multiple trailing `\n\n\n`
- Two lines (one `\n` mid-text)
- Two lines with trailing `\n`
- Three or more lines
- Text with `\r\n` line endings (Windows clipboard)
- Text with mixed line endings
- Text that becomes empty after trimming all newlines

**`safe_preview_line(line: &str) -> String`**

Escapes C0 control characters (U+0000-U+001F, U+007F) in a single line to visible markers. Test with:
- Plain printable text (no-op)
- Tab character
- ESC character
- CR, LF (standalone within a line)
- Null byte
- DEL
- Multiple control characters in one line
- CJK text (no-op, non-C0)

### Integration test seam

**`PasteDisposition`** — the combined decision from `should_confirm_multiline` + `InputModes.bracketed_paste`:
- `InputModes { bracketed_paste: true }` + any text → `SendDirect`
- `InputModes { bracketed_paste: false }` + single-line text → `SendDirect`
- `InputModes { bracketed_paste: false }` + multi-line text → `Confirm`

## Out of Scope

- Configurable multi-line threshold (e.g., "warn after N lines"). The rule is fixed: any text with meaningful newlines triggers confirmation.
- Dangerous-command detection in preview. Control characters are escaped for visibility, but Harbor does not classify or highlight potentially dangerous shell commands.
- Dialog that follows main-window resize. The dialog is positioned once at creation and stays fixed.
- Draggable dialog. The dialog is fixed in position.
- Horizontal scrolling in preview. Long lines wrap automatically; no horizontal scroll.

## Further Notes

- The existing `InputModes::paste()` and `send_paste()` functions are unchanged. The confirmation gate is a new layer before them.
- On platforms other than Windows, owned-window behavior may not be available. The dialog should still function as an independent window, without the z-order guarantee.
- The dialog reuses the main window's glyph cache by holding a shared reference. The glyph cache must outlive any active dialog.
- This feature depends on the existing clipboard integration (Ctrl+C/Ctrl+V via `Selection::try_handle_keyboard`) and the `InputModes` struct tracking bracketed paste state (DECSET/DECRST `?2004`).
