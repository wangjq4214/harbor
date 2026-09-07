# Multi-Terminal Tab Session Model

**Ticket ID:** T0006
**Source:** [Spec: 0010-desktop-terminal-tabs-and-view-macro](../../spec/0010-desktop-terminal-tabs-and-view-macro.md)
**Status:** Todo

## Goal

The Runtime Host owns multiple independent terminal/PTY sessions with stable TabIds, distinct external draw IDs, tab-qualified output events, and deterministic create/activate/close behavior.

## Layers

- [ ] **Widget API:** No tab-domain types enter `harbor-widget`; allow TerminalWidgetBridge construction with an explicit stable ExternalDrawId.
- [ ] **Terminal/PTY:** Each TerminalTab owns one independent Terminal and PTY lifecycle and can receive broadcast resize safely.
- [ ] **Runtime Host:** Add `TabId`, `TerminalTab`, `TabManager`, tab-qualified AppEvent routing, active selection, unread state, and final-tab close outcome.
- [ ] **Runtime Registration:** Mount/schedule only the active terminal bridge and ignore stale events for closed IDs.
- [ ] **Verification:** Model transitions, independent I/O/state, explicit IDs, cleanup, stale events, focus target selection, and inactive scheduling.

## Approach

1. Extract single-terminal creation from Shell bootstrap into a reusable per-tab factory using current configuration and shared GPU/font resources.
2. Allocate monotonic session-lifetime TabIds and distinct ExternalDrawIds.
3. Keep bridge callbacks with TerminalTab lifetime; do not recreate them on every parent build.
4. Replace `AppEvent::TerminalOutputReady` with `TerminalOutputReady(TabId)` and route active/inactive wakes explicitly.
5. Implement add, activate, close, next/previous, numeric activation, and right-then-left close selection policy.
6. Return a Host outcome that closes the main window when the final tab closes.

## Blocked by

- (none)

## Blocks

- T0007 — Product UI binds to TabManager.
- T0008 — Scheduling, cleanup, resize, and lifecycle acceptance consume the model.

## Acceptance

- [ ] Two tabs run independent shells and preserve independent screen, scrollback, selection, cursor, and PTY state across switches.
- [ ] IDs are stable and never reused during one application session; every bridge has a distinct ExternalDrawId.
- [ ] Input and paste target only the active tab.
- [ ] Background events identify their tab, stale closed-tab events are ignored, and inactive output can mark unread state without hidden terminal paint.
- [ ] Closing B in A/B/C activates C; closing C in A/C activates A.
- [ ] Closing the final tab shuts down its resources and closes the main window.
- [ ] Closing a tab releases reader/PTY control, Terminal ownership, external draw registration, and schedule provider without leaks or stale callbacks.

## Out of Scope

- Persistence, restore, dynamic OSC titles, profiles, reorder, pin/group, split panes, detach/cross-window tabs, and multiple main windows.
