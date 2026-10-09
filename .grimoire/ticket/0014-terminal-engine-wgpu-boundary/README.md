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

| Contract or active ticket | Required by | Concrete reason |
| --- | --- | --- |
| [Spec 0016 engine/update and timing contract](../../spec/0016-terminal-engine-wgpu-renderer-boundary.md), with intermediate core-only evidence | T0003 | The renderer must consume the GPU-free update and timing interface; intermediate evidence is not final renderer acceptance. |
| [Session ownership contract](../../../docs/architecture/terminal-session-ownership.md) and [ADR-0043 resize ordering](../../adr/0043-acknowledged-pty-resize-barrier.md) | T0004 | App migration must preserve endpoint ownership, resize ordering, construction-failure cleanup and teardown, and verify them in the integrated host path. |
| T0003 | T0004 | App migration cannot complete until the wgpu renderer consumes the update boundary and preserves retained/live drawing. |
| T0004 | T0005 | Final Windows and performance acceptance must exercise the integrated application, not an intermediate compatibility path. |
| T0005 | — | Final evidence closes the package. |

T0003 and T0004 coordinate on the surviving engine/update and session ownership contracts; shared-file overlap alone is not a blocking edge. T0005 baseline preparation and measurements should begin before migration, even though its final acceptance depends on T0004.

## Coordination Risks

| Tickets | Risk | Strategy |
| --- | --- | --- |
| T0003, T0004 | Engine, session and renderer integration share facade and constructor surfaces. | Use Spec 0016's engine/update and timing contract and the session ownership map; avoid a second source of truth. |
| T0003, T0004 | Renderer initialization failure touches PTY ownership despite separate implementation surfaces. | Preserve safe endpoint/session ownership from the ownership map; T0003 verifies renderer-local resource release and T0004 verifies integrated failure handling. |
| T0003, T0004 | Live/retained drawing, viewport, scheduling and blink span the renderer and `TerminalWidgetBridge`. | Preserve one frame-demand contract and existing host-owned presentation; test hidden-tab resumption and retained drawing at integration. |
| T0005, all | Later changes invalidate earlier runtime/performance claims. | Capture baseline early, but attribute final evidence to the delivered revision and dirty-tree scope. |

## Recommended Order

1. Review [Spec 0016](../../spec/0016-terminal-engine-wgpu-renderer-boundary.md), intermediate core-boundary evidence and the [session ownership map](../../../docs/architecture/terminal-session-ownership.md) as migration prerequisites, not final acceptance.
2. T0003 — wgpu Renderer Update Consumption
3. T0004 — Application and Widget Migration (coordinate with T0003; integrated completion depends on it)
4. T0005 — Windows, Dependency and Performance Acceptance

The #169 contract and first #170 reflow path are closed prerequisites; preserve their behavior during this migration. Shared-file overlap alone is not a blocking edge.

## Ticket Index

| Ticket | File | Outcome |
| --- | --- | --- |
| Historical T0001 | [Spec 0016 test/evidence plan](../../spec/0016-terminal-engine-wgpu-renderer-boundary.md#test-and-evidence-plan), core-boundary evidence | GPU-free engine/update and timing constraints survive in the spec; intermediate core-only and CPU-side baseline evidence does not establish final integrated acceptance. |
| Historical T0002 | [Spec 0016](../../spec/0016-terminal-engine-wgpu-renderer-boundary.md#test-and-evidence-plan), [session ownership map](../../../docs/architecture/terminal-session-ownership.md) | Endpoint, resize and teardown obligations remain constraints; the recorded Done status and unchecked acceptance items are not reconciled or promoted to a pass. Integrated lifecycle verification remains T0004/T0005 work. |
| T0003 | [T0003-wgpu-renderer-update-consumption.md](./T0003-wgpu-renderer-update-consumption.md) | Existing wgpu renderer consumes coherent updates and recovers hidden/failed draws without model mutation. |
| T0004 | [T0004-application-and-widget-migration.md](./T0004-application-and-widget-migration.md) | Application callers migrate safely; compatibility facade may be removed when unused. |
| T0005 | [T0005-windows-dependency-and-performance-acceptance.md](./T0005-windows-dependency-and-performance-acceptance.md) | Integrated behavior, quality gates and before/after evidence are accurately recorded. |
