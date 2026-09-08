# Desktop Interaction, Focus, Commands, and Theme

**Ticket ID:** T0004
**Source:** [Spec: 0010-desktop-terminal-tabs-and-view-macro](../../spec/0010-desktop-terminal-tabs-and-view-macro.md)
**Status:** Done

## Goal

Reusable widget behavior supports selected tab items, close/new controls, visible keyboard focus, desktop pointer states, shortcuts, actions, and theme-driven visuals.

## Layers

- [x] **Widget API:** Add `InteractiveRegion`, `MouseRegion`, node-level `Focus`, `Shortcuts`, typed `Actions`, inherited Theme tokens, and minimal IconButton composition.
- [x] **Fiber/Input:** Preserve interaction/focus state, route commands after event walks, and restore focus safely after active content changes.
- [x] **Renderer:** Reuse current primitives; selected/hovered/focused states resolve to themed colors and borders.
- [x] **Runtime Host:** Expose callbacks/actions without embedding TabManager or terminal types in `harbor-widget`.
- [x] **Verification:** Cover hover/press/cancel/capture, disabled/selected/focus-visible, traversal, shortcut precedence, and idle redraw.

## Approach

1. Extract the current Button interaction state machine into a reusable desktop behavior primitive.
2. Separate raw pointer enter/exit/cursor behavior from semantic click activation.
3. Add explicit focusability/order and command routing without introducing a mobile gesture arena.
4. Map key chords to typed actions; application callbacks execute tab commands.
5. Add inherited semantic Theme tokens and refactor Button away from hard-coded colors/sizes where needed by TabItem.
6. Compose icon-like controls from existing text/quad primitives until a real icon renderer is justified.

## Blocked by

- (none)

## Blocks

- T0007 — Product TabItems and keyboard commands consume these behaviors.

## Acceptance

- [x] InteractiveRegion defines deterministic normal, hovered, pressed, cancelled, focused, focus-visible, disabled, and selected transitions.
- [x] Pointer capture is released on up, cancel, focus loss, unmount, and window lifecycle cancellation.
- [x] Tab/Shift+Tab traversal and explicit focus restoration remain inside the intended scope.
- [x] `Ctrl+T`, `Ctrl+W`, `Ctrl+Tab`, `Ctrl+Shift+Tab`, and `Ctrl+1..9` map to typed actions with tested conflict/propagation policy.
- [x] Theme changes invalidate dependent visuals without hard-coded product colors in generic controls.
- [x] Quiet controls introduce no continuous redraw or Poll scheduling.

## Out of Scope

- General gesture recognition, touch-first interactions, accessibility bridge, animation/ripple effects, image/path icons, and global application command frameworks.
