# Presentation Eligibility Contract

**Ticket ID:** T0001
**Source:** [Spec: 0007-synchronized-output-mode](../../spec/0007-synchronized-output-mode.md)
**Status:** Todo

## Goal

Establish a generic Terminal-to-Runtime presentation-eligibility contract that later slices can use without making `harbor-widget` depend on Terminal types.

## Layers

- [ ] **VT Parser / Screen State:** None — this foundation does not add CSI dispatch or mode state transitions.
- [ ] **Terminal Session / FrameDemand:** Extend the host-neutral `FrameDemand` vocabulary to expose presentation eligibility separately from redraw and deadline demand.
- [ ] **Terminal Widget Bridge / External Schedule:** Translate the new terminal demand into the widget-neutral external scheduling contract in `src/terminal_widget_bridge.rs`.
- [ ] **Runtime Scheduler / Winit Presentation:** Extend the generic demand/scheduler/presenter boundary so a provider can defer an ordinary present without acquiring or exposing terminal resources.
- [ ] **Conformance Evidence:** Add focused demand, bridge, scheduler, and presenter contract tests proving the new state is preserved and remains terminal-independent.

## Approach

1. Define a minimal, host-neutral representation of ordinary-present eligibility on the existing terminal scheduling boundary; do not encode `?2026` or other protocol names into `harbor-widget`.
2. Mirror only that generic information through `ExternalScheduleDemand`, preserving the existing redraw/deadline aggregation behavior.
3. Wire `TerminalWidgetBridge::schedule_demand_for_terminal` to translate the contract one-to-one for its owned external draw ID.
4. Teach Runtime scheduling and frame execution to distinguish an ordinary deferred present from a normal or explicitly forced present, while preserving borrowed `WinitFrameTarget` ownership.
5. Cover default, translated, deferred, and forced-compatible demand values with deterministic unit and integration-contract tests.

## Blocked by

- None — pre-refactoring foundation.

## Blocks

- T0002 — Basic synchronized batching needs a concrete state to publish through this contract.
- T0003 — Recovery needs the contract to request forced frames while ordinary presentation is deferred.
- T0004 — Lifecycle cleanup needs cleared state to restore the contract's ordinary-present behavior.

## Acceptance

- [ ] `FrameDemand` can represent ordinary presentation eligibility without coupling to Widget or winit types.
- [ ] `ExternalScheduleDemand` carries the equivalent generic information without depending on `harbor-terminal`.
- [ ] The bridge faithfully maps matching Terminal demand and ignores an unmatched draw ID.
- [ ] Runtime scheduling/presentation can honor a deferred ordinary present without changing Window, Surface, Device, Queue, or PTY ownership.
- [ ] Deterministic contract tests pass for default, deferred, and force-compatible paths.

## Out of Scope

- Parsing `CSI ?2026h/l`, nesting counters, or DECRQM replies.
- Any 100 ms deadline or forced-recovery policy.
- RIS, DECSTR, PTY/session-close semantics, and protocol checklist updates.
