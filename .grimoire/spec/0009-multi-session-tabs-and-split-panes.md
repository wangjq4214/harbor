# Multi-Session Tabs and Split Panes

**Spec ID:** 0009
**Status:** Draft
**Date:** 2026-08-30

## Requirement

The terminal application must support multiple concurrent terminal sessions displayed via an edge-configurable tab bar, with each session supporting arbitrary nested horizontal and vertical pane splitting.

## Solution

The solution partitions responsibility between `harbor-widget` (reusable UI containers) and `harbor-terminal` / Host application layer (session state, layout tree, and terminal lifecycle):

1. **`harbor-widget` Container Additions**:
   - `SplitContainer`: A two-child layout widget (`first`, `second`) configured with a `SplitDirection` (Horizontal | Vertical) and a fractional `ratio` (0.0..1.0). It renders a hit-testable and draggable `PaneDivider` (Sash) between children, updates the cursor on hover (`ew-resize` / `ns-resize`), and captures pointer events during dragging to report updated split ratios.
   - `TabBar`: A navigation widget displaying a list of tab items with an active index, supporting configurable positions (`top`, `bottom`, `left`, `right`) and scrollable overflow for excessive tabs.

2. **Session and Pane Layout Model**:
   - `PaneLayoutNode`: A recursive binary tree (`Leaf(PaneId)` or `Split { direction, ratio, first, second }`).
   - `TerminalSession`: Represents an independent workspace owning a `SessionId`, title/icon metadata, a `PaneLayoutNode`, and active `PaneId`.
   - `AppSessionState`: Owns the list of `TerminalSession`s, the active `SessionId`, and a map of `PaneId -> Terminal`.

3. **Rendering & Event Integration**:
   - Only the active `TerminalSession`'s layout tree is converted into a widget tree of nested `SplitContainer`s and `CustomPaint::new(pane_draw_id)` leaves during reconciliation.
   - Background sessions keep their `Terminal` internal reader threads running and virtual screen buffers updated per Spec 0002, but register no draw handlers with `harbor-widget`.
   - Keyboard and pointer events are routed by `harbor-widget` hit-testing to the focused `CustomPaint` / `PaneId`.

### Seams

| Seam | Connects | Expects | Provides |
|------|----------|---------|----------|
| Split Layout Widget | Host App ↔ `harbor-widget` | `SplitContainer::new(dir, ratio, first, second).on_resize(cb)` | Layout constraints, sash hit-testing, pointer-drag ratio updates |
| Tab Navigation Widget | Host App ↔ `harbor-widget` | `TabBar::new(items, active_idx, position).on_select(cb).on_close(cb)` | Dual-axis tab rendering, active styling, overflow scrolling, click/close callbacks |
| Pane Terminal CustomPaint | `Terminal` ↔ `harbor-widget` | `CustomPaint::new(pane_draw_id).handler(...).schedule(...)` | Isolated GPU render pass allocation and routed `UiEvent` input per pane |
| Session PTY I/O | Host App ↔ `Terminal` | `Terminal::new(pty_read, pty_write)` per pane | Autonomous background PTY byte reading and synchronous input writes |

## End-to-End Tests

### E2E: Create and switch sessions
- **Given:** The application is running with a single default session.
- **When:** The user clicks the "+" new session button on the tab bar.
- **Then:** A new tab is created and activated, a new `Terminal` instance is initialized with its own PTY, and the widget tree mounts the new session's single pane.

### E2E: Split active pane horizontally and vertically
- **Given:** An active session containing one full-screen pane with keyboard focus.
- **When:** The user triggers horizontal split (e.g. `Ctrl+Shift+D`), followed by vertical split (e.g. `Ctrl+Shift+Plus`) in the newly focused pane.
- **Then:** The layout tree becomes a nested binary split; each leaf pane allocates distinct screen space, renders its own terminal grid, and handles isolated keystrokes and pointer input.

### E2E: Resize split panes via divider drag
- **Given:** Two split panes separated by a vertical divider sash.
- **When:** The user hovers the divider (cursor becomes `ew-resize`), presses the pointer, drags 100 pixels right, and releases.
- **Then:** The split ratio updates proportionally, both child panes re-layout and re-allocate terminal grid dimensions, and pointer capture releases cleanly.

### E2E: Close pane and reclaim space
- **Given:** A session with two split panes (Ratio: 0.5/0.5).
- **When:** The user closes the focused pane (e.g. `Ctrl+Shift+W` or shell exit).
- **Then:** The closed pane's `Terminal` and PTY are torn down, its sibling node is promoted to replace the parent split, and the remaining pane expands to fill the entire space.

### E2E: Background tab continues executing
- **Given:** A background session running a long compilation command producing continuous PTY output.
- **When:** The user switches away to another session and later switches back.
- **Then:** No GPU draw calls were made for the background session while inactive, but all compiled output is present in the screen/scrollback buffer upon reactivation.

