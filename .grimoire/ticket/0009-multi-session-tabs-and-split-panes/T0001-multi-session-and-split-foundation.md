# Multi-Session and Split Foundation

**Ticket ID:** T0001
**Source:** [Spec: 0009-multi-session-tabs-and-split-panes](../../spec/0009-multi-session-tabs-and-split-panes.md)
**Status:** Todo

## Goal

Establish the core data models, types, and widget signatures for multi-session tabs and binary split panes across the workspace crates.

## Layers

- [ ] **Widget Containers & Layout:** Define `SplitDirection` enum (`Horizontal`, `Vertical`), `TabBarPosition` enum (`Top`, `Bottom`, `Left`, `Right`), and placeholder signatures for `SplitContainer` and `TabBar` in `harbor-widget`.
- [ ] **Session State & Layout Tree:** Define `SessionId`, `PaneId`, `PaneLayoutNode` (`Leaf(PaneId)` and `Split { direction, ratio, first, second }`), and `TerminalSession` in `harbor-types` or Host models.
- [ ] **Terminal Bridge & Rendering:** Define unique `ExternalDrawId` allocation helpers for dynamic multi-pane instances.
- [ ] **PTY Lifecycle & Event Routing:** Define keyboard shortcut action enums for split manipulation (`SplitHorizontal`, `SplitVertical`, `ClosePane`, `NewSession`, `CloseSession`).
- [ ] **Verification & Conformance:** Unit tests verifying binary tree serialization, equality, and default state construction.

## Approach

1. Add `SplitDirection` and `TabBarPosition` enums with serialization derives to `harbor-types`.
2. Implement `PaneLayoutNode` with helper methods (`leaf(id)`, `split(dir, ratio, first, second)`, `find_leaf(id)`, `collect_panes()`).
3. Define `TerminalSession` structure holding `id: SessionId`, `title: String`, `layout: PaneLayoutNode`, `active_pane: PaneId`.
4. Define stub `SplitContainer` and `TabBar` widget constructors in `harbor-widget::widgets`.
5. Write unit tests for layout tree construction and node traversal.

## Blocked by

(none — pre-refactoring foundation)

## Blocks

- T0002 — SplitContainer Widget and Sash Interaction (consumes `SplitDirection` and widget skeleton)
- T0003 — Pane Binary Split Tree and Space Reclamation (consumes `PaneLayoutNode`, `PaneId`, and draw ID allocator)
- T0004 — Dual-Axis TabBar Navigation and Session Switching (consumes `TerminalSession`, `SessionId`, and `TabBarPosition`)

## Acceptance

- [ ] `PaneLayoutNode` accurately models single-pane leaves and nested binary split trees.
- [ ] Layout tree helper methods can traverse, find, and enumerate all leaf `PaneId`s.
- [ ] `SplitDirection` and `TabBarPosition` enums compile cleanly in `harbor-widget` and `harbor-types`.
- [ ] Unit tests pass verifying tree hierarchy construction and leaf lookups.

## Out of Scope

- Runtime Sash dragging or pointer capture logic (covered in T0002).
- Dynamic tree insertion and sibling promotion algorithms (covered in T0003).
- TabBar rendering, scrolling, or tab switching (covered in T0004).
