# Terminal Owns Scrollback Input Semantics

**Ticket ID:** T0003
**Source:** [Spec: 0004-widget-runtime-host-simplification](../../spec/0004-widget-runtime-host-simplification.md)
**Status:** Todo

## Goal

PageUp, PageDown, Home, End, and mouse-wheel input scroll terminal history exactly as before without `src/app.rs` understanding terminal navigation semantics.

## Layers

- [ ] **Runtime Host:** Remove `scrollback_navigation` and direct wheel mutation branches and only enforce the existing cross-window gate before generic dispatch.
- [ ] **Winit Runtime Integration:** Preserve wheel unit and magnitude information during adaptation without embedding terminal-specific scaling or scroll policy.
- [ ] **Core Widget Runtime:** Carry platform-independent scroll delta semantics through pointer routing and deliver the event to the targeted CustomPaint node.
- [ ] **Terminal / Application Components:** Interpret navigation keys, modifier/alternate-screen rules, line versus pixel wheel deltas, viewport bounds, and live-view restoration inside `harbor-terminal`.
- [ ] **Verification:** Test routed navigation from adapter input through Runtime to terminal screen/scrollback state, including ignored modifier and alternate-screen cases.

## Approach

1. Audit the existing `UiEvent` wheel representation; extend it with a platform-independent line/pixel distinction if required to preserve current behavior without winit types.
2. Route all navigation keys and wheel events through the same adapter and Runtime path as other Widget input.
3. Move PageUp/PageDown/Home/End policy and wheel-to-scroll conversion into terminal input handling near screen/scrollback ownership.
4. Preserve the current rules for alternate screen, modifiers, line multiplier, pixel conversion, bounds, and terminal-bound input returning to live output.
5. Remove duplicate direct terminal mutations from `src/app.rs` once terminal-level tests and routed integration tests pass.

## Blocked by

- T0001 — provides shared event/effect contracts.
- T0002 — establishes the adapter-to-Runtime input path.

## Blocks

- T0004 — sequencing avoids overlapping `src/app.rs` event-routing edits.

## Acceptance

- [ ] PageUp/PageDown move by the expected viewport amount and Home/End move to history bounds.
- [ ] Wheel line and pixel deltas preserve existing observable scroll distances.
- [ ] Disallowed modifier combinations and alternate-screen mode retain existing behavior.
- [ ] Input is routed by Widget hit testing/focus before terminal interpretation.
- [ ] `src/app.rs` contains no terminal scrollback key mapping or direct wheel scroll calculation.
- [ ] `harbor-widget` contains no terminal-specific scrollback policy.

## Out of Scope

- Changing scrollback capacity or screen storage.
- Adding smooth scrolling or kinetic animation.
- Moving the cross-window gate out of App.
- Runtime frame scheduling and presentation.
