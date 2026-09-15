# Legacy Host Cleanup and Acceptance

**Ticket ID:** T0006
**Source:** [Spec: 0012-widget-winit-native-host-consolidation](../../spec/0012-widget-winit-native-host-consolidation.md)
**Status:** Complete

## Goal

Harbor has one native Widget host implementation with no duplicate binary-owned winit/wgpu/Runtime/HMR orchestration, and executable evidence confirms static/HMR, main/confirmation, terminal, platform, and business behavior.

## Surfaces

- `src/shell.rs`, `src/dialog.rs`, `src/hot_reload.rs`, `src/event.rs`, `src/effects.rs`, and root manifests: remove obsolete infrastructure and dependency/feature wiring.
- `crates/harbor-widget::winit` and `crates/harbor-terminal`: finalize public/internal boundaries and compatibility removal.
- Workspace tests, contract tests, Windows smoke/feasibility evidence, and developer HMR commands/documentation where currently maintained.

## Approach

1. Remove compatibility constructors, duplicate surface configuration, direct Runtime/frame orchestration, application-owned generic HMR lifecycle, and obsolete frame-resource injection after all consumers migrate.
2. Make `WinitFrameTarget` internal or remove it from the application contract; keep only APIs required by adapter implementation and external drawing.
3. Remove root-level generic GPU/HMR dependencies and feature wiring that no longer have an application-owned use, while preserving application platform dependencies required by backdrop/chrome hooks.
4. Confirm the binary ApplicationHandler reads as business coordination: window routing, models/reducers, terminal/PTY and paste policy, platform configuration, and fatal/exit decisions.
5. Run focused, workspace, feature-matrix, and manual Windows verification from Spec 0012; record any environment-only evidence required for HMR and native composition.
6. Treat failures as implementation blockers or focused follow-up work rather than retaining two competing host paths.

## Dependencies and Coordination

### Blocked by

- T0003 — Main window must use the owned host.
- T0004 — Confirmation window must use the owned host.
- T0005 — Adapter-owned HMR must replace application lifecycle code.

### Blocks

- (none)

### Coordination

- This ticket is the integration point for files shared by earlier migrations; rebase/merge those changes before cleanup.
- Do not remove Harbor-specific platform hooks or business event handling merely because they still reference winit/window identities.

## Acceptance

- [x] The binary contains no duplicate generic Window/Surface/Runtime setup, surface configuration, frame target assembly, update/render/present flow, scheduler policy, or HMR generation state.
- [x] Both windows use the same configurable `harbor-widget::winit` host and share compatible GPU resources while retaining independent per-window state.
- [x] `harbor-terminal` owns terminal GPU pipelines/resources but no generic native GPU/window/surface bootstrap; `harbor-widget` has no terminal dependency.
- [x] The application retains EventLoop/ApplicationHandler, multi-window routing, Store reduction, tabs, terminal/PTY, paste gate, backdrop/chrome, and fatal/exit policy.
- [x] Static and HMR roots use one application contract; default/release/unsupported configurations do not activate HMR.
- [x] Resize, DPI, minimize/restore, zero size, recoverable surface errors, cursor/IME/clipboard, idle Wait behavior, and one-redraw/one-frame execution pass regression coverage.
- [x] Confirmation open/close, focus, shortcuts, preview, cross-window gating, terminal output, tab actions, and PTY lifetimes pass application regression coverage.
- [x] Repeated Windows HMR changes layout and callback behavior while preserving native/GPU/terminal/PTY identities and preventing stale callbacks.
- [x] Workspace formatting, linting, tests, backend-feature checks, Windows HMR build checks, and repository documentation checks pass.
- [x] No compatibility path remains solely to preserve the superseded ADR 0015/0030 ownership model.

## Out of Scope

- New Widget, terminal, tab, paste, backdrop, or HMR product features.
- Moving ApplicationHandler/EventLoop or business policy into `harbor-widget`.
- Device-loss recreation, multiple adapters, backend-neutral hosting, or versioned dynamic contracts.
