# Pane Binary Split Tree and Space Reclamation

**Ticket ID:** T0003
**Source:** [Spec: 0009-multi-session-tabs-and-split-panes](../../spec/0009-multi-session-tabs-and-split-panes.md)
**Status:** Todo

## Goal

Implement recursive binary split tree transformations (split insertion, focus navigation, sibling promotion space reclamation) and convert `PaneLayoutNode` into a nested `SplitContainer` / `CustomPaint` widget tree.

## Layers

- [ ] **Widget Containers & Layout:** Recursively transform `PaneLayoutNode` into nested `SplitContainer` instances with `CustomPaint::new(pane_draw_id)` leaves.
- [ ] **Session State & Layout Tree:** Implement in-place split insertion (`split_pane(focused_id, direction)`), pane removal with sibling promotion (`close_pane(target_id)`), and directional/cycling focus movement.
- [ ] **Terminal Bridge & Rendering:** Assign distinct `ExternalDrawId`s to each spawned `Terminal` pane; ensure each pane's `CustomPaint` handler and schedule callbacks are registered independently.
- [ ] **PTY Lifecycle & Event Routing:** Route shortcut keys (`Ctrl+Shift+D` for horizontal split, `Ctrl+Shift+Plus` for vertical split, `Ctrl+Shift+W` for closing pane) and handle PTY teardown when a pane is removed.
- [ ] **Verification & Conformance:** Unit tests for binary tree insertion, sibling promotion on removal, and integration tests verifying multi-pane `CustomPaint` rendering in the widget tree.

## Approach

1. Implement tree transformation algorithms on `PaneLayoutNode`:
   - `split_leaf(target_id, new_id, direction)`: Replaces `Leaf(target_id)` with `Split { direction, ratio: 0.5, first: Leaf(target_id), second: Leaf(new_id) }`.
   - `remove_leaf(target_id)`: Recursively locates parent `Split` node; replaces parent `Split` with the remaining sibling node, returning the removed `PaneId` for teardown.
2. Implement recursive widget builder: `build_pane_tree(node: &PaneLayoutNode, terminals: &HashMap<PaneId, Terminal>) -> View` producing nested `SplitContainer` widgets wrapping `CustomPaint::new(terminal.draw_id())`.
3. Wire terminal shortcuts in Host input processing:
   - On split shortcut: create new `Terminal` with PTY endpoints, allocate `PaneId`, insert into layout tree, and set focus to new pane.
   - On close shortcut / PTY EOF: remove pane from layout tree, tear down `Terminal`, and focus adjacent sibling pane.
4. Write unit tests covering arbitrary depth split insertions and deletions, verifying layout tree balance and sibling promotion correctness.
5. Write integration test mounting 4 quadrants of `CustomPaint` instances in a single `Runtime`, asserting distinct draw calls and isolated event handling.

## Blocked by

- T0001 — Multi-Session and Split Foundation (requires `PaneLayoutNode` and `PaneId`)
- T0002 — SplitContainer Widget and Sash Interaction (requires `SplitContainer`)

## Blocks

- T0004 — Dual-Axis TabBar Navigation and Session Switching (requires functioning pane layout tree within a session)

## Acceptance

- [ ] Triggering horizontal or vertical split on an active pane cleanly splits its allocated area in half and initializes a new terminal pane.
- [ ] Closing a pane tears down its PTY and lifts its sibling node to reclaim the entire parent partition.
- [ ] Pointer click on any pane transfers terminal focus and keyboard input immediately to that pane.
- [ ] Multiple split panes render simultaneously with active visual border/indicator on the focused pane.
- [ ] Sibling promotion correctly handles edge cases (e.g. closing the root split leaves a single leaf pane).

## Out of Scope

- Multi-session tab navigation bar and switching (covered in T0004).
- Cross-tab pane drag-and-drop.
