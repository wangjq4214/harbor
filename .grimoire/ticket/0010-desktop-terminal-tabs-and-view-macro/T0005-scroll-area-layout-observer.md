# Scroll Area and Post-Layout Observation

**Ticket ID:** T0005
**Source:** [Spec: 0010-desktop-terminal-tabs-and-view-macro](../../spec/0010-desktop-terminal-tabs-and-view-macro.md)
**Status:** Todo

## Goal

A vertical tab rail can scroll overflowing items and report the final terminal content allocation after layout without recursive invalidation.

## Layers

- [ ] **Widget API/Layout:** Add vertical `ScrollArea`, controller/metrics, clipping, ensure-visible, and a generic post-layout allocation observer/effect.
- [ ] **Fiber/Input:** Retain scroll offset, route wheel/keyboard events, coalesce unchanged geometry, and queue observer delivery after layout.
- [ ] **Renderer:** Clip child paint/hit testing to the viewport using existing clip contracts.
- [ ] **Runtime Host:** Receive generic logical allocation changes; TerminalSize conversion remains application-owned.
- [ ] **Verification:** Cover scroll bounds, event bubbling, focus ensure-visible, resize, zero extent, observer coalescing, and no layout loops.

## Approach

1. Separate scroll offset/model from viewport widget configuration.
2. Support wheel line/pixel deltas and keyboard navigation with clamped extents.
3. Ensure focused TabItems become visible without moving an already-visible item.
4. Bubble unconsumed boundary scroll where appropriate.
5. Queue changed layout rectangles for post-layout delivery; never invoke state-changing callbacks recursively from measurement.
6. Keep observer payload generic (`Rect`/logical allocation), leaving terminal rows/columns outside `harbor-widget`.

## Blocked by

- T0001 — Requires correct final parent-directed allocation geometry.

## Blocks

- T0007 — Tab rail overflow consumes ScrollArea.
- T0008 — All-tab resize broadcast consumes post-layout allocation feedback.

## Acceptance

- [ ] Vertical content scrolls and clips correctly for wheel line/pixel and keyboard input.
- [ ] Scroll offset clamps after child-count or viewport changes.
- [ ] Focus traversal calls ensure-visible for offscreen tab items.
- [ ] Boundary scrolling follows a documented handled/bubbled policy.
- [ ] Allocation observers fire once per distinct final Rect after layout and never cause recursive layout/build loops.
- [ ] Zero-size/minimized geometry emits no invalid callback or continuous invalidation.
- [ ] Idle ScrollArea and observer state request no redraw.

## Out of Scope

- Draggable Scrollbar, kinetic/touch scrolling, overscroll effects, virtual lists, two-dimensional scrolling, and Terminal-specific payloads.
