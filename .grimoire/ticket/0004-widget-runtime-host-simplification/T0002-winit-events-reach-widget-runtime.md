# Winit Events Reach Widget Runtime

**Ticket ID:** T0002
**Source:** [Spec: 0004-widget-runtime-host-simplification](../../spec/0004-widget-runtime-host-simplification.md)
**Status:** Todo

## Goal

Keyboard, IME, pointer, focus, and modifier input reaches main-window widgets through the public winit integration with no event translation in `src/`.

## Layers

- [ ] **Runtime Host:** Route eligible main-window events to one per-window `WinitAdapter`, apply returned effects, retain close and cross-window policy, and remove `src/app/translate.rs` plus App-owned modifier/IME conversion state.
- [ ] **Winit Runtime Integration:** Own modifier, pointer-position, scale, and IME composition state and convert supported winit events into platform-independent `UiEvent` values.
- [ ] **Core Widget Runtime:** Dispatch adapted events through existing hit testing and capture-target-bubble routing and expose resulting invalidation as RuntimeEffects.
- [ ] **Terminal / Application Components:** Preserve the existing CustomPaint external-input path so focused terminal input still reaches `Terminal::handle_event`; confirmation migration remains deferred.
- [ ] **Verification:** Relocate and expand translation tests under `harbor-widget`, then exercise the main-window route from winit event to terminal/widget effect.

## Approach

1. Move named-key, modifier, pointer, and stateful IME conversion into the feature-gated integration module.
2. Preserve IME de-duplication, numpad distinctions, logical-coordinate scaling, pointer phases, and unsupported-event behavior.
3. Make adapter state explicitly per window so future confirmation integration cannot share composition or modifier state accidentally.
4. Route translated events into Runtime and merge dispatch invalidation with adapter-produced effects.
5. Keep `CloseRequested`, window creation/destruction, and the App cross-window input gate in Host policy.
6. Replace main-window calls to `winit_to_uievent_with_ime` with the integration API and delete the old translation module after parity tests pass.

## Blocked by

- T0001 — provides the feature boundary, adapter, effects, and core integration hooks.

## Blocks

- T0003 — terminal scroll input must use this generic event path.

## Acceptance

- [ ] Keyboard press/release, modifiers, numpad keys, IME enable/disable/commit, cursor movement, mouse buttons, wheel payloads, and focus events have parity tests in `harbor-widget`.
- [ ] IME character keys are not emitted twice during active composition.
- [ ] Main-window Widget controls and focused terminal CustomPaint receive routed input through Runtime.
- [ ] Unsupported winit events are reported as unhandled rather than producing invalid `UiEvent` values.
- [ ] `src/app/translate.rs` and App-owned `ImeState`/modifier conversion logic no longer exist.
- [ ] Close requests and cross-window gating remain App-owned.

## Out of Scope

- Moving terminal-specific scrollback interpretation; covered by T0003.
- Replacing FrameScheduler; covered by T0004.
- GPU frame acquisition or presentation.
- Migrating the confirmation window.
