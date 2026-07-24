# Native Cancellation Window

**Ticket ID:** T0003
**Source:** [Spec: 0001-phase-3-widget-runtime-integration](../../spec/0001-phase-3-widget-runtime-integration.md)
**Status:** Todo

## Goal

A confirmable paste opens a separate owned winit window rendered by Widget Runtime, and cancelling it safely sends no PTY bytes.

## Layers

- [ ] **`harbor-text`:** Supply metrics, glyph data, and cached runs for Widget title, explanatory, and Cancel text.
- [ ] **`harbor-widget`:** Implement the minimum Text primitive/batch and a confirmation root with a focusable Cancel control.
- [ ] **`harbor-render`:** None — terminal output remains the existing provider-owned renderer from T0002.
- [ ] **App host:** Replace legacy `PasteDialog` construction with a secondary surface and Runtime; implement ADR 0009's App-level gate for terminal keyboard input and new paste requests while it exists.
- [ ] **Verification:** Add event-routing integration coverage and manually verify native ownership/centering, cancel, and continued terminal output/scrollback/copying.

## Approach

1. Add Widget label text rendering backed by the shared CPU text core without importing terminal renderer types.
2. Build and render a small confirmation root in a separately owned winit window and surface.
3. Route only that window's events to its Runtime and map Cancel click, Escape, `n`, and default-focused Enter to an App cancellation intent.
4. Gate main-window terminal keyboard input and duplicate paste requests in App while the secondary window exists, as ADR 0009 requires.
5. Drop secondary window resources on cancel and request any necessary main-window redraw.

## Blocked by

- T0001 — Requires shared text data for Widget labels.
- T0002 — Reuses the established App/Runtime lifecycle and provider event boundary.

## Blocks

- T0004 — Confirmation requires this window lifecycle, cancellation route, and App gate.

## Acceptance

- [ ] A valid confirmable paste opens a separate owned native window rendered by Widget Runtime rather than legacy `harbor-ui`.
- [ ] Cancel click, Escape, `n`, and Enter with default Cancel focus close the window and send no PTY bytes.
- [ ] While open, the App blocks terminal keyboard input and new paste requests but continues terminal output, redraw, scrollback, and copying.
- [ ] The secondary window has an independently acquired/presented surface frame while sharing the App Device and Queue.

## Out of Scope

- Confirm/Paste actions and current-`InputModes` send semantics.
- Preview content, wrapping, scrolling, and full focus traversal.
- A main-window overlay or a platform-native alert dialog.