### E2E: Tab bar position reconfiguration
- **Given:** Tab bar positioned at the `top` with multiple open tabs.
- **When:** Configuration updates the position to `left`.
- **Then:** The tab bar transitions to a vertical sidebar layout with icons/titles and vertical scrolling without resetting active session states.

## Decisions

### Binary split tree over N-ary flex tree
- **Choice:** Recursive binary split tree (`PaneLayoutNode`).
- **Reason:** Provides deterministic, unambiguous space reclamation on leaf closure (sibling promotion) and isolated two-pane sash drag calculations without cascading weight re-normalization.
- **ADR reference:** [0025-binary-split-tree-and-active-session-mounting](../adr/0025-binary-split-tree-and-active-session-mounting.md)

### Active session mounting with background PTY continuity
- **Choice:** Mount only the active session's pane tree into `harbor-widget`. Background terminals run reader threads without GPU draw registrations.
- **Reason:** Guarantees zero unnecessary GPU rendering and layout diffing for invisible tabs while maintaining autonomous PTY stream consumption per Spec 0002. Keystroke writes remain synchronous on routed input.
- **ADR reference:** [0025-binary-split-tree-and-active-session-mounting](../adr/0025-binary-split-tree-and-active-session-mounting.md), [0012-consolidate-render-ui-into-terminal](../adr/0012-consolidate-render-ui-into-terminal.md), [0013-synchronous-pty-io](../adr/0013-synchronous-pty-io.md)

### Generic SplitContainer and TabBar in harbor-widget
- **Choice:** Implement `SplitContainer` and `TabBar` as reusable widgets in `harbor-widget`.
- **Reason:** Keeps sash hit-testing, pointer capture, dual-axis layout constraints, and visual styling encapsulated in the widget runtime, keeping the host application purely declarative.
- **ADR reference:** [0025-binary-split-tree-and-active-session-mounting](../adr/0025-binary-split-tree-and-active-session-mounting.md), [0001-widget-crate-separation](../adr/0001-widget-crate-separation.md)

### Multiple CustomPaint instances in single widget tree
- **Choice:** Each pane registers its own `ExternalDrawId` and `CustomPaint` component.
- **Reason:** Aligns with ADR-0005 and ADR-0011 where Runtime dispatches external draw and input handlers by ID, allowing arbitrary numbers of concurrent panes to coexist.
- **ADR reference:** [0005-custom-paint-provider-by-id](../adr/0005-custom-paint-provider-by-id.md), [0011-terminal-custompaint-gpu-injection](../adr/0011-terminal-custompaint-gpu-injection.md)

## Test Plan

- **Integration tests:**
  - `harbor-widget::widgets::split_container`: BoxConstraints splitting calculation (horizontal and vertical), divider hit-testing, ratio clamping (preventing zero-size collapse), pointer drag event handling and capture.
  - `harbor-widget::widgets::tab_bar`: Layout in `top`, `bottom`, `left`, `right` orientations; selection change event firing; close button event bubbling; overflow scroll offset behavior.
  - Host session layout tree: Binary tree split insertion (horizontal/vertical on focused leaf), sibling promotion on leaf removal, focus traversal between adjacent panes.
  - Cross-pane input isolation: Dispatching keyboard events to focused `CustomPaint` verifies only the targeted pane receives input.
- **Manual tests:**
  - Launch application, open 5+ tabs, drag tabs if supported, switch positions in config.
  - Split current window into 4 quadrants; run `top` or animated script in one pane while typing in another; drag sashes to resize.
  - Close inner panes in different tree depths; verify layout collapses cleanly.
- **Performance thresholds:**
  - Tab switch latency: Widget tree reconciliation and external draw rebinding complete in a single `Runtime::update` pass without auxiliary heap reallocation for inactive terminal buffers.
  - Sash drag re-layout: Pointer motion events execute a single layout pass and clamp without triggering cascading tree rebuilds.
  - Inactive session cost: Inactive sessions generate zero GPU draw calls, zero widget layout passes, and zero frame invalidations while their PTYs remain idle.
- **Edge cases:**
  - Window resized below minimum pane size: `SplitContainer` clamps child bounds to a minimum allocation (at least 1 cell width/height or configurable minimum size) to prevent zero-width or negative layout constraints.
  - Closing the last pane in a session: Closes the owning `TerminalSession` and activates the nearest adjacent tab.
  - Closing the last session: Closes the window or re-initializes a clean default single-pane session per application exit policy.
  - Concurrent PTY output across multiple split panes: Independent terminal screen updates and dirty-row damage tracking execute without cross-pane lock contention.

## Out of Scope

- Detachable floating windows (moving a tab/pane to a separate OS window).
- Persistence of session/layout state across application restarts.
- Arbitrary free-form floating panes over the terminal grid.

## Future Evolution

- Tab dragging and reordering via pointer drag-and-drop within the `TabBar`.
- Session serialization and restore (saving workspace splits to a configuration file).
- Tab grouping and coloring for workspace organization.
