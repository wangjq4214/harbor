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

### Seams

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Native host configuration | Application Business Host → `harbor-widget::winit` | Window attributes, root factory, platform hooks, and error policy owned by the application | A configured per-window host that owns Window, Surface, Runtime, scheduling, and presentation |
| Event-loop routing | ApplicationHandler ↔ per-window adapter host | Window identity, native events, idle turns, and adapter wake work routed on the UI thread | Encapsulated event adaptation, Runtime updates, platform effects, redraw, and wait requests |
| Shared GPU resources | Main and confirmation adapter hosts ↔ widget GPU facility | Compatible surface capabilities and application-selected backend constraints | Shared Instance/Adapter/Device/Queue with independent per-window surfaces and configurations |
| Platform window integration | `harbor-widget::winit` ↔ application platform hooks | Harbor backdrop, caption, icon, ownership, positioning, and specialized composition requirements | Customized windows/surfaces whose generic lifecycle remains adapter-owned |
| Terminal rendering resources | `harbor-widget::winit` → `harbor-terminal` CustomPaint rendering | Adapter-owned Device/Queue access and frame-scoped render context | Terminal-owned pipelines and draws without terminal ownership of Window, Surface, or generic wgpu bootstrap |
| Reloadable root | Optional widget HMR integration ↔ application UI root factory | A fixed development-session contract and replacement root construction from Host-owned handles | Safe teardown barrier, active-generation root installation, Runtime update, and redraw |
| Application state/actions | Application Business Host ↔ reloadable/static widgets | Store-published state and Dispatcher actions drained at the application event-turn boundary | Business state retained across reloads without reducers or effects moving into `harbor-widget` |

## End-to-End Tests

### E2E: Main window starts through the consolidated host

- **Given:** Harbor starts with its normal static UI configuration.
- **When:** ApplicationHandler resumes for the first time.
- **Then:** The widget winit integration creates and configures the main Window, shared GPU resources, Surface, Runtime, and initial root; the application supplies Harbor-specific configuration but performs no duplicate generic winit/wgpu/Runtime bootstrap.

### E2E: Confirmation window uses the same host boundary

- **Given:** The main window is active and a paste requires confirmation.
- **When:** The application creates the independent confirmation window.
- **Then:** The confirmation window is built through the same adapter/builder, reuses compatible shared GPU resources, owns an independent Runtime and Surface lifecycle, and preserves application-owned cross-window input gating.

### E2E: Application UI reload retains native and business resources

- **Given:** Harbor is running in Windows debug HMR mode with active tabs, terminal sessions, PTYs, and Host-owned Store state.
- **When:** A valid application UI generation is rebuilt.
- **Then:** The adapter tears down the old root before replacement, installs and redraws the new root, resets Widget/Fiber-local state, and retains window identity, GPU resources, terminal sessions, PTYs, and application state.

### E2E: Old-generation callbacks cannot execute

- **Given:** The active root contains action callbacks and terminal external-draw registrations from the current dynamic generation.
- **When:** Reload begins while input or rendering work may be pending.
- **Then:** The adapter cancels or isolates stale input ownership, destroys the old root and registrations before releasing the reload barrier, and exposes only callbacks from the active replacement generation after reload.

### E2E: Terminal rendering consumes adapter-owned GPU resources

- **Given:** A terminal CustomPaint node is present in either a static or reloaded root.
- **When:** The adapter renders a frame.
- **Then:** Terminal-owned pipelines draw using adapter-provided GPU/frame access without owning or creating the Window, Surface, Instance, Adapter, Device, or Queue, and no terminal dependency is introduced into `harbor-widget`.

### E2E: Resize and surface recovery remain encapsulated

- **Given:** Either native window is running through the adapter.
- **When:** It resizes, changes DPI, becomes zero-sized, restores, or receives a recoverable surface error.
- **Then:** The adapter updates viewport and configuration state, suspends zero-sized acquisition, follows the existing recovery/presentation policy, and reports only unrecoverable failures for application fatal-policy handling.

### E2E: Ordinary builds remain static

