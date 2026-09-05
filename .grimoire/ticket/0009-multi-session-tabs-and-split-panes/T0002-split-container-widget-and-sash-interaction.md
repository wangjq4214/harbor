# SplitContainer Widget and Sash Interaction

**Ticket ID:** T0002
**Source:** [Spec: 0009-multi-session-tabs-and-split-panes](../../spec/0009-multi-session-tabs-and-split-panes.md)
**Status:** Todo

## Goal

Implement the generic two-child `SplitContainer` widget in `harbor-widget` with interactive sash divider dragging, ratio clamping, and pointer capture.

## Layers

- [ ] **Widget Containers & Layout:** Implement `SplitContainer` in `harbor-widget::widgets::split_container`, implementing `Component` and `AnyView`. Support horizontal and vertical BoxConstraints splitting, sash divider quad painting, hit-testing, and dynamic ratio adjustment.
- [ ] **Session State & Layout Tree:** Connect `SplitContainer` ratio changes to an `on_resize(Arc<dyn Fn(f32)>)` callback for parent state updates.
- [ ] **Terminal Bridge & Rendering:** None — `SplitContainer` lays out arbitrary child `View`s without coupling to terminal internals.
- [ ] **PTY Lifecycle & Event Routing:** Route pointer events (`PointerDown`, `PointerMove`, `PointerUp`) to sash interaction, using `EventCtx::capture_pointer` during drag and requesting cursor changes (`ew-resize` / `ns-resize`).
- [ ] **Verification & Conformance:** Unit and integration tests for layout sizing, minimum constraint clamping, sash hover/hit-testing, and drag event sequences.

## Approach

1. Create `harbor-widget/src/widgets/split_container.rs` defining `SplitContainer` with fields `direction: SplitDirection`, `ratio: f32`, `first: View`, `second: View`, `sash_thickness: f32`, `min_pane_size: f32`, `on_resize: Option<Arc<dyn Fn(f32) + Send + Sync>>`.
2. Implement `AnyView::intrinsic_size` and `AnyView::paint_primitives` to calculate child bounds based on `ratio` and `sash_thickness`, clamping child sizes so neither child violates `min_pane_size`.
3. Implement `AnyView::handle_event` to detect pointer interaction in the sash region:
   - On hover: trigger resize cursor via runtime effects.
   - On `PointerDown`: capture pointer via `EventCtx::capture_pointer`.
   - On `PointerMove` while captured: calculate delta, compute new ratio clamped to `[min_ratio, max_ratio]`, invoke `on_resize` callback, and request redraw.
   - On `PointerUp`: release pointer capture.
4. Export `SplitContainer` in `harbor-widget::widgets`.
5. Write integration tests in `harbor-widget` verifying constraint distribution, sash hit testing, and drag calculations.

## Blocked by

- T0001 — Multi-Session and Split Foundation (requires `SplitDirection`)

## Blocks

- T0003 — Pane Binary Split Tree and Space Reclamation (requires working `SplitContainer`)

## Acceptance

- [ ] `SplitContainer` correctly divides parent constraints between two children along horizontal or vertical axes.
- [ ] Child sizes are clamped to `min_pane_size` (preventing zero-size or negative dimensions).
- [ ] Sash divider renders between children with configurable thickness and color.
- [ ] Hovering the sash requests appropriate resize cursor.
- [ ] Dragging the sash smoothly updates split ratio and captures pointer across the window.
- [ ] Releasing the pointer terminates dragging and releases pointer capture.

## Out of Scope

- Multi-pane recursive layout tree mapping (covered in T0003).
- Keyboard shortcuts for splitting/closing (covered in T0003).
- TabBar integration (covered in T0004).
