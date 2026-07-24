# Preview and DPI Completion

**Ticket ID:** T0005
**Source:** [Spec: 0001-phase-3-widget-runtime-integration](../../spec/0001-phase-3-widget-runtime-integration.md)
**Status:** Todo

## Goal

The functional native confirmation window shows an escaped, wrapped, read-only scrollable preview with complete keyboard focus behavior and correct layout at every scale factor.

## Layers

- [ ] **`harbor-text`:** Provide cached metrics and runs for preview wrapping and visible-line rendering.
- [ ] **`harbor-widget`:** Build the confirmation-specific preview layout, bounded vertical scroll state, wheel/arrow/PageUp/PageDown handling, Tab / Shift+Tab traversal, and visible-line text batching.
- [ ] **`harbor-render`:** None — terminal rendering remains unchanged after T0002; this slice does not add terminal GPU behavior.
- [ ] **App host:** Forward scale-factor and resize changes for both Runtime viewports and retain the App gate/lifecycle established by prior tickets.
- [ ] **Verification:** Add preview unit/integration tests and manually validate both windows across DPI and resize changes.

## Approach

1. Reuse `safe_preview_line` only for display, preserving raw content for later send and visibly escaping controls including `\r`.
2. Implement automatic wrapping against the preview viewport and retain only confirmation-specific read-only scroll state; do not introduce general ScrollView APIs.
3. Render only visible wrapped lines and clamp every scrolling route to valid bounds.
4. Ensure Cancel starts focused, Tab / Shift+Tab moves within the confirmation controls, and Enter activates the focused control.
5. Send each window's current logical size, physical size, and scale factor to its Runtime after resize or DPI changes.

## Blocked by

- T0001 — Requires shared text metrics and caches.
- T0004 — Extends the working confirmation lifecycle and decision routes.

## Blocks

- None.

## Acceptance

- [ ] Preview visibly escapes control characters, wraps to available width, and retains raw content unchanged for confirmation.
- [ ] Wheel, arrows, PageUp, and PageDown scroll only within the preview's valid range.
- [ ] Cancel is initial focus; Tab / Shift+Tab and Enter follow expected focused-control behavior.
- [ ] Main and confirmation Runtime layout, text placement, and hit testing remain aligned after resize and DPI changes.
- [ ] Rendering an idle confirmation window requests no redraw, and preview rendering emits text only for visible lines.

## Out of Scope

- A reusable general-purpose scroll widget.
- Rich text, shaping, selection/caret editing, images, or non-rectangular clipping.
- Shared GPU glyph textures between renderer adapters.