- **Given:** Harbor is built without the HMR feature or on an unsupported HMR target.
- **When:** The application starts and source or dynamic-library outputs change.
- **Then:** The same native host path runs with a static root factory, no reload observer or lifecycle is active, and normal multi-window behavior remains unchanged.

### E2E: Business behavior remains application-owned

- **Given:** Widgets dispatch tab, terminal, or paste intent through existing application contracts.
- **When:** The adapter completes event routing.
- **Then:** The application drains and reduces actions once at its established boundary, coordinates PTY and cross-window effects, and remains the sole owner of business policy.

## Decisions

### Consolidate native infrastructure without moving the complete event loop

- **Choice:** `harbor-widget::winit` owns reusable per-window native infrastructure, while the binary retains EventLoop/ApplicationHandler and application-wide coordination.
- **Reason:** This removes generic host mechanics from the application without coupling Harbor-specific multi-window and process policy to the widget crate.
- **ADR reference:** [0031-widget-winit-adapter-owns-native-host-infrastructure](../adr/0031-widget-winit-adapter-owns-native-host-infrastructure.md)

### Share GPU infrastructure while keeping surfaces per window

- **Choice:** The widget integration owns shareable Instance/Adapter/Device/Queue resources and independent per-window Surface/Runtime state.
- **Reason:** Both windows use one reusable initialization boundary while preserving the established Runtime-per-window model.
- **ADR reference:** [0031-widget-winit-adapter-owns-native-host-infrastructure](../adr/0031-widget-winit-adapter-owns-native-host-infrastructure.md)

### Keep terminal GPU state terminal-owned

- **Choice:** Move only generic wgpu bootstrap and surface ownership; retain terminal pipelines/resources behind CustomPaint GPU injection.
- **Reason:** This keeps `harbor-widget` independent of `harbor-terminal` and preserves terminal rendering ownership.
- **ADR reference:** [0011-terminal-custompaint-gpu-injection](../adr/0011-terminal-custompaint-gpu-injection.md), [0031-widget-winit-adapter-owns-native-host-infrastructure](../adr/0031-widget-winit-adapter-owns-native-host-infrastructure.md)

### Encapsulate generic HMR lifecycle in the widget adapter

- **Choice:** The optional widget winit/HMR integration owns observer and generation lifecycle; applications provide root factories and stable state/action handles.
- **Reason:** HMR root replacement is native Widget host infrastructure, while application contracts and durable state remain business concerns.
- **ADR reference:** [0031-widget-winit-adapter-owns-native-host-infrastructure](../adr/0031-widget-winit-adapter-owns-native-host-infrastructure.md), [0029-widget-store-state-action-boundary](../adr/0029-widget-store-state-action-boundary.md)

### Preserve the independent confirmation window and application input gate

- **Choice:** Main and confirmation windows use the same adapter type but remain separate native windows with independent Runtime state; the application continues to gate cross-window input.
- **Reason:** Infrastructure reuse does not change the accepted paste safety or independent-window behavior.
- **ADR reference:** [0007-retain-separate-paste-confirmation-window](../adr/0007-retain-separate-paste-confirmation-window.md), [0009-app-cross-window-input-gate](../adr/0009-app-cross-window-input-gate.md)

## Test Plan

- **Contract tests:** Verify adapter construction and public ownership boundaries without requiring application code to assemble SurfaceConfiguration, WinitFrameTarget, or Runtime scheduling state. Verify `harbor-widget` has no dependency on `harbor-terminal`.
- **Integration tests:** Cover main/confirmation host isolation, shared GPU identity, independent surfaces and Runtimes, event adaptation, RuntimeEffects application, resize/DPI transitions, zero-size suspension, surface recovery, and fatal error propagation.
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

## Future Evolution

- Reconsider moving more ApplicationHandler mechanics only if another application needs the same process-level multi-window orchestration and a business-neutral abstraction can be demonstrated.
- Add explicit device-loss recreation or multi-adapter selection when those become supported runtime requirements.
- Introduce a backend-neutral native-host abstraction only when a second production window/event backend exists.
- Revisit versioned or serialized HMR contracts only if changing shared types without restart becomes a required workflow.
