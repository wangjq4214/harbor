# Terminal Frame Scheduling and Standalone Host

**Source:** [Spec: 0006-terminal-frame-scheduling-and-standalone-host](../../spec/0006-terminal-frame-scheduling-and-standalone-host.md)
**Ticket folder:** `.grimoire/ticket/0006-terminal-frame-scheduling-and-standalone-host/`

## Overview

These tickets make cursor blinking deadline-driven when Terminal is idle, in both widget-hosted and direct winit/wgpu modes. Terminal provides one host-neutral Frame Demand; the widget Runtime owns scheduling in embedded mode, while a companion direct host consumes the same contract without adding window ownership to the Terminal core. The work preserves `CustomPaint` rendering and Runtime Host resource boundaries.

## Layers

The project's architectural layers confirmed during decomposition:

1. **harbor-terminal state and rendering** — Cursor state, Terminal Frame Demand, and terminal encoding into a supplied render pass.
2. **harbor-widget Runtime / CustomPaint** — External provider registration, frame scheduling, and widget-owned rendering policy.
3. **Runtime Host / winit-wgpu adapter** — Per-window control flow, drawable-surface lifecycle, and direct-host event-loop integration.
4. **Verification** — Unit, contract, integration, and end-to-end behavior tests.

Every ticket lists all confirmed layers; explicit `None` entries identify intentionally untouched layers.

## Dependency Graph

### Blocking relationships

| Prerequisite | Consumers | Reason |
| --- | --- | --- |
| [Spec 0006 shared Frame Demand contract](../../spec/0006-terminal-frame-scheduling-and-standalone-host.md#use-one-host-neutral-frame-demand-contract) | T0002, T0003 | Both hosts consume the same demand and blink-reset semantics. |
| T0002 | — | Widget-hosted behavior is independently demonstrable using the shared contract. |
| T0003 | — | Direct-host behavior is independently demonstrable using the shared contract. |

### Parallel groups

| Group | Tickets | Reason |
| --- | --- | --- |
| A | T0002, T0003 | Using the Spec 0006 Frame Demand contract, widget scheduling and the companion direct host modify separate modules and have no producer-consumer dependency between them. |

## Recommended Order

1. T0002 and T0003 in parallel — deliver widget-hosted and direct-host behavior using the shared Frame Demand contract.

## Ticket Index

| Ticket ID | File | Title | Summary |
| --- | --- | --- | --- |
| T0002 | [T0002-widget-hosted-cursor-blink.md](./T0002-widget-hosted-cursor-blink.md) | Widget-Hosted Cursor Blink | Let CustomPaint feed terminal deadlines to Runtime-owned scheduling. |
| T0003 | [T0003-standalone-terminal-winit-wgpu-host.md](./T0003-standalone-terminal-winit-wgpu-host.md) | Standalone Terminal Winit/WGPU Host | Provide direct winit/wgpu hosting that consumes the same contract. |

## Historical Contract Destinations

The stable ID 0006/T0001 identifies the retired Frame Demand planning slice. Its surviving destination is [Spec 0006's host-neutral demand and blink-reset contract](../../spec/0006-terminal-frame-scheduling-and-standalone-host.md#solution). Widget-hosted acceptance and the standalone host remain tracked by T0002 and T0003; this mapping does not complete either ticket.
