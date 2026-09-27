# Terminal Engine and wgpu Renderer Boundary

**Source:** [Spec 0016](../../spec/0016-terminal-engine-wgpu-renderer-boundary.md), [ADR-0045](../../adr/0045-gpu-independent-terminal-core-boundary.md), [issue #176](https://github.com/wangjq4214/harbor/issues/176)
**Ticket folder:** `.grimoire/ticket/0014-terminal-engine-wgpu-boundary/`

## Overview

Deliver a GPU-independent terminal core while preserving the concrete wgpu renderer and application behavior. The compatibility facade protects callers only while they migrate and may be removed afterwards. The update contract preserves hidden/failed-draw changes; PTY lifetime belongs to the session adapter. No new renderer or generic rendering abstraction is included.

## Delivery Surfaces

1. **Core and build boundary:** `harbor-terminal` features/dependencies, screen/parser/input/selection/damage/preedit/timing, `TerminalSnapshot` and `UpdateDamage`.
2. **PTY session:** `harbor-terminal` I/O, `harbor-pty` endpoint/control capabilities, acknowledged resize, ownership map and safe teardown.
3. **Concrete renderer:** `harbor-terminal/src/render/` projection, GPU upload/invalidations, blink display, preedit and retained/live draw.
4. **Host integration:** `harbor-app` terminal view, tabs and scheduling; widget external-draw integration, GPU initialization/failure policy.
5. **Evidence:** core-only dependency graph, focused tests, quality gates, Windows scenarios and one-/multi-session performance comparison.

## Dependency Graph

| Ticket | Blocks | Concrete reason |
| --- | --- | --- |
| T0001 | T0002, T0003 | Session and renderer integration need the GPU-free engine/update interface and timing contract; independently verify it with a core-only build. |
| T0002 | T0004 | App migration cannot complete until the PTY session owns endpoints, resize ordering and teardown. |
| T0003 | T0004 | App migration cannot complete until the wgpu renderer consumes the update boundary and preserves retained/live drawing. |
| T0004 | T0005 | Final Windows and performance acceptance must exercise the integrated application, not an intermediate compatibility path. |
| T0005 | — | Final evidence closes the package. |

T0002 and T0003 are parallel with coordination after T0001; neither is blocked solely because the other touches `lib.rs`. T0005 baseline preparation and measurements should begin before migration, even though its final acceptance depends on T0004.

## Coordination Risks

| Tickets | Risk | Strategy |
| --- | --- | --- |
| T0001, T0002, T0003 | Existing `Terminal` combines model, I/O and renderer and `lib.rs` will change across tickets. | Fix the engine/update interface in T0001, then coordinate edits to facade and constructors without introducing a second source of truth. |
| T0002, T0003 | Renderer initialization failure touches PTY ownership despite separate implementation surfaces. | T0002 defines safe endpoint/session ownership; T0003 verifies renderer-local resource release; T0004 verifies their integrated failure handling. |
| T0003, T0004 | Live/retained drawing, viewport, scheduling and blink span the renderer and `TerminalWidgetBridge`. | Preserve one frame-demand contract and existing host-owned presentation; test hidden-tab resumption and retained drawing at integration. |
| T0005, all | Later changes invalidate earlier runtime/performance claims. | Capture baseline early, but attribute final evidence to the delivered revision and dirty-tree scope. |

## Recommended Order

1. T0001 — GPU-Free Engine and Update Boundary
2. T0002 — PTY Session Ownership; T0003 — wgpu Renderer Update Consumption (parallel with coordination)
3. T0004 — Application and Widget Migration
4. T0005 — Windows, Dependency and Performance Acceptance

The #169 contract and first #170 reflow path are closed prerequisites; preserve their behavior during this migration. Shared-file overlap alone is not a blocking edge.

## Ticket Index

| Ticket | File | Outcome |
| --- | --- | --- |
| T0001 | [T0001-gpu-free-engine-and-update-boundary.md](./T0001-gpu-free-engine-and-update-boundary.md) | Core-only compilation and tests prove the engine/update and timing boundary without GPU/widget dependencies. |
| T0002 | [T0002-pty-session-ownership.md](./T0002-pty-session-ownership.md) | PTY adapter owns endpoints, resize coordination and teardown without breaking engine behavior. |
| T0003 | [T0003-wgpu-renderer-update-consumption.md](./T0003-wgpu-renderer-update-consumption.md) | Existing wgpu renderer consumes coherent updates and recovers hidden/failed draws without model mutation. |
| T0004 | [T0004-application-and-widget-migration.md](./T0004-application-and-widget-migration.md) | Application callers migrate safely; compatibility facade may be removed when unused. |
| T0005 | [T0005-windows-dependency-and-performance-acceptance.md](./T0005-windows-dependency-and-performance-acceptance.md) | Integrated behavior, quality gates and before/after evidence are accurately recorded. |
