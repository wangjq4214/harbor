# Native Root Factory Component/View Parity

**Ticket ID:** T0001
**Source:** [spec 0042](../../spec/0042-native-host-roots-deadlines-and-gpu-outcomes.md) R1/R6, E1/E5; [ADR 0088](../../adr/0088-native-host-root-deadline-and-shared-gpu-fault-contracts.md) D1/D7; review F9.
**Status:** Done

## Goal

Both public native Host builder forms accept ordinary Components, macro Views and existing Views without wrapper Components, while preserving startup errors, bootstrap output and mounting timing.

## Affected Surfaces

- `crates/harbor-widget/src/winit/host.rs` build/build_with_output and affected imports/public compile contracts.
- Actual main/confirmation/bootstrap/HMR callers, native examples/docs and affected macro/native fixtures.
- Existing `winit_contracts.rs` and `native_host_smoke.rs` provide entry points; extend/reuse where they establish the contract rather than assuming coverage from their names.

## Approach

Use IntoChildView + 'static at both root result boundaries and the existing Runtime mounting protocol. Keep factory failure/panic attribution and output delivery atomic. Do not eagerly build Components or add Component-for-View compatibility behavior. Select concrete regression structure within R1; do not redesign HMR/root identity.

## Dependencies and Coordination

- **Blocked by:** None.
- **Blocks:** T0004's integrated E1 verdict until root-factory delivery exists.
- **Coordination risks:** T0003 touches the same Host initialization/outcome area; integrate serially or agree file ownership. Shared files do not make T0003 a prerequisite.

## Acceptance

- [ ] Public compilation covers both build forms with ordinary Component, macro-produced View and existing View results, without a wrapper Component. (R1/E1)
- [ ] Mounted/native construction preserves one factory invocation, setup-before-factory ordering and Runtime-owned build/Fiber/hooks; an existing View is passed through the established protocol. (R1/E1)
- [ ] Factory Err/panic and root-construction panic retain startup error attribution; build_with_output returns the intended output only alongside successful Host construction. (R1/E1)
- [ ] GPU bootstrap and reuse callers, initial update/focus/redraw behavior, actual application/static/HMR callers and relevant examples remain valid. (R1/R6/E1/E5)
- [ ] Affected tests/docs are migrated; results and native limitations are bound to the actual execution tree. No claim of real HMR unload proof comes from compile/mounted tests. (R6)

## Out of Scope

GPU fault policy, deadline semantics, automatic device reconstruction, new root identity/description types and HMR redesign.
