# Render through TerminalWidgetBridge

**Ticket ID:** T0002
**Source:** [Spec: 0005-terminal-widget-boundary-migration](../../spec/0005-terminal-widget-boundary-migration.md)
**Status:** Todo

## Goal

The App renders a running terminal through `TerminalWidgetBridge: Component` with the same widget allocation, clipping, paint order, and resize behavior as before.

## Layers

- [ ] **Terminal engine (`harbor-terminal`):** Migrate render entry points and viewport construction from `ExternalDrawContext`/`ExternalDrawId` to `RenderTarget`; remove terminal ownership of the widget draw identifier from the render path.
- [ ] **Widget external-paint (`harbor-widget`):** No framework source change — consume the existing `CustomPaint` external-draw registration and callback contract through the bridge.
- [ ] **Runtime Host / bridge (`src/`):** Add `TerminalWidgetBridge: Component`, make it own the widget-facing draw identifier and external draw handler, convert external draw geometry to `RenderTarget`, and replace direct App `CustomPaint` glue with the component.
- [ ] **Verification:** Test geometry adaptation and bridge handler registration; verify terminal rendering and resize behavior through the component path.

## Approach

1. Refactor the terminal viewport construction to derive from `RenderTarget`, avoiding a second geometry model.
2. Change `Terminal::render` to accept only terminal-owned render data plus the existing frame-scoped render-pass/GPU access; eliminate render-time widget ID checks from terminal.
3. Implement `TerminalWidgetBridge` in a root `src/` module as a `Component` that composes `CustomPaint`, owns `ExternalDrawId`, and captures the App-owned terminal through the existing safe shared ownership pattern.
4. Convert `ExternalDrawContext` into `RenderTarget` at the bridge boundary and preserve the current frame-scoped GPU lookup in the external draw callback.
5. Replace the direct terminal draw-handler/`CustomPaint` setup in `src/app.rs` with the bridge Component, retaining App terminal lifecycle ownership.

## Blocked by

- T0001 — supplies the widget-free `RenderTarget` contract.

## Blocks

- T0003 — input routing must address this component's bridge-owned external draw identifier.

## Acceptance

- [ ] A terminal placed in the main Runtime renders through `TerminalWidgetBridge` and `CustomPaint` without a direct terminal reference to `ExternalDrawContext` or `ExternalDrawId`.
- [ ] Resize, clipping, widget paint order, and zero/empty allocation handling remain behaviorally equivalent to the existing terminal path.
- [ ] The bridge registers its external draw handler through existing `CustomPaint` behavior; no `AnyView` visibility or widget framework API is widened.
- [ ] Focused bridge/geometry tests and the existing rendering-related tests pass.

## Out of Scope

- Mapping widget input to `TerminalEvent` or changing `Terminal::handle_event`.
- Removing the terminal's remaining input-driven `harbor-widget` dependency.
- Cross-window gate changes, scheduler/deadline behavior, or a reusable bridge crate.
