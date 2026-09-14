# Confirmation Window Uses the Native Host

**Ticket ID:** T0004
**Source:** [Spec: 0012-widget-winit-native-host-consolidation](../../spec/0012-widget-winit-native-host-consolidation.md)
**Status:** Todo

## Goal

The independent paste-confirmation window uses the same configurable adapter-owned host as the main window, reuses compatible shared GPU resources, and preserves its separate Runtime behavior and application-owned paste safety policy.

## Surfaces

- `src/dialog.rs`: confirmation creation, root, event routing, results, positioning, and lifecycle.
- `src/shell.rs` and application paste coordination: host registration and cross-window routing.
- `crates/harbor-widget::winit`: only shared host APIs required by both windows; no confirmation-specific policy.
- Confirmation and winit integration tests.

## Approach

1. Construct the confirmation Window, Surface, Runtime, adapter state, scheduler, and presentation through the T0002 builder rather than local infrastructure fields.
2. Reuse compatible adapter-owned Instance/Adapter/Device/Queue resources while retaining an independent confirmation Surface, Runtime, viewport, and input state.
3. Supply undecorated size, ownership/z-order, centering, root construction, and other dialog-specific attributes through application configuration.
4. Delegate generic event, redraw, resize/DPI, effects, and idle behavior to the host while retaining confirm/cancel/close outcomes and preview behavior in the application.
5. Preserve the application-owned cross-window gate and independent confirmation FocusScope.

## Dependencies and Coordination

### Blocked by

- T0002 — Supplies the common owned host and shared GPU access.

### Blocks

- T0006 — Duplicate confirmation surface/runtime/frame code cannot be removed before migration.

### Coordination

- May proceed in parallel with T0003; coordinate ApplicationHandler window lookup, wait-policy aggregation, error mapping, and shared GPU access.
- Do not move paste text, confirmation decisions, main-window gating, ownership policy, or dialog positioning semantics into `harbor-widget`.

## Acceptance

- [ ] `ConfirmationWindow` uses the common adapter/builder instead of owning separate SurfaceConfiguration, Runtime, `WinitAdapter`, and manual frame target state.
- [ ] Confirmation and main hosts share compatible Instance/Adapter/Device/Queue resources but have independent Window, Surface, Runtime, viewport, scheduler, focus, and pointer state.
- [ ] Confirm, cancel, close, keyboard shortcuts, focus, preview scrolling, and raw-text preservation remain unchanged.
- [ ] The application blocks terminal keyboard input and new paste requests while confirmation exists, while permitted terminal output/rendering/scrollback behavior remains available.
- [ ] Dialog ownership/z-order, centering including negative-coordinate monitors, fixed size, DPI behavior, and independent redraw remain correct.
- [ ] Resize/zero-size/surface-recovery behavior is delegated to the host without affecting the main window.
- [ ] Repeated open/close cycles release per-window resources and do not leak callbacks or scheduler work.
- [ ] Existing dialog tests plus multi-window Windows smoke coverage pass.

## Out of Scope

- Converting confirmation into a main-window overlay.
- Moving paste safety or dialog business decisions into `harbor-widget`.
- HMR for confirmation composition unless it naturally uses the same application root contract selected by T0005.
