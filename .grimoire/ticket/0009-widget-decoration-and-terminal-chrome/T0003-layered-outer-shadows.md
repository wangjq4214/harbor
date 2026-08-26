# Layered Outer Shadows

**Ticket ID:** T0003
**Source:** [Spec: 0009-widget-decoration-and-terminal-chrome](../../spec/0009-widget-decoration-and-terminal-chrome.md)
**Status:** Todo

## Goal

A decorated Widget displays one or more ordered outer shadows with configured color, offset, blur, and spread while its allocation and idle scheduling remain unchanged.

## Layers

- [ ] **Widget API and Layout:** Consume ordered `BoxShadow` values from `BoxDecoration` without adding padding or changing intrinsic/layout size.
- [ ] **Fiber and Retained Scene:** Emit stable pre-background shadow SceneItems in list order, first shadow at the bottom, with bounds intersected only by ancestor clips.
- [ ] **Widget Renderer and Frame Encoder:** Render outer-shadow geometry and soft alpha falloff for offset, blur, spread, per-corner radius, and transparent colors.
- [ ] **Runtime Host and Terminal Bridge:** None — this slice is demonstrated on a generic decorated Widget and introduces no Host scheduling or Terminal dependency.
- [ ] **Verification:** Add ordering, geometry, degenerate-value, retained-update, DPI, transparency, and performance tests plus a GPU/visual contract fixture.

## Approach

1. Extend `DecoratedBox` pre-child output to emit one shadow item per non-degenerate `BoxShadow` before the optional background.
2. Derive shadow geometry from the decorated box, finite spread, offset, blur extent, and normalized corner radii without changing layout bounds.
3. Extend the independent widget renderer with a bounded outer-shadow representation and alpha falloff compatible with existing alpha blending.
4. Preserve list ordering across scene updates and flush boundaries so overlapping shadows compose deterministically.
5. Skip zero-alpha and collapsed shadow bounds; ensure zero blur produces a valid hard-edged outer shadow rather than invalid GPU values.
6. Measure steady-state behavior to ensure unchanged shadows reuse retained data and do not request animation deadlines.

## Blocked by

- T0001 — Defines `BoxShadow`, validation, geometry, and shared retained contracts.
- T0002 — Provides the concrete `DecoratedBox` scene and renderer path that shadows extend.

## Blocks

- T0004 — Rounded clipping follows stabilization of the same decoration and renderer contracts.
- T0006 — The Terminal product preset requires working outer shadows.

## Acceptance

- [ ] A single shadow visibly respects color alpha, logical-pixel offset, blur radius, spread radius, and the decorated box's four corner radii.
- [ ] Multiple shadows paint in list order from back to front with the first item at the bottom.
- [ ] Shadows extend beyond the Widget's layout bounds without changing measured size.
- [ ] Ancestor clips can truncate shadow pixels, while the Widget's future child clip contract does not clip its own shadows.
- [ ] Zero-alpha or collapsed negative-spread shadows emit no spurious pixels; zero blur remains valid.
- [ ] Unchanged steady-state decoration creates no redraw deadline and no allocation proportional to terminal cell count.
- [ ] Existing widget and renderer tests remain green.

## Out of Scope

- Inner/inset shadows.
- Child clipping and rounded hit testing.
- Arbitrary filters, blend modes, save layers, or general blur APIs.
- Terminal preset composition.
