# Bounded Forced Recovery

**Ticket ID:** T0003
**Source:** [Spec: 0007-synchronized-output-mode](../../spec/0007-synchronized-output-mode.md)
**Status:** Todo

## Goal

An unclosed synchronized batch remains visibly fresh because the Runtime force-presents the current terminal frame at least every 100 ms.

## Layers

- [ ] **VT Parser / Screen State:** Preserve the nonzero `?2026` nesting state and DECRQM Set status while recovery frames occur.
- [ ] **Terminal Session / FrameDemand:** Expose ongoing synchronization eligibility without adding a Terminal-owned window timer or changing parsing and damage behavior.
- [ ] **Terminal Widget Bridge / External Schedule:** Report the active recovery scheduling requirement through the existing generic provider while retaining draw-ID isolation.
- [ ] **Runtime Scheduler / Winit Presentation:** Own a recurring monotonic 100 ms deadline, request a force-compatible frame when due, reschedule after a successful attempt, and cancel on final disable.
- [ ] **Conformance Evidence:** Add deterministic Runtime/Scheduler and presenter tests for deadline registration, recurrence, cancellation, coalescing, and suspended/zero-sized surface behavior.

## Approach

1. Build on T0002's synchronized state and T0001's generic distinction between ordinary deferred and forced-compatible presentation.
2. Have the external schedule provider indicate that recovery remains necessary while synchronization is active; do not let Terminal acquire or present a surface.
3. Extend Runtime scheduling so its monotonic deadline coexists with cursor blink, autoscroll, redraw coalescing, and host deadlines.
4. At each due deadline, use the normal frame/presenter flow for a current terminal frame, then arm the next recovery deadline only while the mode remains enabled.
5. Cancel the recovery deadline immediately after final disable and ensure non-drawable surfaces do not create busy waiting or stale past deadlines.

## Blocked by

- T0001 — Uses the generic presentation-eligibility and forced-present contract.
- T0002 — Requires concrete synchronized state and ordinary batch suppression.

## Blocks

- T0004 — Final evidence and lifecycle behavior must include the completed recovery contract.

## Acceptance

- [ ] A dirty terminal with synchronization still enabled receives a normal-path forced presentation no later than each 100 ms recovery boundary.
- [ ] Recovery does not reset nesting state; DECRQM remains Set until the final matching disable or lifecycle cleanup.
- [ ] Each forced frame schedules the next recovery boundary only while synchronization remains active.
- [ ] Final disable cancels the recovery deadline and restores ordinary scheduling.
- [ ] Blink, autoscroll, redraw coalescing, host deadlines, and suspended surfaces retain their existing behavior.
- [ ] Deterministic scheduler and presenter tests pass without wall-clock sleeps.

## Out of Scope

- Configurable, adaptive, or user-visible recovery intervals.
- Lifecycle reset/session-close cleanup and protocol checklist updates; T0004.
- Renderer throughput optimization or direct ownership of platform resources by Terminal.
