# Multi-Session Tab Management and Binary Split Pane Architecture

**Status:** Proposed
**Date:** 2026-08-30

## Context

Multi-session (tab) management and arbitrary pane splitting require establishing data structures and UI boundaries across `harbor-widget` and terminal host layers. Alternatives included an N-ary flex layout tree, offscreen widget stacking for background tabs, and ad-hoc application-level split rendering.

## Decision

1. **Binary Split Tree**: Represent each session's pane layout as a recursive binary split tree (`PaneLayoutNode`) supporting horizontal and vertical divisions with fractional ratios. Leaf removal deterministically promotes the sibling node to replace the parent split.
2. **Active Session Mounting**: Mount only the currently active session's pane layout into the `harbor-widget` runtime view during reconciliation. Background `Terminal` instances maintain their internal blocking-read threads and virtual screen buffers per Spec 0002 (with keystroke writes executed synchronously upon routed input), but emit no widget draw registrations while inactive.
3. **Generic SplitContainer Widget**: Implement a reusable `SplitContainer` in `harbor-widget` responsible for sash layout, cursor changes, ratio clamping, and pointer capture drag handling, rather than ad-hoc application layout code.
4. **Dual-Axis Tab Bar**: Implement a configurable `TabBar` widget supporting `top`, `bottom`, `left`, and `right` placements with axis-adaptive layout flow and overflow scrolling.

## Consequences

- Space reallocation on pane closure is deterministic: removing a leaf lifts its sibling to replace the parent split.
- GPU draw call and widget reconciliation overhead is restricted to visible panes.
- Tab switching rebuilds the widget tree to bind the newly active session's pane draw IDs.
- Core UI runtime gains a reusable split-pane and tab navigation foundation without coupling to terminal-specific logic.
