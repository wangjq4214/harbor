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

| Ticket | Blocks | Reason |
| --- | --- | --- |
| T0001 | T0002, T0003 | Both hosts consume the shared Terminal Frame Demand and blink-reset semantics. |
| T0002 | — | Widget-hosted behavior is independently demonstrable after T0001. |
| T0003 | — | Direct-host behavior is independently demonstrable after T0001. |

### Parallel groups

| Group | Tickets | Reason |
| --- | --- | --- |
| A | T0002, T0003 | After T0001 freezes the contract, widget scheduling and the companion direct host modify separate modules and have no producer-consumer dependency between them. |

## Recommended Order

1. T0001 — establish and test the shared Frame Demand foundation.
2. T0002 and T0003 in parallel — deliver widget-hosted and direct-host behavior.

## Ticket Index

| Ticket ID | File | Title | Summary |
| --- | --- | --- | --- |
| T0001 | [T0001-terminal-frame-demand-foundation.md](./T0001-terminal-frame-demand-foundation.md) | Terminal Frame Demand Foundation | Define the shared blink scheduling contract and reset behavior. |
| T0002 | [T0002-widget-hosted-cursor-blink.md](./T0002-widget-hosted-cursor-blink.md) | Widget-Hosted Cursor Blink | Let CustomPaint feed terminal deadlines to Runtime-owned scheduling. |
| T0003 | [T0003-standalone-terminal-winit-wgpu-host.md](./T0003-standalone-terminal-winit-wgpu-host.md) | Standalone Terminal Winit/WGPU Host | Provide direct winit/wgpu hosting that consumes the same contract. |
