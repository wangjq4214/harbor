# Parent-Directed Flex Layout

**Ticket ID:** T0001
**Source:** [Spec: 0010-desktop-terminal-tabs-and-view-macro](../../spec/0010-desktop-terminal-tabs-and-view-macro.md)
**Status:** Todo

## Goal

`harbor-widget` can correctly allocate a bounded fixed-width side rail and give the remaining width to terminal content under resize and DPI changes.

## Layers

- [ ] **Widget API and Layout:** Add parent-directed per-child constraints, bounded remeasurement, typed flex parent data, `Flex`, `Flexible`, `Expanded`, `Spacer`, `ConstrainedBox`, and `Separator`.
- [ ] **Fiber and Retained Scene:** Retain final child geometry and dirty propagation without changing paint identity for layout-only updates.
- [ ] **Widget Renderer:** No new primitive; existing quads/text/external draws consume corrected geometry.
- [ ] **Runtime Host:** No tab behavior yet; existing main and confirmation roots remain compatible.
- [ ] **Verification:** Cover loose/tight/unbounded/zero constraints, nested flex, gaps, overflow, remeasurement, resize, and fractional scale.

## Approach

1. Replace the one-constraint-for-all-children assumption with a parent-controlled child measurement interface.
2. Keep layout work finite and deterministic; reject or bound cyclic/infinite remeasurement.
3. Represent flex factor and fit as typed parent data rather than runtime `Any` values.
4. Measure inflexible children first, subtract gaps, distribute finite remaining space, then measure flexible children using tight or loose fit.
5. Keep `Row` and `Column` as thin facades over one Flex implementation.
6. Implement `ConstrainedBox` and a compositional `Separator` without introducing mobile layout policy.

## Blocked by

- (none)

## Blocks

- T0005 — Post-layout observation consumes final allocation geometry.
- T0007 — Product composition requires fixed rail plus expanded terminal behavior.

## Acceptance

- [ ] A horizontal Flex with a 200dp constrained child, 1dp separator, and Expanded child allocates exactly the remaining finite width.
- [ ] Compact 56dp and expanded 200dp rail cases remain valid at zero, minimum, loose, tight, and oversized constraints.
- [ ] Flex factors, loose/tight fit, gap, main alignment, cross alignment, and overflow diagnostics have deterministic CPU tests.
- [ ] No layout path produces NaN, infinity, negative size, or a hit rectangle outside its documented allocation.
- [ ] Viewport changes mark the required layout work without rebuilding unaffected GPU resources.
- [ ] Existing `Padding`, `Align`, `Stack`, `DecoratedBox`, `CustomPaint`, main window, and confirmation window tests remain green.

## Out of Scope

- Full Flutter Flex compatibility, baseline alignment, intrinsic multi-pass optimization, Wrap, Grid, Table, LayoutBuilder, and mobile breakpoints.
