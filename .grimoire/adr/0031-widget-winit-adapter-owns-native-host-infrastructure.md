# Move Native Widget Host Infrastructure into the Winit Adapter

**Status:** Implementing
**Date:** 2026-09-13
**Supersedes:** [0015-runtime-owned-frame-presentation.md](./0015-runtime-owned-frame-presentation.md), [0030-application-layer-widget-hot-reload.md](./0030-application-layer-widget-hot-reload.md)

## Context

Harbor's binary currently owns generic winit window bootstrap, wgpu instance/adapter/device/queue/surface setup, frame-resource injection, and application-widget HMR lifecycle. This leaves the main application coordinating infrastructure that is common to native widget hosts rather than focusing on terminal, tabs, paste, and other business behavior. Alternatives were to retain the borrowed-resource boundary, move the complete event loop into `harbor-widget`, or consolidate reusable per-window infrastructure while keeping application-wide business coordination outside the widget crate.

## Decision

Make `harbor-widget::winit` the reusable native widget-host boundary. Its configurable adapter/builder owns per-window Window, Surface, Runtime, input/scheduling state, and presentation lifecycle, and owns generic wgpu instance/adapter/device/queue initialization with sharing support across windows. The application retains `ApplicationHandler` and EventLoop ownership, multi-window routing, exit/fatal policy, and Harbor-specific business coordination; it supplies window attributes and platform hooks such as backdrop configuration instead of implementing generic initialization.

Move generic debug HMR observation, reload lifecycle, teardown barriers, and Runtime root replacement into an optional `harbor-widget` winit/HMR integration. Applications supply a reloadable root factory and Host-owned state; Harbor-specific UI contracts do not move into `harbor-widget`.

Split the existing terminal GPU context: generic wgpu bootstrap and surface management move to the widget winit adapter, while terminal-specific pipelines and resources remain in `harbor-terminal` and are initialized from borrowed shared Device/Queue handles. Both the main window and paste-confirmation window use the same configurable adapter/builder.

## Consequences

- The main application primarily coordinates business state and policies instead of native widget infrastructure.
- Window creation, Runtime setup, wgpu initialization, surface recovery, scheduling, rendering, and optional HMR root replacement have one reusable owner.
- The application still owns process-level event-loop and multi-window policy, preventing Harbor-specific coordination from leaking into `harbor-widget`.
- Backdrop, icons, platform chrome, terminal/PTY behavior, paste policy, reducers, and fatal/exit decisions remain application concerns exposed through configuration or hooks.
- `harbor-widget` must not depend on `harbor-terminal`; terminal GPU resources consume adapter-provided Device/Queue access.
- The former borrowed `WinitFrameTarget` Host contract may be removed or made internal because the adapter owns the resources it previously borrowed.
- Existing HMR safety requirements remain: old callbacks and roots are dropped before library replacement, Widget/Fiber state resets, Host-owned business state survives, and shared contract changes require restart.
