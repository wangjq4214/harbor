# Runtime Integration Foundation

**Ticket ID:** T0001
**Source:** [Spec: 0004-widget-runtime-host-simplification](../../spec/0004-widget-runtime-host-simplification.md)
**Status:** Done

## Goal

All later slices can compile against one documented winit integration contract without committing to obsolete binary-owned frame orchestration.

## Layers

- [ ] **Runtime Host:** Enable the `harbor-widget` winit feature in the binary and establish compile-only Host call sites or fixtures for the shared contracts.
- [ ] **Winit Runtime Integration:** Add the feature-gated module boundary and define `WinitAdapter`, `RuntimeEffects`, `WinitFrameTarget`, frame outcome/error, external invalidation, and control-flow effect contracts.
- [ ] **Core Widget Runtime:** Expose only the platform-independent hooks required by the integration; keep winit types out of core modules.
- [ ] **Terminal / Application Components:** None — terminal and confirmation behavior do not change in pre-refactoring; their existing public contracts must continue compiling.
- [ ] **Verification:** Add compile/API tests for feature-on and feature-off builds and contract-level tests for effect defaults and outcome classification.

## Approach

1. Add optional `winit` dependency and feature wiring to `crates/harbor-widget/Cargo.toml`, preserving a winit-free default core build.
2. Create a focused integration module rather than adding winit types to `runtime.rs` or `input` core APIs.
3. Define frame-scoped borrowed target types that cannot outlive Window, Surface, Device, or Queue and cannot be retained by Runtime.
4. Define composable effects for redraw, event-loop control flow, cursor, IME, and clipboard requests, plus explicit fatal frame outcomes.
5. Add the minimum Runtime entry points for generic invalidation and effect production; do not implement scenario behavior yet.
6. Update ADR 0004 status to Superseded with ADR 0015 as successor, preserving ADR history.
7. Verify both `harbor-widget` feature configurations and the workspace compile before starting a vertical slice.

## Blocked by

- (none)

## Blocks

- T0002 — event adaptation consumes `WinitAdapter` and `RuntimeEffects`.
- T0003 — scroll routing consumes the integration event contract.
- T0004 — scheduling consumes external invalidation and control-flow effects.
- T0005 — presentation consumes `WinitFrameTarget` and frame outcomes.
- T0006 — recovery extends the frame outcome contract.
- T0007 — confirmation integration consumes all per-window contracts.

## Acceptance

- [ ] `harbor-widget` builds and tests with default features and with the winit integration feature enabled.
- [ ] Core Runtime public signatures contain no winit types.
- [ ] `WinitFrameTarget` contains borrowed resources and cannot transfer their ownership into Runtime.
- [ ] Runtime effect and frame outcome contracts have deterministic tests.
- [ ] The workspace still compiles with existing App behavior unchanged.
- [ ] ADR 0004 points to ADR 0015 as its superseding decision.

## Out of Scope

- Converting actual `WindowEvent` values.
- Moving scheduling behavior.
- Acquiring or presenting a SurfaceTexture.
- Changing terminal or confirmation-window behavior.
