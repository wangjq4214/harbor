# ExternalSurface and Cross-Crate External Draw Migration

**Ticket ID:** T0005
**Source:** [spec 0041](../../spec/0041-widget-api-naming-and-configuration-validation.md) R1 (ExternalSurface), R3 (external providers), R8; [ADR 0087](../../adr/0087-widget-api-naming-and-validation-boundaries.md) D1/D3/D5; [related file](./README.md).
**Status:** Todo

## Goal

Let the public names express external draw, input, IME, and scheduling responsibilities, and wire them through the terminal bridge/application root/provider consumers, without changing actual ownership or resource lifetime.

## Affected Surfaces

- `widgets/custom_paint.rs` → the target ExternalSurface module, and root/module exports.
- The real terminal bridge, application/host/HMR composition calls, and registration/description references tied to the old component name.
- SingleChild/external draw/input/IME/scheduling/lifecycle, cross-crate/API fixtures, and current API docs.

## Approach

Fully migrate the type/module names and the handler/schedule/ime methods, together with real consumers and docs; do not rename still-accurate concept names such as ExternalDrawId/ExternalDrawFn, and do not rewrite the renderer, host factory, or provider ABI/ownership.

Carry U1's already-delivered Option<View>/SingleChild semantics into the new type instead of fixing the child again.

## Dependencies and Coordination

- **Blocked by:** none; the root-export overlap with T0003 is only a merge coordination, not a semantic dependency.
- **Blocks:** T0006 full cross-crate, external provider, and lifecycle acceptance.
- **Coordination risks:** shares exports/macro fixtures with T0003/T0004; the actual terminal/Host/HMR seams need live caller verification rather than string replacement. Coordinate file ownership or integrate serially first.

## Acceptance

- [ ] The CustomPaint type, its module/path, and all real consumers become ExternalSurface with no old alias; the new public API and cross-crate declarations compile and the old entries are rejected.
- [ ] handler → on_draw, schedule → schedule_with, and ime → ime_with are fully migrated; input, on_unmount, etc. are preserved, and providers are not turned into events or given different Send/Sync/Rc/Arc/'static contracts.
- [ ] The actual draw/input/IME/schedule call timing, GPU borrowing, host resource ownership, and scheduling semantics are preserved; U5's Host View factory or deadline rework is not delivered along the way.
- [ ] The new type preserves U1's single-slot replace/clear and static/dynamic count rules; registration/subscription cleanup on stable rebuild and real unmount is correct.
- [ ] The spec E1/E7 real paths for external draw/terminal bridge have regressions; direct pointer focus, Tab/IME, and existing policies do not change because of naming.
- [ ] Docs, examples, and compile fixtures are fully migrated; native focus/IME and real HMR DLL generation teardown results are recorded separately as verified/not run/blocked, and no cross-generation identity compatibility is promised from headless tests or type renaming.

## Out of Scope

Host/GPU/renderer architecture rewrites, the device-fault production path, HMR ABI redesign, mechanically renaming ExternalDrawId/ExternalDrawFn, cross-parent state migration, or a new focus policy.
