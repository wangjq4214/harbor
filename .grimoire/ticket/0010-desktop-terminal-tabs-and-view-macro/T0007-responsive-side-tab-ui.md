# Responsive Side Tab Product UI

**Ticket ID:** T0007
**Source:** [Spec: 0010-desktop-terminal-tabs-and-view-macro](../../spec/0010-desktop-terminal-tabs-and-view-macro.md)
**Status:** In Progress

## Goal

The Harbor main window uses a macro-authored, keyboard-accessible vertical tab rail with expanded/compact desktop presentation and one active expanded terminal.

## Layers

- [ ] **Widget Composition:** Compose Flex, bounded rail, Separator, Expanded terminal, ScrollArea, interaction/focus/theme primitives, and explicit keyed TabItems.
- [ ] **Declarative Macro:** Author the product tree with `view!(cx, expression => { children })`, including the dynamic keyed tab loop.
- [ ] **Runtime Host:** Bind TabManager actions, active bridge, unread/selected state, focus restoration, and paste gate.
- [ ] **Renderer:** Reuse existing primitives and terminal CustomPaint; only active terminal paints.
- [ ] **Verification:** Compare expanded/compact geometry, mouse/keyboard operation, focus, dynamic close/insert, scrolling, decoration, and Acrylic.

## Approach

1. Keep TabRail and TabItem application-level compositions until reuse proves generic widget contracts.
2. Use 200dp expanded rail at window widths >= 900dp and 56dp compact rail below that threshold.
3. Show deterministic labels in expanded mode and icon/abbreviation plus accessible tooltip/title behavior in compact mode.
4. Keep close and add actions keyboard reachable; avoid nested interactive controls with ambiguous event targeting.
5. Attach `.keyed(TabId)` explicitly to dynamic tab items.
6. Mount the active TerminalWidgetBridge once under Expanded and preserve existing decoration/root inset/Acrylic composition.

## Blocked by

- T0001 — Flex allocation.
- T0002 — Keyed dynamic children.
- T0003 — View macro.
- T0004 — Interaction, focus, commands, and theme.
- T0005 — Rail scrolling and allocation observation.
- T0006 — Tab/session model.

## Blocks

- T0008 — Final acceptance validates the integrated UI.

## Acceptance

- [ ] The rail displays all tabs, selected/unread/focus states, new action, and close actions with theme-driven visuals.
- [ ] Mouse and keyboard can create, activate, traverse, and close tabs.
- [ ] The product tree is authored with `view!`, and an equivalence fixture protects macro versus handwritten semantics.
- [ ] Dynamic close/insert preserves the intended keyed TabItem state and focus.
- [ ] Many tabs scroll vertically and focused items are ensured visible.
- [ ] Width changes cross the 900dp breakpoint without a mobile drawer/bottom bar or invalid terminal geometry.
- [ ] Existing terminal decoration, clipping, selection, scrollbar, cursor, and Acrylic behavior remain intact.

## Out of Scope

- Moving TabRail/TabItem into the generic crate, animations, drag reorder, custom user styling, image icons, mobile navigation, and cross-window tabs.
