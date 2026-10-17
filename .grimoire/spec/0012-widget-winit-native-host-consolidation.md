# Widget Winit Native Host Consolidation

**Spec ID:** 0012
**Status:** Implementing
**Date:** 2026-09-13

## Requirements

Harbor must consolidate reusable native Widget hosting in `harbor-widget::winit` so the binary application focuses on terminal, tabs, paste, and other business behavior rather than generic winit, wgpu, Runtime, presentation, or HMR infrastructure.

The consolidated integration must:

1. Own configurable per-window creation and the Window, Surface, Runtime, event-adaptation, scheduling, and presentation lifecycle.
2. Own generic wgpu Instance, Adapter, Device, Queue, surface configuration, and cross-window GPU-sharing infrastructure.
3. Support both the main window and the independent paste-confirmation window through the same adapter/builder boundary.
4. Move generic debug HMR observation, teardown barriers, reload lifecycle, and Runtime root replacement into an optional `harbor-widget` winit/HMR capability.
5. Preserve application-owned state across reloads while resetting the replaced Widget/Fiber tree and preventing callbacks from an unloaded generation from remaining reachable.
6. Split terminal-specific GPU resources from generic native-host GPU resources without introducing a `harbor-widget` dependency on `harbor-terminal`.
7. Leave the application responsible for EventLoop/ApplicationHandler coordination, window routing, application models and reducers, terminal/PTY policy, paste safety, platform-specific window policy, and fatal/exit decisions.
8. Remove duplicate native-host orchestration from the binary after both native windows use the consolidated integration.

## Solution

Extend the feature-gated `harbor-widget::winit` boundary from an event/presentation adapter into a configurable native Widget host. Each constructed window host owns its native Window and Surface, its independent Widget Runtime, its input and scheduling state, its surface configuration, and its complete update/render/present lifecycle. The host applies generic Runtime effects such as redraw, cursor, IME, clipboard, and wait-policy operations without requiring the application to reproduce Widget integration policy.

The adapter/builder accepts application-provided window attributes, root construction, and platform hooks. This keeps Harbor-specific title, icon, theme, backdrop, caption, ownership, positioning, and fallback behavior outside `harbor-widget`, while preventing those customizations from forcing the application to own generic initialization. Where Windows composition requires specialized surface creation, the configured platform seam supplies the platform operation and the resulting resources remain under adapter lifecycle ownership.

A widget-owned shared GPU facility initializes and retains the wgpu Instance, Adapter, Device, and Queue. Per-window hosts create and own their own Surface and SurfaceConfiguration against those shared resources. The main window establishes the shared facility; the confirmation window reuses compatible GPU resources while retaining its own Runtime, input state, scheduler, viewport, and surface lifecycle. Startup and configuration errors are returned to the application, which retains fatal/exit policy.

The existing `harbor-terminal::render::gpu::GpuContext` is separated by responsibility. Instance/Adapter/Device/Queue ownership, main and secondary Surface creation, surface capabilities, and generic configuration move to the widget winit integration. Terminal-specific pipelines, upload policy, atlases, buffers, and rendering state remain in `harbor-terminal` and initialize from adapter-provided Device/Queue access. Terminal rendering continues through the established CustomPaint injection boundary; `harbor-widget` does not import terminal types.

The optional HMR integration owns reload observation, synchronization with the UI thread, old-root teardown, generation activation, replacement-root installation, Runtime update, and redraw scheduling. The application supplies a reloadable root factory and Host-owned state handles, but does not implement the reload state machine. Any event-loop bridge needed to wake the UI thread exposes only adapter work to dispatch; reload lifecycle ordering and blocker release remain encapsulated by the adapter.

Before a dynamic-library generation can be replaced, the adapter removes the old Runtime root and completes destruction of its Views, Fibers, external registrations, and callbacks while the old generation remains loaded. It then releases the reload barrier, activates the valid replacement generation, builds and installs one new root, updates the Runtime, and requests presentation through its ordinary scheduler. Widget-local hook state resets. Application models, terminal sessions, PTYs, Store state, and other application-owned resources remain alive. Changes to the shared dynamic contract still require an application restart.

The binary ApplicationHandler continues to identify the target window and forward native events or adapter wake work to the corresponding host. It owns the collection and relationship of main and confirmation hosts, enforces the cross-window paste gate, reduces Store actions, coordinates PTYs and terminal tabs, and decides whether a reported host failure exits the process. It does not manually create Runtime/surface state, configure surfaces, inject frame targets, execute Widget frames, or coordinate HMR generations.

### E2E: Main window starts through the consolidated host

First static startup creates main Window, shared GPU, Surface, Runtime and root through the adapter without duplicate application bootstrap.

## Decisions

