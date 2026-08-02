# Runtime-Owned Frame Presentation with Borrowed Host Resources

**Status:** Implementing
**Date:** 2026-07-25

## Context

Widget event conversion and frame orchestration currently span the binary and `harbor-widget`, while earlier decisions assigned GPU submission and presentation to the App. Alternatives were to keep submission and presentation in the App, transfer ownership of windows and surfaces to Runtime, or let the runtime integration execute the complete frame using resources borrowed from the App.

## Decision

Keep the App as Runtime Host and long-term owner of each Window, Surface, Device, and Queue, but make the feature-gated `harbor-widget` winit runtime integration responsible for event adaptation and the complete frame policy. For each frame the Host injects borrowed platform resources through a `WinitFrameTarget`; the integration acquires the SurfaceTexture, encodes Widget and CustomPaint rendering, submits GPU work, calls the window pre-present notification, and presents the frame without retaining Window or Surface references.

Each window continues to use an independent Runtime, while Device, Queue, and text resources may be shared. The core Runtime remains platform-independent, and platform operations outside frame presentation continue to be returned as RuntimeEffects for the Host to apply.

## Consequences

- Rendering and presentation policy move out of `src/`, while window creation, destruction, resource lifetimes, cross-window routing, and fatal-error policy stay in the App.
- Runtime owns UI-specific pipelines, buffers, textures, scene state, layout, and paint scheduling but does not own Window, Surface, Device, or Queue.
- Lost and outdated surfaces are reconfigured by the integration, timeouts skip a frame, zero-sized windows suspend drawing, and out-of-memory errors return to the App as fatal.
- The winit integration may depend on winit and wgpu without exposing winit types through the platform-independent Runtime API.
- ADR 0008's assignment of submission and presentation to the App and ADR 0014's equivalent Host responsibility are superseded; their per-window Runtime and thin-Host intentions remain incorporated here.
