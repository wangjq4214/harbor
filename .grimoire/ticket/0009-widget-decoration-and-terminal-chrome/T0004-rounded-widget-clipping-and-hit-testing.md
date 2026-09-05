# Rounded Widget Clipping and Hit Testing

**Ticket ID:** T0004
**Source:** [Spec: 0009-widget-decoration-and-terminal-chrome](../../spec/0009-widget-decoration-and-terminal-chrome.md)
**Status:** Todo

## Goal

A normal child subtree wrapped by `DecoratedBox` obeys `HardEdge` or `AntiAlias` rounded painting bounds and rejects pointer targets outside the same rounded shape.

## Layers

- [ ] **Widget API and Layout:** Apply `ClipBehavior` independently from `BoxDecoration`; preserve layout for `None`, `HardEdge`, and `AntiAlias`.
- [ ] **Fiber and Retained Scene:** Propagate nested rounded child clips through scene collection and use the same normalized shape during reverse-paint-order hit testing.
- [ ] **Widget Renderer and Frame Encoder:** Intersect ancestor and local clips and encode normal widget primitives with hard-edge or anti-aliased rounded boundaries.
- [ ] **Runtime Host and Terminal Bridge:** None — this slice deliberately proves normal Widget behavior before extending clipping across the external-draw boundary.
- [ ] **Verification:** Add nested clip, boundary hit, paint/hit consistency, DPI, oversized-radius, and ancestor-intersection integration tests.

## Approach

1. Make `DecoratedBox` establish a child-only rounded clip after painting its own shadow/background and before painting its post-child border.
2. Carry a normalized clip stack or equivalent retained representation instead of collapsing rounded clips into the existing rectangular scissor.
3. Apply `None` as a zero-cost compatibility path, `HardEdge` as a sharp rounded boundary, and `AntiAlias` as a softened rounded boundary without save-layer semantics.
4. Intersect nested and ancestor clips deterministically, preserving the rule that a child cannot escape any ancestor clip.
5. Extend event routing/hit testing so points inside the allocation rectangle but outside an active rounded child shape do not target descendants.
6. Verify exact-boundary behavior uses one consistent inclusion rule for paint and input.

## Blocked by

- T0001 — Supplies `ClipBehavior`, normalized radii, and retained clip contracts.
- T0003 — Stabilizes the shared decoration, scene, and renderer files before clipping extends them.

## Blocks

- T0005 — `CustomPaint` clipping consumes the rounded clip representation and semantics proven here.

## Acceptance

- [ ] `ClipBehavior::None` preserves existing child painting and rectangular hit testing.
- [ ] `HardEdge` removes normal child pixels outside the rounded shape without soft edge coverage.
- [ ] `AntiAlias` produces a visually smooth rounded edge without introducing `AntiAliasWithSaveLayer` behavior.
- [ ] Pointer events inside the allocation rectangle but outside the rounded shape do not target the clipped child subtree.
- [ ] Pointer events inside the rounded shape preserve existing focus, capture, and routing behavior.
- [ ] Nested rounded and rectangular ancestor clips intersect correctly at fractional DPI scales.
- [ ] A DecoratedBox's own shadow and border remain outside the child clip where specified.

## Out of Scope

- `Primitive::External` and Terminal clipping; covered by T0005.
- Arbitrary path clips, transformed non-axis-aligned clips, or save layers.
- Shadow rendering changes.
- Host root composition.
