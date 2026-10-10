# Widget Winit Native Host Consolidation

**Source:** [Spec: 0012-widget-winit-native-host-consolidation.md](../../spec/0012-widget-winit-native-host-consolidation.md)
**Decision:** [ADR: 0031-widget-winit-adapter-owns-native-host-infrastructure.md](../../adr/0031-widget-winit-adapter-owns-native-host-infrastructure.md)
**Ticket folder:** `.grimoire/ticket/0011-widget-winit-native-host-consolidation/`

## Overview

Two tickets remain open. The shared GPU boundary, the adapter-owned per-window host, and the main and confirmation window migrations are implemented; their contracts are recorded in the source spec and the historical destinations below. The open tickets cover adapter-owned HMR and the legacy cleanup gate.

## Dependency Graph

```text
Spec 0012 Solution (adapter-owned host; historical T0001–T0004)
  ├──> T0005 Adapter-owned Widget HMR Lifecycle
  └──> T0006 Legacy Host Cleanup and Acceptance <── T0005
```

## Recommended Order

1. T0005 — Adapter-Owned Widget HMR Lifecycle
2. T0006 — Legacy Host Cleanup and Acceptance

## Requirement Coverage

| Spec requirement | Tickets |
| --- | --- |
| Configurable per-window Window/Surface/Runtime ownership | [Spec 0012 Solution](../../spec/0012-widget-winit-native-host-consolidation.md#solution); historical T0002, T0003 |
| Generic wgpu ownership and cross-window sharing | [Spec 0012 Solution](../../spec/0012-widget-winit-native-host-consolidation.md#solution); historical T0001 |
| Same boundary for main and confirmation windows | [Spec 0012 Test Plan](../../spec/0012-widget-winit-native-host-consolidation.md#test-plan); historical T0003, T0004 |
| Generic optional HMR lifecycle | T0005 |
| Safe root teardown and Host-state retention | T0005, T0006 |
| Terminal-specific GPU split without reverse dependency | [Spec 0012 Solution](../../spec/0012-widget-winit-native-host-consolidation.md#solution); historical T0001 |
| Application retains business/event-loop/fatal policy | [Spec 0012 Decisions](../../spec/0012-widget-winit-native-host-consolidation.md#decisions); T0005, T0006 |
| Duplicate binary orchestration removed | T0006 |

## Ticket Index

| Ticket | Title | Outcome |
| --- | --- | --- |
| [T0005](./T0005-adapter-owned-widget-hmr.md) | Adapter-Owned Widget HMR Lifecycle | Reload observation and safe root replacement leave the binary application |
| [T0006](./T0006-native-host-cleanup-acceptance.md) | Legacy Host Cleanup and Acceptance | Duplicate paths are removed and the complete static/HMR multi-window matrix is verified |

## Historical Contract Destinations

These mappings do not mark integrated acceptance passed.

| Historical ID | Contract destination |
| --- | --- |
| T0001 — Shared Widget GPU Foundation and Terminal Resource Split | [Spec 0012 Solution](../../spec/0012-widget-winit-native-host-consolidation.md#solution): shared Instance/Adapter/Device/Queue ownership in `harbor-widget::winit`; terminal pipelines stay in `harbor-terminal`. |
| T0002 — Adapter-Owned Per-Window Native Host | [Spec 0012 Solution](../../spec/0012-widget-winit-native-host-consolidation.md#solution): one configurable host owns Window, Surface, Runtime, events, scheduling, and presentation. Evidence in `crates/harbor-widget/tests/winit_contracts.rs`. |
| T0003 — Main Window Uses the Native Host | [Spec 0012 E2E](../../spec/0012-widget-winit-native-host-consolidation.md#e2e-main-window-starts-through-the-consolidated-host): main window created and driven through the host; `Shell` keeps business coordination. |
| T0004 — Confirmation Window Uses the Native Host | [Spec 0012 Test Plan](../../spec/0012-widget-winit-native-host-consolidation.md#test-plan): main/confirmation isolation, dialog ownership, placement and DPI, and repeated open/close cleanup. Evidence in `crates/harbor-widget/tests/native_host_smoke.rs` and `winit_contracts.rs`. |
