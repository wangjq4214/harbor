# DecoratedBox Fill and Border

**Ticket ID:** T0002
**Source:** [Spec: 0009-widget-decoration-and-terminal-chrome](../../spec/0009-widget-decoration-and-terminal-chrome.md)
**Status:** In Progress

## Goal

A caller can wrap one normal Widget in `DecoratedBox` and observe an optional rounded background behind it and a uniform rounded border above it without changing the child's layout.

## Layers

- [ ] **Widget API and Layout:** Add the single-child `DecoratedBox` builder, consume `BoxDecoration`, preserve child constraints and measured size, and emit no fill when color is absent.
- [ ] **Fiber and Retained Scene:** Materialize background before child content and border after child content with stable SceneItem identities and paint order.
- [ ] **Widget Renderer and Frame Encoder:** Encode optional color, uniform border, and normalized four-corner radii in logical pixels at the current DPI.
- [ ] **Runtime Host and Terminal Bridge:** None — this slice is independently demonstrated with a non-terminal child and must not alter the application root.
- [ ] **Verification:** Add widget, retained-scene, paint-order, renderer-contract, optional-decoration, and layout-invariance tests.

## Approach

1. Implement `DecoratedBox` as a one-child Component following existing fluent widget conventions.
2. Preserve the child's incoming constraints and return the child's size, including empty and zero-size cases.
3. Generate pre-child background and post-child border output from `BoxDecoration`, skipping absent or fully transparent effects.
4. Extend scene/quad data only as needed to carry normalized per-corner radii and a uniform border through retained updates.
5. Update the independent instanced widget pipeline so fill and border render correctly at fractional coordinates and DPI scales.
6. Demonstrate the complete path with a simple child whose background, child pixels, and border ordering are distinguishable.

## Blocked by

- T0001 — Supplies decoration values, validation, normalized radii, paint phases, and clip metadata contracts.

## Blocks

- T0003 — Outer shadows extend this concrete `DecoratedBox` scene and renderer path.

## Acceptance

- [ ] `DecoratedBox::new(decoration).child(child)` exposes exactly one staged child and replaces a previously staged child consistently with existing one-child widgets.
- [ ] Adding or removing decoration does not change the child's measured size or allocation.
- [ ] No-color decoration emits no opaque background, preserving transparent surfaces.
- [ ] Uniform and independent corner radii render correctly for both fill and border.
- [ ] Observable paint order is background → child → border.
- [ ] Unwrapped widgets and existing scalar-radius Button rendering remain visually and behaviorally compatible.
- [ ] Retained updates modify only changed decoration SceneItems rather than rebuilding unrelated content.

## Out of Scope

- Shadow rendering.
- Child clipping or rounded hit testing.
- `Primitive::External` clipping.
- Per-edge borders, gradients, images, or decoration-driven padding.
- Applying decoration to Terminal.
