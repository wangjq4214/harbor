# Shared Widget GPU Foundation and Terminal Resource Split

**Ticket ID:** T0001
**Source:** [Spec: 0012-widget-winit-native-host-consolidation](../../spec/0012-widget-winit-native-host-consolidation.md)
**Status:** Todo

## Goal

`harbor-widget::winit` owns reusable wgpu Instance/Adapter/Device/Queue and surface-support infrastructure, while `harbor-terminal` owns only terminal-specific GPU pipelines and rendering resources initialized from adapter-provided GPU access.

## Surfaces

- `crates/harbor-widget/src/winit/`: shared GPU ownership, backend constraints, surface creation/capability/configuration support, and safe Device/Queue access.
- `crates/harbor-widget/Cargo.toml`: feature/dependency wiring required by the native GPU host.
- `crates/harbor-terminal/src/render/gpu.rs` and adjacent terminal renderer modules: split generic bootstrap from terminal-specific resources.
- `crates/harbor-terminal/tests/boundary_contract.rs` and widget winit contract tests: ownership and dependency evidence.

## Approach

1. Establish a widget-owned shared GPU resource boundary that can serve more than one per-window Surface.
2. Move generic Instance/Adapter/Device/Queue ownership, surface capabilities, and reusable surface configuration support out of the terminal context.
3. Retain terminal upload policy, pipelines, atlases, buffers, and draw behavior in `harbor-terminal`; initialize them from borrowed adapter GPU handles.
4. Preserve backend selection and Windows specialized composition requirements without importing Harbor application or terminal types into `harbor-widget`.
5. Keep a compatibility path only where needed to maintain a buildable migration into T0002–T0004; identify it explicitly for removal in T0006.

## Dependencies and Coordination

### Blocked by

- (none)

### Blocks

- T0002 — The owned native host needs the shared GPU boundary.

### Coordination

- Coordinate terminal constructor changes with `crates/harbor-app` and `src/shell.rs`, which currently create terminal resources from the application-owned `GpuContext`.
- Windows DirectComposition/backdrop behavior crosses the application platform hook seam; do not move backdrop policy into `harbor-widget`.

## Acceptance

- [ ] `harbor-widget::winit` owns reusable Instance/Adapter/Device/Queue resources and exposes the minimum GPU access needed by per-window hosts and external drawing.
- [ ] `harbor-terminal` no longer owns or creates Window, Surface, Instance, Adapter, Device, or Queue.
- [ ] Terminal-specific pipelines, upload state, atlases, buffers, and rendering behavior remain in `harbor-terminal`.
- [ ] Terminal CustomPaint can initialize and render from adapter-provided GPU/frame access with unchanged paint ordering.
- [ ] Main and secondary surfaces can query capabilities and configure against the shared GPU boundary.
- [ ] `harbor-widget` has no dependency on `harbor-terminal` or Harbor application types.
- [ ] Existing backend feature configurations and terminal rendering/contract tests pass.

## Out of Scope

- Migrating either application window to the final owned host.
- HMR behavior.
- Device-loss recreation or multiple-adapter policy beyond the current supported behavior.
