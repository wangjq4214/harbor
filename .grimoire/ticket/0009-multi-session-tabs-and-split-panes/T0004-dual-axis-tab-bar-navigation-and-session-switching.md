# Dual-Axis TabBar Navigation and Session Switching

**Ticket ID:** T0004
**Source:** [Spec: 0009-multi-session-tabs-and-split-panes](../../spec/0009-multi-session-tabs-and-split-panes.md)
**Status:** Todo

## Goal

Implement the four-way configurable `TabBar` widget with overflow scrolling, manage multi-session application state, and mount only the active session's pane layout into the widget tree while preserving background PTY execution.

## Layers

- [ ] **Widget Containers & Layout:** Implement `TabBar` in `harbor-widget::widgets::tab_bar` supporting `TabBarPosition` (`Top`, `Bottom`, `Left`, `Right`), horizontal/vertical flex layout, tab items with active indicator, close buttons, "+" add button, and scrollable overflow.
- [ ] **Session State & Layout Tree:** Implement `AppSessionState` managing `Vec<TerminalSession>`, `active_session: SessionId`, session creation (`new_session()`), session deletion (`close_session(id)`), and tab switching (`switch_session(id)`).
- [ ] **Terminal Bridge & Rendering:** Reconcile only the active session's pane tree into `harbor-widget`'s root view. Ensure inactive sessions emit zero `CustomPaint` draw registrations and consume zero GPU draw calls.
- [ ] **PTY Lifecycle & Event Routing:** Ensure background sessions retain their autonomous PTY reader threads and update screen buffers without interference. Route global session shortcuts (`Ctrl+Shift+T` for new tab, `Ctrl+Tab` / `Ctrl+Shift+Tab` for switching, `Ctrl+Shift+W` on single-pane session for closing tab).
- [ ] **Verification & Conformance:** Unit tests for `TabBar` layout across 4 orientations, session switching state consistency, and integration tests confirming background PTY output persistence across tab switches.

## Approach

1. Implement `TabBar` widget in `harbor-widget::widgets::tab_bar`:
   - Accept `position: TabBarPosition`, `tabs: Vec<TabItem>`, `active_idx: usize`, `on_select: Arc<dyn Fn(usize)>`, `on_close: Arc<dyn Fn(usize)>`, `on_new: Arc<dyn Fn()>`.
   - In `Top`/`Bottom` positions, lay out horizontally in a `Row` with horizontal scroll capability.
   - In `Left`/`Right` positions, lay out vertically in a `Column` (sidebar style) with vertical scroll capability.
   - Render tab title, optional icon, active highlight quad/border, and close "x" button.
2. Implement application root component:
   - Compose `TabBar` and the active session's `PaneLayoutNode` widget tree in a `Column` (for Top/Bottom tabs) or `Row` (for Left/Right tabs).
3. Connect session management actions:
   - "New Tab" / `Ctrl+Shift+T`: spawn a new default single-pane `TerminalSession`, push to sessions list, and set as active.
   - "Close Tab" / `Ctrl+Shift+W` (when last pane closes): remove session, activate adjacent session.
   - Tab click / `Ctrl+Tab`: update `active_session`, triggering view tree rebuild and binding the new session's pane draw handlers.
4. Verify PTY background continuity:
   - Run a test feeding PTY output to an inactive session's `Terminal`, switch back, and assert the output is rendered on the next frame encode.
5. Add configuration setting for `tab_bar_position` and test dynamic layout reconfiguration.

## Blocked by

- T0001 — Multi-Session and Split Foundation (requires `SessionId`, `TerminalSession`, `TabBarPosition`)
- T0003 — Pane Binary Split Tree and Space Reclamation (requires pane layout tree building and teardown)

## Blocks

(none — closes the feature)

## Acceptance

- [ ] TabBar displays open sessions at `top`, `bottom`, `left`, or right edge according to configuration.
- [ ] Clicking a tab or pressing `Ctrl+Tab` switches the active session immediately, rendering its pane layout.
- [ ] Clicking "+" adds a new session tab and activates it with a fresh PTY.
- [ ] Clicking "x" on a tab closes the session and tears down all its panes and PTYs.
- [ ] Long compilation / continuous PTY output in a background tab runs uninterrupted and appears immediately when returning to that tab.
- [ ] Overflowing tabs scroll smoothly without breaking layout bounds.

## Out of Scope

- Tab dragging and reordering via drag-and-drop.
- Session persistence to disk across app restarts.
- Tab grouping or color badges.
