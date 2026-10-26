# Return-Driven Event Consumption and Unconditional Commit

**Status:** Testing
**Date:** 2026-10-25
**Sources:** historical UI/API review U2, F11 and F12; refinement conversation accepting return-value-controlled propagation and removal of public `stop_propagation()` after comparing Harbor, Flutter and Web event models.
**Related:** [ADR 0009](./0009-app-cross-window-input-gate.md), [ADR 0029](./0029-widget-store-state-action-boundary.md).

## Context

Harbor currently exposes two apparent consumption protocols: `EventHandled` is documented as indicating consumption, but the router discards handler return values and checks only `EventCtx::stop_propagation()`. Built-in controls commonly use both, while an external `PrimitiveLeaf` or `CustomPaint::on_input` handler returning `Handled` alone does not prevent enclosing controls from activating. F12 separately identifies a hover-redraw short circuit that skips the current Move event's command submission.

The discussion compared three models: explicit propagation control separate from default-action cancellation in Web DOM, raw-pointer observation plus gesture arbitration in Flutter, and return-driven consumption. Harbor currently derives control activation directly from routed input rather than a separate gesture arena. The selected direction makes the existing return type meaningful without introducing a browser-style default-action system or a gesture-recognition redesign. Retaining public `stop_propagation()` merely to support callbacks without return values was considered and rejected: observation callbacks and activation notifications need not independently control the underlying input's propagation.

This record preserves the selected architecture, not completed implementation, regression results, or authorization to begin implementation.

## Decision

1. **Use one public propagation-control protocol.** `EventHandled::Handled` means the current event is consumed: after the handler finishes, the router stops delivery to remaining handlers/nodes/phases of that event walk. `Ignored` permits continued propagation; it does not assert that the handler produced no side effects. Apply the contract consistently to built-in and public extension handlers, including `PrimitiveLeaf` and `CustomPaint::on_input`.
2. **Remove public `EventCtx::stop_propagation()`.** Callers express consumption through the handler's return value, not a second mutable flag. Keep EventCtx's deferred side-effect requests, including focus, pointer capture/release, paint invalidation and clipboard operations. Internal routing representation is not prescribed by this decision.
3. **Keep observation and activation callbacks distinct from input decisions.** A callback such as `on_click` reports activation; the owning control decides whether it consumed the underlying input. Boundary-observation callbacks such as `on_enter`/`on_exit` do not cancel other nodes' required boundary notifications. Do not mechanically add consumption returns to all callbacks. If an existing callback genuinely needs to decide propagation, identify that requirement and give its input-processing contract an explicit result rather than reintroducing hidden context-based control.
4. **Keep consumption local to its dispatch.** A Handled result during capture deliberately intercepts later delivery, including the target; ordinary enclosing activation controls must still allow descendants first refusal. Consumption does not undo already executed capture handlers, cancel separately delivered hover/focus notifications, or automatically consume later Up, Cancel or IME events. Preserve the established pointer, key-sequence and IME ownership rules rather than deriving them from a single event's result.
5. **Commit accumulated side effects independently of consumption and redraw.** A stopped event walk still submits its accumulated commands; consumption is not rollback. Hover requiring redraw must not skip the current Move event's submission. Preserve established command ordering, effect coalescing and exactly-once processing; do not replay lost work on a later input event. A redraw indication is not a consumption result.
6. **Keep U2 bounded.** Fix the existing consumption and commit contracts rather than introduce Flutter-style gesture arbitration, a Web-style default-action subsystem, or unrelated API renaming. Runtime-to-host effect delivery and the application's cross-window input gate and action-drain responsibilities remain in place.

## Consequences

- Extension authors can consume an event by returning Handled without knowing a second stopping convention. An Ignored result cannot contradict an explicit public stop flag because that API is removed.
- This is a breaking public-API change: existing stop calls and affected callback expectations must be audited and migrated, together with public extension documentation and examples.
- U2 must preserve nested child-first activation, deliberate capture interception, independent boundary/focus notification delivery, pointer/key/IME lifecycle behavior, and accumulated effect submission. The F11 nested-consumption and F12 hover-plus-Move scenarios remain required regression cases; passing unrelated tests cannot close them.
- The selected requirements contract is [spec 0039](../spec/0039-return-driven-event-consumption-and-commit.md). This record does not establish implementation, native/IME/HMR acceptance, or a ticket set.
