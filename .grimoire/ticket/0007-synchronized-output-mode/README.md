# Synchronized Output Mode

**Source:** [Spec: 0007-synchronized-output-mode.md](../../spec/0007-synchronized-output-mode.md)
**Ticket folder:** `.grimoire/ticket/0007-synchronized-output-mode/`

## Overview

These tickets implement DEC private synchronized output (`?2026`) without intermediate terminal presentation, while retaining bounded recovery and lifecycle cleanup. They preserve Terminal ownership of session state, the generic Terminal-to-Widget scheduling boundary, and Runtime ownership of presentation. The work is decomposed so the reusable presentation contract is established before observable batching, recovery, and cleanup behavior.

## Layers

The project's architectural layers (confirmed during decomposition):

1. **VT Parser / Screen State** — private CSI dispatch, mode querying, and reset semantics in `harbor-terminal`.
2. **Terminal Session / FrameDemand** — session state, PTY lifecycle, and host-neutral terminal demand.
3. **Terminal Widget Bridge / External Schedule** — the `CustomPaint` adapter translating Terminal demand into `harbor-widget`-neutral demand.
4. **Runtime Scheduler / Winit Presentation** — generic deadline scheduling, redraw coalescing, and frame execution with borrowed Host resources.
5. **Conformance Evidence** — parser, terminal, runtime, winit, and protocol-checklist verification.

Every ticket cuts through all confirmed layers.

## Dependency Graph

### Blocking relationships

| Ticket | Blocks | Reason |
| --- | --- | --- |
| T0001 | T0003, T0004 | Recovery and cleanup require the shared generic presentation-eligibility contract. |
| [Spec 0007 synchronized batch contract](../../spec/0007-synchronized-output-mode.md#solution) | T0003, T0004 | Recovery and lifecycle cleanup act on the concrete synchronized-output state and normal-release behavior. |
| T0003 | T0004 | Final evidence must include recovery behavior and shares the Terminal-to-Scheduler contract. |
| T0004 | — | Final lifecycle and checklist evidence closes the feature. |

### Parallel groups

None. Every post-foundation ticket modifies the same Terminal → Bridge → Scheduler contract or relies on behavior produced by the preceding ticket, so parallel work would risk contract and file conflicts.

## Recommended Order

1. T0001 — Presentation Eligibility Contract (pre-refactoring)
2. T0003 — Bounded Forced Recovery, using the Spec 0007 synchronized batch contract
3. T0004 — Lifecycle Cleanup and Conformance Evidence

## Ticket Index

| Ticket ID | File | Title | Summary |
| --- | --- | --- | --- |
| T0001 | [T0001-presentation-eligibility-contract.md](./T0001-presentation-eligibility-contract.md) | Presentation Eligibility Contract | Establishes the generic Terminal-to-Runtime contract used by every slice. |
| T0003 | [T0003-bounded-forced-recovery.md](./T0003-bounded-forced-recovery.md) | Bounded Forced Recovery | Forces a normal presentation every 100 ms for an unclosed synchronized batch. |
| T0004 | [T0004-lifecycle-cleanup-and-conformance-evidence.md](./T0004-lifecycle-cleanup-and-conformance-evidence.md) | Lifecycle Cleanup and Conformance Evidence | Clears synchronization on RIS and session close, then records verified conformance. |

## Historical Contract Destinations

The stable ID 0007/T0002 identifies the retired basic synchronized batch planning slice. Its surviving destination is [Spec 0007's nested counter, deferred ordinary presentation, DECRQM status, and final-disable release contract](../../spec/0007-synchronized-output-mode.md#solution). T0001 remains retained, and T0003/T0004 still own unresolved recovery, lifecycle, and conformance evidence; Windows smoke acceptance remains required by the [spec test plan](../../spec/0007-synchronized-output-mode.md#test-plan).
