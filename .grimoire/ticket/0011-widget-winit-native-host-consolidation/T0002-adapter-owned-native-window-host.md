# Adapter-Owned Per-Window Native Host

**Ticket ID:** T0002
**Source:** [Spec: 0012-widget-winit-native-host-consolidation](../../spec/0012-widget-winit-native-host-consolidation.md)
**Status:** Todo

## Goal

A configurable `harbor-widget::winit` host owns one native Window, Surface, Runtime, viewport/surface state, event adaptation, Runtime scheduling, generic effects, and complete frame presentation while leaving process-level and business policy to the application.

## Surfaces

- `crates/harbor-widget/src/winit/mod.rs`, `event.rs`, `surface.rs`, `presenter.rs`, and focused new modules where needed.
- `crates/harbor-widget/tests/winit_contracts.rs`: public contract, lifecycle, scheduler, and recovery behavior.
- Application-facing construction and event-routing seams used later by `src/shell.rs` and `src/dialog.rs`.

## Approach

1. Extend or compose the current `WinitAdapter` behind a configurable builder that accepts application-owned Window attributes, root construction, shared GPU resources, and platform hooks.
2. Move long-lived Window, Surface, Runtime, per-window SurfaceConfiguration, input state, scheduler, and presentation lifecycle behind the host boundary.
3. Encapsulate supported WindowEvent handling, idle turns, redraw/update/render/present, resize/DPI handling, zero-size suspension, surface recovery, and generic RuntimeEffects application.
4. Expose only the window identity/access, business-event result, wait requirement, GPU access, and recoverable/fatal outcomes required by ApplicationHandler coordination.
5. Remove the need for application callers to construct `WinitFrameTarget` or manually synchronize Runtime and surface state; retain it only as an internal detail if still useful.
6. Return startup and unrecoverable runtime failures to the application rather than owning exit policy.

## Dependencies and Coordination

### Blocked by

- T0001 — Supplies widget-owned shared GPU and surface support.

### Blocks

- T0003 — Main-window migration consumes the host contract.
- T0004 — Confirmation-window migration consumes the host contract.
- T0005 — Generic HMR must replace roots inside the owned host.

### Coordination

- T0003 and T0004 must use the same construction/event API rather than adding window-specific host variants.
- Platform hooks must remain configuration seams; they must not introduce Harbor backdrop, terminal, or paste types into `harbor-widget`.

## Acceptance

- [ ] A per-window host owns Window, Surface, Runtime, adapter state, scheduler, SurfaceConfiguration, and presentation lifecycle.
- [ ] Construction accepts application-provided attributes, root construction, compatible shared GPU resources, and platform customization without importing application types.
- [ ] Event routing, idle scheduling, generic RuntimeEffects, redraw, frame acquisition, submission, pre-present notification, and presentation are executable through the host API.
- [ ] Resize, scale-factor changes, zero-size suspension/restoration, lost/outdated/suboptimal/timeout handling, and fatal GPU outcomes preserve existing behavior.
- [ ] Application callers do not construct `WinitFrameTarget`, configure surfaces, or directly coordinate Runtime updates for normal host operation.
- [ ] Each host has independent Runtime/input/scheduler/surface state while GPU resources can be shared.
- [ ] Fatal/exit and multi-window policy remain outside `harbor-widget`.
- [ ] Deterministic contract tests cover lifecycle transitions that do not require a real OS surface; applicable native smoke tests cover real creation/presentation.

## Out of Scope

- Application main/confirmation migration.
- Harbor-specific backdrop, icon, caption, paste, terminal, tab, or PTY behavior.
- EventLoop/ApplicationHandler ownership.
- HMR generation management.
