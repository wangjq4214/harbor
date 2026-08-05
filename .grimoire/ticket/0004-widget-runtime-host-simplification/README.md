# Widget Runtime Host Simplification

**Source:** [Spec: 0004-widget-runtime-host-simplification.md](../../spec/0004-widget-runtime-host-simplification.md)
**Ticket folder:** `.grimoire/ticket/0004-widget-runtime-host-simplification/`

## Overview

Move generic winit event adaptation, frame scheduling, GPU command submission, and surface presentation from the binary into a feature-gated `harbor-widget` runtime integration. The App remains the owner of windows and GPU resources and keeps Harbor-specific multi-window and paste policy, while each window uses an independent Runtime and borrowed frame target.

## Layers

The project's architectural layers confirmed during decomposition are:

1. **Runtime Host** — `src/` resource ownership, winit application lifecycle, multi-window routing, business policy, and fatal-error handling.
2. **Winit Runtime Integration** — feature-gated `harbor-widget` event adaptation, scheduling, frame execution, and presentation.
3. **Core Widget Runtime** — platform-independent event routing, invalidation, reconciliation, layout, scene, and Widget renderer.
4. **Terminal / Application Components** — `harbor-terminal` input/render behavior, CustomPaint, and paste-confirmation components.
5. **Verification** — crate tests, integration tests, and observable window behavior.

Every ticket includes all five layers and explicitly identifies layers that require no change.

## Dependency Graph

```text
T0001
  ↓
T0002
  ↓
T0003
  ↓
T0004
  ↓
T0005
  ↓
T0006
  ↓
T0007
```

### Blocking relationships

| Ticket | Blocks | Reason |
| --- | --- | --- |
| T0001 | T0002, T0003, T0004, T0005, T0006, T0007 | Every slice consumes the shared feature, effects, frame-target, invalidation, and outcome contracts. |
| T0002 | T0003 | Terminal scroll semantics must enter through the established winit-to-Runtime event path. |
| T0003 | T0004 | Both replace main-window branches in `src/app.rs`; sequencing prevents overlapping event-routing edits. |
| T0004 | T0005 | Frame presentation consumes runtime-owned redraw and frame-completion scheduling. |
| T0005 | T0006 | Surface recovery extends the established happy-path acquire/render/submit/present flow. |
| T0006 | T0007 | Confirmation presentation reuses the completed viewport and surface policy. |
| T0007 | — | Final multi-window slice. |

### Parallel groups

None. The slices share `src/app.rs`, the winit integration contract, or the same frame lifecycle; sequential execution avoids contract and file conflicts.

## Recommended Order

1. T0001 — Runtime Integration Foundation
2. T0002 — Winit Events Reach Widget Runtime
3. T0003 — Terminal Owns Scrollback Input Semantics
4. T0004 — Runtime-Owned Wake and Idle Scheduling
5. T0005 — Main Window Runtime Presentation
6. T0006 — Viewport and Surface Recovery
7. T0007 — Confirmation Window Uses the Same Integration

## Ticket Index

| Ticket ID | File | Title | Summary |
| --- | --- | --- | --- |
| T0001 | [T0001-runtime-integration-foundation.md](./T0001-runtime-integration-foundation.md) | Runtime Integration Foundation | Establish shared winit integration contracts and resolve the governing ADR boundary. |
| T0002 | [T0002-winit-events-reach-widget-runtime.md](./T0002-winit-events-reach-widget-runtime.md) | Winit Events Reach Widget Runtime | Move stateful event conversion from `src/` into `harbor-widget`. |
| T0003 | [T0003-terminal-owns-scrollback-input-semantics.md](./T0003-terminal-owns-scrollback-input-semantics.md) | Terminal Owns Scrollback Input Semantics | Route navigation and wheel events generically and interpret them in `harbor-terminal`. |
| T0004 | [T0004-runtime-owned-wake-and-idle-scheduling.md](./T0004-runtime-owned-wake-and-idle-scheduling.md) | Runtime-Owned Wake and Idle Scheduling | Move redraw coalescing and event-loop control-flow policy into Runtime. |
| T0005 | [T0005-main-window-runtime-presentation.md](./T0005-main-window-runtime-presentation.md) | Main Window Runtime Presentation | Let runtime integration acquire, render, submit, and present the main frame. |
| T0006 | [T0006-viewport-and-surface-recovery.md](./T0006-viewport-and-surface-recovery.md) | Viewport and Surface Recovery | Handle resize, DPI, zero-size, and recoverable surface outcomes inside the integration. |
| T0007 | [T0007-confirmation-window-uses-same-integration.md](./T0007-confirmation-window-uses-same-integration.md) | Confirmation Window Uses the Same Integration | Apply the event and presentation boundary to the separate confirmation window. |

## ADR Note

ADR 0004 still assigns shared RenderPass orchestration to the binary, while ADR 0015 assigns complete frame presentation to runtime integration. T0001 must mark ADR 0004 as superseded by ADR 0015 before implementation proceeds; no ticket should implement the obsolete binary orchestration boundary.
