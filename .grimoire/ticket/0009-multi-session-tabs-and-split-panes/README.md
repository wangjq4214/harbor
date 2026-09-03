# Multi-Session Tabs and Split Panes

**Source:** [Spec: 0009-multi-session-tabs-and-split-panes.md](../../spec/0009-multi-session-tabs-and-split-panes.md)
**Ticket folder:** `.grimoire/ticket/0009-multi-session-tabs-and-split-panes/`

## Overview

These tickets implement multi-session tab management and arbitrary nested binary pane splitting for the Harbor terminal emulator. Responsibility is cleanly partitioned between `harbor-widget` (reusable `SplitContainer` and `TabBar` layout containers) and the Host application layer (`PaneLayoutNode` binary tree manipulation, session lifecycle, and active-session view reconciliation). The decomposition ensures that data models and widget foundations are established first, followed by interactive split containers, binary layout tree space management, and multi-session tab navigation with background PTY continuity.

## Layers

The project's architectural layers (confirmed during decomposition):

1. **Widget Containers & Layout** — `SplitContainer` and `TabBar` layout widgets, constraint splitting, sash hit-testing, ratio clamping, and pointer capture.
2. **Session State & Layout Tree** — `PaneLayoutNode` binary tree, split insertion, sibling promotion on pane removal, `TerminalSession`, and focus tracking.
3. **Terminal Bridge & Rendering** — `pane_draw_id` allocation, `CustomPaint` draw registrations, and active-session view reconciliation.
4. **PTY Lifecycle & Event Routing** — Autonomous background PTY blocking-read threads, synchronous input writes, and keyboard shortcut routing.
5. **Verification & Conformance** — Unit, integration, layout constraint, and E2E behavioral tests across all layers.

Every ticket cuts through all confirmed layers.

## Dependency Graph

### Blocking relationships

| Ticket | Blocks | Reason |
| --- | --- | --- |
| T0001 | T0002, T0003, T0004 | Foundation data structures, type definitions, and widget scaffolding are prerequisites for all slices. |
| T0002 | T0003 | The binary pane layout tree renders directly onto the interactive `SplitContainer` widget. |
| T0003 | T0004 | Multi-session switching requires mounting and unmounting fully operational pane layout trees. |

### Parallel groups

None. T0002 builds the core split container, T0003 integrates it into terminal pane layout trees, and T0004 integrates multi-session tab bar navigation across the complete application view hierarchy.

## Recommended Order

1. T0001 — Multi-Session and Split Foundation (pre-refactoring)
2. T0002 — SplitContainer Widget and Sash Interaction
3. T0003 — Pane Binary Split Tree and Space Reclamation
4. T0004 — Dual-Axis TabBar Navigation and Session Switching

## Ticket Index

| Ticket ID | File | Title | Summary |
| --- | --- | --- | --- |
| T0001 | [T0001-multi-session-and-split-foundation.md](./T0001-multi-session-and-split-foundation.md) | Multi-Session and Split Foundation | Shared data models, type definitions, and widget contract scaffolding. |
| T0002 | [T0002-split-container-widget-and-sash-interaction.md](./T0002-split-container-widget-and-sash-interaction.md) | SplitContainer Widget and Sash Interaction | Two-child split layout widget with sash hit-testing, pointer capture, and ratio clamping. |
| T0003 | [T0003-pane-binary-split-tree-and-space-reclamation.md](./T0003-pane-binary-split-tree-and-space-reclamation.md) | Pane Binary Split Tree and Space Reclamation | Recursive binary layout tree, split insertion, sibling promotion on close, and multi-pane CustomPaint integration. |
| T0004 | [T0004-dual-axis-tab-bar-navigation-and-session-switching.md](./T0004-dual-axis-tab-bar-navigation-and-session-switching.md) | Dual-Axis TabBar Navigation and Session Switching | Four-way configurable TabBar with overflow scrolling, session switching, and background PTY continuity. |
