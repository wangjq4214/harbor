# Synchronized Output Mode

**Source:** [Spec: 0007-synchronized-output-mode.md](../../spec/0007-synchronized-output-mode.md)
**Ticket folder:** `.grimoire/ticket/0007-synchronized-output-mode/`

## Overview

One ticket remains open in this set: lifecycle cleanup and conformance evidence for DEC `?2026`. The presentation-eligibility contract, the nested batch behavior, and the bounded 100 ms recovery are implemented and are recorded in the source spec and the historical destinations below.

## Dependency Graph

T0004 depends on the synchronized batch contract in [Spec 0007](../../spec/0007-synchronized-output-mode.md#solution) and on the recovery and cancellation behavior described in its [recovery E2E](../../spec/0007-synchronized-output-mode.md#e2e-an-unclosed-batch-receives-bounded-recovery-presents). It has no open ticket dependencies.

## Ticket Index

| Ticket | File | Title | Summary |
| --- | --- | --- | --- |
| T0004 | [T0004-lifecycle-cleanup-and-conformance-evidence.md](./T0004-lifecycle-cleanup-and-conformance-evidence.md) | Lifecycle Cleanup and Conformance Evidence | Clears synchronization on RIS and session close, then records verified conformance. |

## Historical Contract Destinations

| Historical ID | Surviving contract |
| --- | --- |
| 0007/T0001 | [Spec 0007 Solution](../../spec/0007-synchronized-output-mode.md#solution): presentation-eligibility seam between Terminal and the Runtime scheduler. |
| 0007/T0002 | [Spec 0007 Solution](../../spec/0007-synchronized-output-mode.md#solution): nested counter, deferred ordinary presentation, DECRQM status, and final-disable release. |
| 0007/T0003 | [Spec 0007 Solution](../../spec/0007-synchronized-output-mode.md#solution) and [recovery E2E](../../spec/0007-synchronized-output-mode.md#e2e-an-unclosed-batch-receives-bounded-recovery-presents): bounded 100 ms recovery and its cancellation on final disable. |

Windows smoke acceptance remains required by the [spec test plan](../../spec/0007-synchronized-output-mode.md#test-plan). These mappings do not mark that acceptance passed.
