# Widget runtime internals

The runtime turns immutable Rust descriptions into a retained GPU scene. This guide covers ownership and invariants; [widget concepts](../widget/concepts.md), [catalog](../widget/catalog.md) and [recipes](../widget/recipes.md) cover use.

## Retained layers

```text
View -> reconciliation -> Fiber tree -> committed layout/hit geometry
     -> paint -> SceneItems / SceneDelta -> GPU encoding -> host presentation
```

| Layer | Lifetime and owner |
| --- | --- |
| `View` | Short-lived immutable build result |
| `Fiber` | Retained identity, hooks, subscriptions, children and dirty state |
| Layout data | Retained logical geometry, constraints, clipping and hit regions |
| `SceneItem` | Stable draw identity, primitive, clip stack and paint order |
| Renderer | Pipelines, text resources, caches and incremental uploads |
| `Runtime` | Per-window tree, input state, scene and encoding coordination |
| `WinitWindowHost` | Native window, surface, Runtime, adapter and presenter |
| `SharedGpu` | Shared wgpu instance, adapter, device and queue |

Descriptions, component state, geometry and GPU resources are not interchangeable owners. `harbor-widget` has no terminal-model dependency; its core uses wgpu but native-window integration is behind the `winit` feature.

## Identity and invalidation

`FiberId` is generation-qualified: removing a fiber and reusing its arena slot must not make stale captures or subscriptions address a new node. Reconciliation matches within a parent's sibling list:

- a key unique in both lists can preserve a compatible widget-type fiber across positions;
- an unkeyed view can preserve only the compatible unkeyed fiber at the same position;
- mismatches create new fibers; duplicate keys produce diagnostics and do not reuse ambiguous identities;
- unmatched old fibers unmount, releasing subscriptions and transient input ownership.

`Signal<T>` is UI-thread pull-based state: `cx.track(&signal)`, `store.watch(cx)` or hook creation establish subscriptions; `read()` alone does not subscribe; writes enqueue the subscribing fibers for their runtime. The dirty queue is not proof of isolated subtree rebuilding: `Runtime::flush_pending_work` conservatively rebuilds from the root for queued build work. Layout-only invalidation follows a separate path without evaluating components. Cross-thread producers notify the host rather than writing signals directly.

## Geometry and event routing

Layout uses logical pixels, parent-directed child measurement, bounded corrective passes and an atomic final-geometry commit. Painting and subsequent input routing observe committed geometry; physical scaling belongs at the rendering/native boundary. Layout observer callbacks run after settlement; writes from those callbacks queue another turn. The application, not layout, owns PTY resize policy. See the full [flex layout contract](../flex-layout.md).

Pointer routing is hit test -> root-to-target capture -> target -> target-to-root bubble. Handlers issue `EventCtx` commands; ownership mutations are applied after routing. Focus, hover, pointer capture, modal routing and IME are per-runtime. Native normalization belongs to `WinitAdapter`, including scale, modifiers, focus-loss pointer quarantine and IME commit deduplication.

Text editing belongs to the reusable controller/editing core/field layers. Search meaning and query limits remain application policy. Terminal preedit is a separate transient overlay through the bridge. See [text input](../widget-input.md) and [`event_router.rs`](../../crates/harbor-widget/src/runtime/event_router.rs).

`Portal` moves declared content into a root overlay outside ancestor layout/clipping without reserving terminal space. It does not itself supply modal barriers, anchored placement or focus trapping. Product notification eligibility, placement and dismissal history remain application-owned.

## Scene and external draws

Painting updates an ordered flat scene; `SceneDelta` records additions, modifications and removals for retained GPU data. Only adjacent compatible items may batch. Transparent primitives must not cross paint boundaries during optimization.

Rectangular clipping uses scissors; rounded clipping also constrains painting and hit testing, including external draws. Decoration does not alter measured size. `BackdropFilter` samples already-drawn in-window content and leaves foreground sharp; it is not desktop capture or Windows Acrylic. Encoder-level `Runtime::encode_frame` supports these local filters; the caller-owned render-pass `encode` path rejects filter primitives. Product glass recipes remain outside the toolkit.

`ExternalSurface` registers an `ExternalDrawId` with independent draw, input, IME and schedule providers. Encoding flushes widget batches around the external item, supplies physical allocation/scissor and frame-scoped GPU access, then restores state for following items. The terminal owns its renderer and input semantics; the runtime owns the region's geometry, clipping, focus, routing and paint order.

External eligibility is tracked per draw. A deferred terminal can reuse its retained projection while widgets in the same window continue updating; a forced commit can request live projection. Engine update acknowledgement belongs to the bridge, never the generic widget renderer. See [terminal session](terminal-session.md).

## Native host lifecycle

Each OS window has an independent Runtime, surface, adapter, scheduler and input state. Sharing GPU or text resources must not share surface/focus/capture state. The binary owns `ApplicationHandler`, multi-window routing, platform authorization and fatal/exit policy; the optional widget host owns reusable native infrastructure. [ADR 0031](../../.grimoire/adr/0031-widget-winit-adapter-owns-native-host-infrastructure.md) records this split.

For a drawable frame, the host resolves runtime updates and external demand, acquires a surface texture, encodes the scene, submits, calls `pre_present_notify` and presents. Native presentation classifies skipped/recovery/fatal outcomes for the application. Zero-sized surfaces suspend drawing; surface recovery does not recreate the terminal session.

`RuntimeEffects` carries neutral redraw, control-flow, cursor, IME and clipboard requests. The per-window scheduler coalesces outstanding redraws and chooses wait/deadline/poll behavior. A clean runtime with no deadline or animation must not spin. External synchronized-output demand has bounded recovery without freezing unrelated widget work.

Paste confirmation uses a separate native host/runtime. The application owns the cross-window input gate. Optional HMR tears down old root-owned registrations and callbacks before unloading the old library, retaining native infrastructure and host business state; HMR is not configuration reload.

`harbor-text` supplies shared font selection, fallback, metrics and rasterization, while terminal and widget renderers keep their own cell/run adapters and caches. Atlas repacking must invalidate dependent UV geometry. See [architecture](architecture.md) for product boundaries and [profiling](profiling.md) before changing resource lifetimes.
