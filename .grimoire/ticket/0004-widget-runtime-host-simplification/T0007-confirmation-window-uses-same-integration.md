# Confirmation Window Uses the Same Integration

**Ticket ID:** T0007
**Source:** [Spec: 0004-widget-runtime-host-simplification](../../spec/0004-widget-runtime-host-simplification.md)
**Status:** Todo

## Goal

The native paste-confirmation window independently receives events and presents frames through the shared runtime integration while existing cross-window paste safety remains observable.

## Layers

- [ ] **Runtime Host:** Retain confirmation Window/Surface lifetime, WindowId routing, cross-window gate, confirmed PTY write, cancellation, and fatal-error handling while replacing per-window event/render glue.
- [ ] **Winit Runtime Integration:** Give the confirmation window its own `WinitAdapter`, Runtime, scheduler state, borrowed frame target, viewport handling, and surface recovery path.
- [ ] **Core Widget Runtime:** Preserve strict per-window Widget tree, focus, input, scene, and invalidation isolation while sharing permitted Device, Queue, and text resources.
- [ ] **Terminal / Application Components:** Keep confirmation UI/state as application Widget components and terminal paste encoding/writes as terminal/App policy rather than generic Runtime behavior.
- [ ] **Verification:** Exercise confirm, cancel, close, focus traversal, input gating, independent redraw, DPI, resize, and surface recovery across both windows.

## Approach

1. Replace confirmation-local translation and frame code with one adapter/Runtime pair using the contracts proven by the main window.
2. Route by WindowId in App, then let the selected integration process generic input and redraw; do not share adapter input state between windows.
3. Borrow the shared Device/Queue and the confirmation Surface/Window only for each frame, and reuse the common viewport and recovery policy.
4. Keep App-level keyboard/new-paste gating while the confirmation exists; allow PTY output, rendering, copying, and permitted scrollback behavior.
5. Keep confirmation outcomes as application commands: cancel closes without writing, confirm reads current terminal InputModes and writes unchanged raw content.
6. Remove duplicate event conversion, scheduler, encoder, submit, present, and recovery code from `src/app/confirmation.rs` after cross-window tests pass.

## Blocked by

- T0001 — provides all shared per-window contracts.
- T0006 — provides the complete event, scheduling, presentation, viewport, and recovery integration.

## Blocks

- (none)

## Acceptance

- [ ] Main and confirmation windows each have independent adapter, Runtime, focus, scheduler, and surface state.
- [ ] Both windows may share Device, Queue, and text resources without sharing Widget/input state.
- [ ] Confirmation click, keyboard, IME, focus traversal, resize, DPI, redraw, and recovery behavior works through runtime integration.
- [ ] While confirmation is open, main-window terminal keyboard input and new paste requests are blocked, while allowed output/render/scroll/copy behavior continues.
- [ ] Confirm writes unchanged raw content using current InputModes; cancel or close writes nothing.
- [ ] App still owns both window/surface lifetimes and cross-window policy.
- [ ] `src/app/confirmation.rs` contains no duplicate generic event conversion or GPU frame orchestration.

## Out of Scope

- Replacing the separate native confirmation window with an overlay.
- Moving paste policy or PTY ownership into `harbor-widget`.
- Building a general-purpose multi-window manager.
- Adding new confirmation UI features unrelated to integration parity.