[ADR 0031](../adr/0031-widget-winit-adapter-owns-native-host-infrastructure.md) owns the native-host consolidation and supersedes ADRs 0015/0030. Preserve the [separate confirmation window](../adr/0007-retain-separate-paste-confirmation-window.md), [application input gate](../adr/0009-app-cross-window-input-gate.md), and [Store boundary](../adr/0029-widget-store-state-action-boundary.md).

## Test Plan

- **Contract tests:** Verify adapter construction and public ownership boundaries without requiring application code to assemble SurfaceConfiguration, WinitFrameTarget, or Runtime scheduling state. Verify `harbor-widget` has no dependency on `harbor-terminal`.
- **Integration tests:** Cover main/confirmation host isolation, shared GPU identity, independent surfaces and Runtimes, event adaptation, RuntimeEffects application, resize/DPI transitions, zero-size suspension, surface recovery, and fatal error propagation. Dialog ownership, z-order, centering including negative-coordinate monitors, fixed size and DPI behavior stay correct; repeated open/close cycles release per-window resources and scheduler work.
- **HMR tests:** Verify prepare/teardown/release/activate/install ordering, rapid reload serialization, failed-build behavior, shutdown/proxy failure, stale callback elimination, Store state retention, Dispatcher drain-once behavior, and static/HMR parity.
- **Terminal tests:** Preserve terminal CustomPaint rendering and input tests while replacing generic GpuContext ownership; verify terminal pipelines initialize from adapter-provided Device/Queue and render in Widget paint order.
- **Application regression tests:** Verify tab actions, PTY lifetime, paste confirmation, cross-window input gating, backdrop/fallback selection, icons/chrome, terminal allocation, focus, clipboard, and fatal-exit behavior.
- **Build matrix:** Check workspace default builds, relevant backend features, Windows debug HMR builds, and non-Windows/default configurations without HMR activation.
- **Manual Windows tests:** Start Harbor, open and close confirmation windows repeatedly, move windows across DPI boundaries, minimize/restore, trigger surface recovery where practical, and repeatedly reload visible layout and callback changes while terminal processes continue running.
- **Performance regression:** An idle host must retain Wait behavior without redraw spinning; consolidation must not add duplicate frame acquisition, Runtime update, or GPU submission for one redraw.
- **Edge cases:** Reload during pointer capture/focus, reload while minimized, confirmation open during reload, queued actions at teardown, invalid HMR generation, application exit during reload, unsupported surface transparency, and shared-GPU initialization failure.

## Out of Scope

- Moving EventLoop/ApplicationHandler ownership or Harbor's complete multi-window coordinator into `harbor-widget`.
- Moving terminal/PTY models, tab reducers, paste policy, backdrop policy, platform chrome decisions, or fatal/exit policy into the widget crate.
- Making `harbor-widget` depend on `harbor-terminal` or embedding terminal-specific pipelines in the generic adapter.
- Combining the confirmation UI into the main window.
- Preserving Widget/Fiber hook state across HMR generations.
- Supporting shared-contract ABI changes without restarting the application.
- Release-mode dynamic loading or a cross-platform HMR guarantee.
- Introducing a general message bus, reducer framework, serialization boundary, or backend-neutral window framework.

## Outstanding Acceptance

Historical 0011/T0001–T0004 contracts survive in Requirements, Solution and Test Plan. Adapter-owned HMR (T0005) and duplicate-host cleanup (T0006) are implemented in source; retirement of their execution notes does not establish integrated acceptance.

Required proofs remain: old Views/Fibers, external draw/schedule registrations, pointer/focus ownership and callbacks are unreachable before the old library unloads; each valid generation installs exactly one root and redraws without recreating Window, Surface, Instance, Adapter, Device, Queue, terminals or PTYs. Repeated real Windows DLL generations must pass TypeId, duplicate-dependency, tracing, callback-lifetime and generation feasibility checks. Exercise failed builds, rapid reloads, missing startup sessions, closed proxies and application exit without deadlock or invalid activation; static/non-HMR/unsupported builds must have no active observer.

The retired T0005 explicitly lacked repeated interactive Windows watcher evidence, while T0006 checked all acceptance boxes including repeated Windows reloads without an accompanying execution record. That discrepancy remains unresolved: do not infer a runtime pass from source, checkbox state or this cleanup.

Verify both-window lifecycle/GPU isolation, business/input/paste behavior, restart-only shared-contract changes, default/backend/debug-HMR/release build parity, idle Wait and one-redraw/one-frame behavior under the Test Plan and [validation policy](../../docs/validation.md). No duplicate binary Window/Surface/Runtime setup, surface configuration, WinitFrameTarget assembly, frame execution, scheduler/HMR state or compatibility path solely for the superseded ownership model may remain. WinitFrameTarget must be internal or absent from the application contract; required external-draw/platform seams remain. Failures block acceptance or require explicit follow-up, not competing host paths.
