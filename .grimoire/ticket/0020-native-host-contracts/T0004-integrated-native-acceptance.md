# Integrated U5 Native Acceptance

**Ticket ID:** T0004
**Source:** [spec 0042](../../spec/0042-native-host-roots-deadlines-and-gpu-outcomes.md) R1–R6, E1–E5 and Verification/Completion Boundaries; [ADR 0088](../../adr/0088-native-host-root-deadline-and-shared-gpu-fault-contracts.md); review §6.
**Status:** Todo

## Goal

Produce a criterion-to-evidence verdict for the integrated U5 delivery. Establish what is implemented and verified, identify concrete gaps, and retain explicit native/device/HMR evidence obligations instead of equating test counts with full acceptance.

## Affected Surfaces

- Delivered T0001–T0003 contracts, public root compilation and mounted/native behavior, scheduler/adapter and actual GPU notification delivery.
- Application main/paste/OSC52 consumers, shared-device secondary lifecycle, affected features, native examples/docs and regression boundaries.
- Evidence and remaining-work handoff in this ticket/spec as authorized during execution; no unrelated code redesign is implied by an acceptance ticket.

## Approach

Audit the combined tree against the spec, not only each ticket's local test summary. Bind observations/commands to the actual revision/dirty scope and environment/backend. Exercise or explicitly classify each E1–E5 case; retain distinctions between policy injection, actual production callbacks, native event-loop behavior, hardware failure and real HMR-generation release.

## Dependencies and Coordination

- **Blocked by:** T0001, T0002 and T0003 for completion: integrated verification needs their actual delivered behavior and migrated consumers. Evidence-matrix preparation is possible earlier.
- **Blocks:** No other ticket; findings may require fixes in the owning delivery ticket, not a new unapproved behavior contract.
- **Coordination risks:** Recheck after shared-file integration; an earlier local run does not validate later outcome/signature/wait changes. Return new consequential decisions to refinement rather than modifying acceptance semantics.

## Acceptance

- [ ] Every R1–R6 requirement and E1–E5 scenario has criterion-specific code/execution evidence or an explicit gap/needs-verification classification. A green test summary alone does not establish complete acceptance.
- [ ] Verify both factory forms/root kinds, factory/output/error/mount timing, and actual application/example/static/HMR call compatibility. (R1/R6/E1)
- [ ] Verify future/absent/expired deadlines, suspended rendering/recovery, restoration/coalescing and cross-window arbitration, including native suspended-window wake evidence or its explicit unverified status. (R2/E2)
- [ ] Trace actual Device notification → shared state → idle/minimized UI wake → non-frame/frame report → application exit. Verify secondary-first observation, main/paste/OSC52 consumers, pending-action safety and local-surface-recovery separation. (R3–R5/E3/E4)
- [ ] Verify failed construction, repeated secondary closure, normal destruction, closed transport, callback lifetime/native destruction order and relevant HMR integration. Record real release-before-DLL-unload proof separately from mounted teardown. (R3/R6/E5)
- [ ] Run/record affected widget/native/macro/application checks and relevant default/backend/debug-HMR/release combinations under repository policy, with native/ignored/skipped limitations identified. Check docs/examples, formatting and diff scope.
- [ ] Report injected policy checks, real production callback-path checks, native multi-window/minimized checks, real OOM/device-loss checks and real HMR lifecycle evidence separately. Obtain separate authorization before destructive fault experiments.
- [ ] The final handoff distinguishes audit completion from full U5 acceptance. Missing real-device/DLL evidence stays open and does not become either a fabricated pass or an unproven product defect. Do not require repetition of already user-reported interaction smoke solely because lifecycle tracing is missing.

## Out of Scope

New product/architecture choices, automatic device recovery, repairing unrelated U1–U4/U6 behavior and claiming full native/GPU/HMR acceptance from static compilation or injected errors alone.
