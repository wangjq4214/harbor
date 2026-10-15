# Configuration Hot Reload: Remaining Delivery

**Source:** [Spec 0024](../../spec/0024-watched-reload-and-launch-settings.md), [spec 0023](../../spec/0023-atomic-manual-configuration-reload.md), [ADR 0062](../../adr/0062-watched-reload-and-new-session-launch-settings.md), [#155](https://github.com/wangjq4214/harbor/issues/155).
**Ticket folder:** `.grimoire/ticket/0017-configuration-hot-reload/`
**Status:** In Progress — T0001 Done, supported by automated checks, scoped Windows smoke and user-confirmed prior manual verification; T0002–T0004 remain open. Local tickets, not created GitHub sub-issues.

## Overview

Accept the existing manual/font transaction, add environment and initial-directory launch settings, deliver default-on watching, and establish package-level Windows evidence. Environment and directory are included, not deferred. No existing PTY restart, second acceptance policy, global environment/directory mutation or widget-HMR conflation.

At refinement, PR #201 contains the manual implementation; it is not treated as accepted or merged. T0001 is scoped acceptance/remediation, not a request to reimplement it. Spec 0024's conversation contract records the user-approved policies. Concrete field spelling, watcher backend and debounce duration are not fixed here.

## Delivery Surfaces

- `harbor-config`: launch-field schema and validation; startup/live policy separation.
- `src/shell.rs`, `src/event.rs`, `src/tab_coordinator.rs`: reload triggers, acceptance, feedback and future factories.
- `harbor-pty`: child-only environment and directory; native launch failure.
- Existing terminal/font/input/OSC seams: retain manual transaction invariants.
- README/config example/status/roadmap, retrievable Windows evidence, issue scope reconciliation.

## Ticket Index

| Ticket | File | Outcome |
| --- | --- | --- |
| T0001 | [T0001-manual-reload-acceptance.md](./T0001-manual-reload-acceptance.md) | Done — manual/font baseline accepted with automated and user-confirmed manual evidence; package exclusions remain |
| T0002 | [T0002-session-environment-and-directory.md](./T0002-session-environment-and-directory.md) | New sessions receive accepted environment/directory through startup and manual reload |
| T0003 | [T0003-watched-configuration-reload.md](./T0003-watched-configuration-reload.md) | Default-on saves/replacements/bursts drive the same strict transaction |
| T0004 | [T0004-package-integration-and-evidence.md](./T0004-package-integration-and-evidence.md) | Combined behavior and #155 completion supported by retrievable evidence |

## Requirement Coverage

| Source | Owner |
| --- | --- |
| 0024 R1; 0023 R1–R8 and E1–E11 | T0001 baseline; T0004 integrated regressions |
| 0024 R2/R3; E1–E7/E12 | T0002 |
| 0024 R4; manual future-policy acceptance | T0002 |
| 0024 R4/R5; E8/E9/E11 and ordering with existing live fields | T0003 |
| 0024 E10 across all live/launch fields; R6 and final evidence | T0004 |
| Redaction, failure retention, no process restart | T0002/T0003 own slice checks; T0004 integrated check |

## Dependencies

| Producer | Consumer | Why consumer cannot finish first |
| --- | --- | --- |
| T0001 | T0004 | Final acceptance needs established manual/font behavior and known exclusions |
| T0002 | T0004 | Combined tests need real environment/directory launch behavior |
| T0003 | T0004 | Combined tests need real automatic observation |

T0002 and T0003 use the existing manual implementation in PR #201, or its successor containing the same transaction, as a code baseline. They do not depend on each other's completion or on completion of T0001's evidence work. Integration with both is verified in T0004. A defect found in T0001 may create a specific blocker; do not preemptively invent a blanket dependency.

## Coordination

| Tickets | Risk | Strategy |
| --- | --- | --- |
| T0001/T0002/T0003 | Shared coordinator, fixtures and feedback; baseline fixes may affect both extensions | Keep acceptance contract stable, coordinate file edits, rebase fixes and rerun affected tests |
| T0002/T0003 | Manual versus watched invocation could split policy or publish factories differently | Both use R4; no second parser/acceptance owner; T0004 verifies interleaving |
| All/T0004 | Missing or ignored evidence and stale scope claims | Preserve retrievable artifacts and exact revisions; reconcile claims only to actual results |

## Recommended Order

Start T0001 to expose baseline failures early. T0002 and T0003 may proceed independently with coordinated shared-file edits; there is no new reflow prerequisite. Finish T0004 after all three outputs are available. This is outcome sequencing, not an implementation plan.

## Execution Boundary

Use `grimoire-loop` when execution is authorized. This ticket set does not itself run implementation/QA, edit Git ignore policy, stage/commit changes, open GitHub sub-issues or close #155/#172. At execution, reconcile remote scope through authorized issue updates; otherwise retain an explicit pending remote-action item. Store durable evidence under the issue's allowed policy or retrievable CI artifacts, not a missing local-only link.
