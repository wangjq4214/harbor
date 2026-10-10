# Terminal Engine and wgpu Renderer Boundary

**Source:** [Spec 0016](../../spec/0016-terminal-engine-wgpu-renderer-boundary.md), [ADR-0045](../../adr/0045-gpu-independent-terminal-core-boundary.md), [issue #176](https://github.com/wangjq4214/harbor/issues/176)
**Ticket folder:** `.grimoire/ticket/0014-terminal-engine-wgpu-boundary/`

## Overview

One ticket remains open: Windows, dependency, and performance acceptance for the core/renderer/session boundary. The engine/update boundary, the wgpu renderer consumption, and the application migration are implemented; their contracts are recorded in [Spec 0016](../../spec/0016-terminal-engine-wgpu-renderer-boundary.md) and the historical destinations below.

## Dependency Graph

| Contract | Required by | Concrete reason |
| --- | --- | --- |
| [Spec 0016 solution boundary](../../spec/0016-terminal-engine-wgpu-renderer-boundary.md#solution-boundary) and [necessary seams](../../spec/0016-terminal-engine-wgpu-renderer-boundary.md#necessary-seams) | T0005 | Final acceptance must exercise the integrated engine, session, renderer, and host path. |

## Recommended Order

1. T0005 — Windows, Dependency and Performance Acceptance

## Coordination Risks

| Tickets | Risk | Strategy |
| --- | --- | --- |
| T0005, all | Later changes invalidate earlier runtime/performance claims. | Capture baseline early, but attribute final evidence to the delivered revision and dirty-tree scope. |

## Ticket Index

| Ticket | File | Outcome |
| --- | --- | --- |
| T0005 | [T0005-windows-dependency-and-performance-acceptance.md](./T0005-windows-dependency-and-performance-acceptance.md) | Integrated behavior, quality gates and before/after evidence are accurately recorded. |

## Historical Contract Destinations

These mappings do not mark integrated acceptance passed.

| Historical ID | Contract destination |
| --- | --- |
| T0001 (GPU-Free Engine and Update Boundary) | [Spec 0016 test and evidence plan](../../spec/0016-terminal-engine-wgpu-renderer-boundary.md#test-and-evidence-plan): GPU-free engine/update and timing constraints; intermediate core-boundary evidence. |
| T0002 (PTY Session Ownership) | [Spec 0016 test and evidence plan](../../spec/0016-terminal-engine-wgpu-renderer-boundary.md#test-and-evidence-plan) and [session ownership map](../../../docs/architecture/terminal-session-ownership.md): endpoint, resize, and teardown obligations. |
| T0003 (wgpu Renderer Update Consumption) | [Spec 0016 solution boundary](../../spec/0016-terminal-engine-wgpu-renderer-boundary.md#solution-boundary): the renderer consumes a coherent engine update and never mutates parser or screen state. |
| T0004 (Application and Widget Migration) | [Spec 0016 necessary seams](../../spec/0016-terminal-engine-wgpu-renderer-boundary.md#necessary-seams) and [end-to-end verification cases](../../spec/0016-terminal-engine-wgpu-renderer-boundary.md#end-to-end-verification-cases): application callers migrated; one owner per engine, session, and renderer. |
