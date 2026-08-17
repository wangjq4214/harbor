# Basic Synchronized Batch

**Ticket ID:** T0002
**Source:** [Spec: 0007-synchronized-output-mode](../../spec/0007-synchronized-output-mode.md)
**Status:** Todo

## Goal

A terminal application can wrap output in nested `?2026` boundaries and users see only the completed batch after its final matching disable.

## Layers

- [ ] **VT Parser / Screen State:** Route `CSI ?2026h/l` through existing private-mode handling, maintain a saturating session-owned nesting counter, and report Set/Reset through DECRQM.
- [ ] **Terminal Session / FrameDemand:** Publish deferred ordinary presentation while the counter is nonzero and restore eligibility when it returns to zero, without stopping parsing, Screen mutation, damage tracking, or preparation.
- [ ] **Terminal Widget Bridge / External Schedule:** Carry the concrete Terminal eligibility change through the T0001 bridge contract for the matched external draw.
- [ ] **Runtime Scheduler / Winit Presentation:** Suppress ordinary terminal presents during the batch and request/present the completed terminal frame as soon as the final disable restores eligibility.
- [ ] **Conformance Evidence:** Add parser, Terminal, Bridge/Runtime, and presenter tests for one batch, nesting, unmatched disables, and visible final release.

## Approach

1. Add session-owned synchronized-output state at the Terminal boundary and connect existing private CSI handling without expanding `harbor-parser`'s public API.
2. Increment on `?2026h`; decrement only when nonzero on `?2026l`; expose Set to DECRQM for every nonzero nesting depth.
3. Feed the derived ordinary-present eligibility into the T0001 demand contract while allowing normal Terminal processing and renderer preparation to continue.
4. Have the bridge and Runtime use the contract to avoid intermediate presentation, then release the current dirty terminal frame on the final matching disable.
5. Test the whole PTY-output-to-present path, including extra disables that must not underflow or block subsequent ordinary output.

## Blocked by

- T0001 — Requires the generic presentation-eligibility contract.

## Blocks

- T0003 — Recovery extends the synchronized state and deferred-present behavior introduced here.
- T0004 — Reset and session-close cleanup must clear the state introduced here.

## Acceptance

- [ ] `CSI ?2026h` enables deferred ordinary presentation and `CSI ?2026l` restores it at zero depth.
- [ ] Repeated enables require matching disables; disables at zero are no-ops.
- [ ] DECRQM reports Set at nonzero depth and Reset at zero depth.
- [ ] Screen contents update throughout a batch while no intermediate terminal presentation occurs.
- [ ] Final disable produces an observable completed terminal frame through the normal Runtime/presenter path.
- [ ] Focused parser, Terminal, Runtime/Bridge, and presenter tests pass.

## Out of Scope

- Time-based recovery for a missing final disable; T0003.
- RIS, DECSTR, PTY/session-close cleanup, and checklist evidence; T0004.
- DCS synchronization extensions or unrelated DEC private modes.
