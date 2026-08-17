# Lifecycle Cleanup and Conformance Evidence

**Ticket ID:** T0004
**Source:** [Spec: 0007-synchronized-output-mode](../../spec/0007-synchronized-output-mode.md)
**Status:** Todo

## Goal

RIS and PTY/session close reliably release synchronized-output suppression, and the protocol checklist records evidence for the completed `?2026` behavior.

## Layers

- [ ] **VT Parser / Screen State:** Clear synchronized-output state on RIS while preserving it on DECSTR, with protocol-query behavior verified after each reset path.
- [ ] **Terminal Session / FrameDemand:** Clear state when Terminal I/O observes PTY reader/session shutdown so a surviving terminal view returns to ordinary presentation eligibility.
- [ ] **Terminal Widget Bridge / External Schedule:** Propagate released eligibility and remove any sync-driven scheduling demand for the bridge's matched terminal.
- [ ] **Runtime Scheduler / Winit Presentation:** Cancel pending recovery work and permit the normal post-cleanup redraw/present path without violating drawable-surface behavior.
- [ ] **Conformance Evidence:** Add end-to-end reset and closure coverage, record the evidence, and check only the verified `?2026` entries in `docs/protocol/checklist.md`.

## Approach

1. Connect RIS hard-reset handling to the Terminal/session synchronized-output state, while explicitly leaving DECSTR unchanged.
2. Define the Terminal I/O close/EOF path that clears the same state exactly once and makes a remaining view eligible for ordinary redraw.
3. Ensure bridge and Runtime demand collection observe the cleared state, cancel stale recovery deadlines, and do not retain suppression after cleanup.
4. Exercise RIS and close paths from PTY output through the visible Runtime/presenter contract, including a prior forced-recovery state.
5. Update the protocol checklist only after automated evidence establishes the spec's batch, nesting, recovery, and lifecycle criteria.

## Blocked by

- T0001 — Uses the shared presentation-eligibility contract.
- T0002 — Clears the concrete synchronized state and validates normal release.
- T0003 — Must verify cancellation of the completed recovery behavior.

## Blocks

- None.

## Acceptance

- [ ] RIS clears every active synchronization depth and permits ordinary presentation.
- [ ] DECSTR leaves an active synchronization depth and its query status unchanged.
- [ ] PTY EOF/session close clears synchronization and a surviving terminal view can redraw normally.
- [ ] Cleanup cancels pending recovery scheduling without a stale or busy wake.
- [ ] End-to-end tests cover reset and session close after active synchronization, including recovery state.
- [ ] `docs/protocol/checklist.md` checks only `?2026` items supported by the completed automated evidence.

## Out of Scope

- Changes to alternate-screen, reflow, wide-cell, mouse, or focus semantics.
- Windows manual smoke execution itself; this ticket supplies the protocol evidence that unblocks it.
- New synchronized-output protocol families or diagnostics beyond the accepted checklist evidence.
