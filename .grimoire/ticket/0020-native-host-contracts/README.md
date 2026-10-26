# U5 — Native Host Contracts

**Source:** [spec 0042](../../spec/0042-native-host-roots-deadlines-and-gpu-outcomes.md), [ADR 0088](../../adr/0088-native-host-root-deadline-and-shared-gpu-fault-contracts.md), [UI/API review](../../../UI_API_REVIEW.md) U5/F9/F13/F15, and the approved 2026-10-26 refinement.
**Ticket folder:** .grimoire/ticket/0020-native-host-contracts/
**Endpoint:** Spec → tickets only in this refinement; implementation and QA have not started.

## Overview

Deliver three independently useful outcomes: Component/View root factory parity, non-rendering Host deadlines, and a complete production shared-GPU fatal reporting path. A final acceptance ticket binds integrated evidence to the actual tree. These tickets do not reopen U1–U4 or implement U6.

The settled policy is persistent per-SharedGpu fatal state; callback-thread record/wake rather than UI manipulation; explicit frame and non-frame reporting; OOM/unexpected device loss/uncaptured validation/internal are fatal; normal destruction is not a false fatal; Harbor owns diagnosis/exit; no automatic Device reconstruction. Future Host deadlines survive non-drawability without render-animation polling. Preserve local surface recovery, root mounting/output/error contracts, native ownership and HMR barriers.

Requirements and choices come from the spec/ADR and the approved conversation. Organize and implement within them; do not invent or revise domain facts, acceptance semantics or architecture. Return a needed consequential new decision to refinement before proceeding. API/helper spellings are not fixed, but their behavior must satisfy the spec. Historical tests or Proposed/Draft statuses establish neither missing implementation nor completed acceptance.

## Delivery Surfaces

- Native Host builders, public outcomes and lifetimes: `crates/harbor-widget/src/winit/{host,gpu,presenter,scheduler,mod}.rs` and any required generic wake/effects seam.
- Application event-loop/wake and consumers: `src/{shell,dialog,event}.rs`, main/paste/OSC52 confirmation paths and relevant bootstrap/HMR call sites.
- Public compile contracts, scheduler/presenter and callback-path regressions, native host smoke/multi-window tests, and affected macro/application tests.
- Current native-host docs/examples under `docs/widget/` and any actual affected public callers. File lists are seams, not instructions to modify every file.

## Requirement Coverage

| Contract | Delivery owner | Integrated verification |
| --- | --- | --- |
| R1 / F9; E1 | T0001 | T0004 |
| R2 / F15; E2 | T0002 | T0004 |
| R3 / F13; E3/E5 | T0003 | T0004 |
| R4 / F13; E3/E4 | T0003 | T0004 |
| R5 / F13; E3/E4 | T0003 | T0004 |
| R6 / preserved ownership/migration/lifetimes | T0001–T0003 for their respective changes | T0004, including E5 |

## Blocking Dependencies

| Producer | Consumer | Why consumer cannot complete first |
| --- | --- | --- |
| T0001 | T0004 | Integrated root-factory acceptance needs delivered public builder changes. |
| T0002 | T0004 | Integrated non-drawable deadline acceptance needs delivered scheduling behavior. |
| T0003 | T0004 | Integrated callback/wake/fatal policy acceptance needs real production wiring and migrated consumers. |

T0001, T0002 and T0003 have no semantic dependency on one another. T0004 may prepare its evidence matrix earlier, but cannot finish the integrated audit before all three deliveries exist. The graph has no other blocking edges.

## Coordination

| Tickets | Risk | Strategy |
| --- | --- | --- |
| T0001, T0003 | Shared builder/initialization and host-contract tests | Coordinate ownership or integrate serially; preserve root bounds/error/output while adding GPU wake/fatal contracts. |
| T0002, T0003 | Presenter/Host idle behavior, outcome shapes and application wait arbitration | Agree the existing spec-level deadline-versus-fatal contract; integrate together or serially so neither wait handling nor non-frame fatal is dropped. |
| T0001–T0003 | Shared native examples, docs and test fixtures | Each ticket migrates its own real consumers; reconcile combined declarations and features in T0004. |
| T0003, T0004 | Injected results can be mistaken for real-device/native acceptance | Separate evidence classes and retain explicit unavailable/unverified obligations. |

## Recommended Order

T0001 → T0002 → T0003 → T0004 is a low-conflict integration order, not a new dependency chain. The first three may proceed independently with coordinated shared-file edits. Keep GPU callbacks, shared state, wake transport, Host outcomes and application consumers together in T0003 rather than delivering a callback-only intermediate state.

## Ticket Index

| Ticket | File | Outcome |
| --- | --- | --- |
| T0001 | [T0001-root-factory-parity.md](./T0001-root-factory-parity.md) | Both builders accept Component/macro/existing View roots with preserved construction contracts. |
| T0002 | [T0002-non-rendering-host-deadlines.md](./T0002-non-rendering-host-deadlines.md) | Non-drawable Hosts honor future business wake deadlines without rendering or busy loops. |
| T0003 | [T0003-shared-gpu-fatal-delivery.md](./T0003-shared-gpu-fatal-delivery.md) | Production GPU notifications reach every sharing Host and Harbor's exit policy, even without redraw. |
| T0004 | [T0004-integrated-native-acceptance.md](./T0004-integrated-native-acceptance.md) | An integrated criterion-to-evidence verdict distinguishes implemented behavior, failures and remaining native/device/HMR evidence. |

## Handoff and Evidence Limits

Baseline source inspection used `a6da35c`; do not reuse earlier U4 test results as U5 verification. At execution time refresh the actual revision/dirty scope and the affected tests. All tickets start Todo; no implementation, QA result, native device fault or DLL-release proof is supplied by this set.

Record policy injection, actual wgpu callback-path checks, native event-loop/multi-window behavior and real OOM/device-loss evidence separately. Do not deliberately reset a driver/exhaust GPU resources without separate authorization. Missing real-device or DLL lifecycle evidence is needs-verification, not proof of a defect and not a passing result. Existing user-reported native interaction smoke does not need repetition solely because release-before-unload tracing is absent.
