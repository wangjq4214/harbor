# Widget Winit Native Host Consolidation

**Source:** [Spec: 0012-widget-winit-native-host-consolidation.md](../../spec/0012-widget-winit-native-host-consolidation.md)
**Decision:** [ADR: 0031-widget-winit-adapter-owns-native-host-infrastructure.md](../../adr/0031-widget-winit-adapter-owns-native-host-infrastructure.md)
**Ticket folder:** `.grimoire/ticket/0011-widget-winit-native-host-consolidation/`

## Overview

These tickets move reusable winit, wgpu, Runtime, presentation, and HMR infrastructure into `harbor-widget::winit`. The binary application retains EventLoop/ApplicationHandler coordination and Harbor business policy, while both the main and paste-confirmation windows use one configurable native-host boundary.

## Delivery Surfaces

- `crates/harbor-widget/src/winit/`: shared GPU resources, owned per-window host, event/effect integration, presentation, and optional HMR.
- `crates/harbor-terminal/src/render/`: terminal-only GPU pipelines and resources consuming adapter-provided GPU access.
- `src/shell.rs`, `src/dialog.rs`, `src/event.rs`, `src/hot_reload.rs`, `src/effects.rs`: migration from infrastructure ownership to business coordination.
- `crates/harbor-app/`: static and reloadable application root contracts and Host-owned state/action handles.
- Workspace manifests, integration/contract tests, and Windows runtime verification.

## Dependency Graph

```text
T0001 Shared GPU and terminal split
  └──> T0002 Adapter-owned per-window host
         ├──> T0003 Main-window migration ──> T0005 Adapter-owned HMR
         └──> T0004 Confirmation-window migration

T0003 ───────────────┐
T0004 ───────────────┼──> T0006 Cleanup and acceptance
T0005 ───────────────┘
```

T0001 establishes the generic GPU ownership required by the final host. T0002 establishes the adapter-owned Window/Surface/Runtime contract. T0003 and T0004 can proceed in parallel after T0002, but both touch application window routing and must coordinate shared host construction and error types. T0005 follows the migrated main-window host because its acceptance requires adapter-owned root replacement in the real application. T0006 removes compatibility paths only after both windows and HMR use the new boundary.

## Recommended Order

1. T0001 — Shared Widget GPU Foundation and Terminal Resource Split
2. T0002 — Adapter-Owned Per-Window Native Host
3. T0003 — Main Window Uses the Native Host
4. T0004 — Confirmation Window Uses the Native Host
5. T0005 — Adapter-Owned Widget HMR Lifecycle
6. T0006 — Legacy Host Cleanup and Acceptance

T0003 and T0004 may be implemented concurrently with explicit coordination around application event routing and shared-GPU access.

## Requirement Coverage

| Spec requirement | Tickets |
| --- | --- |
| Configurable per-window Window/Surface/Runtime ownership | T0002, T0003, T0004 |
| Generic wgpu ownership and cross-window sharing | T0001, T0002, T0004 |
| Same boundary for main and confirmation windows | T0003, T0004 |
| Generic optional HMR lifecycle | T0005 |
| Safe root teardown and Host-state retention | T0005, T0006 |
| Terminal-specific GPU split without reverse dependency | T0001 |
| Application retains business/event-loop/fatal policy | T0002–T0006 |
| Duplicate binary orchestration removed | T0006 |

## Ticket Index

| Ticket | Title | Outcome |
| --- | --- | --- |
| [T0001](./T0001-shared-widget-gpu-terminal-split.md) | Shared Widget GPU Foundation and Terminal Resource Split | Generic wgpu ownership moves to `harbor-widget` while terminal pipelines stay terminal-owned |
| [T0002](./T0002-adapter-owned-native-window-host.md) | Adapter-Owned Per-Window Native Host | One configurable host owns Window, Surface, Runtime, events, scheduling, and presentation |
| [T0003](./T0003-main-window-native-host-migration.md) | Main Window Uses the Native Host | Shell retains business coordination without manual main-window Widget infrastructure |
| [T0004](./T0004-confirmation-window-native-host-migration.md) | Confirmation Window Uses the Native Host | Independent confirmation UI uses the same host and shared GPU resources |
| [T0005](./T0005-adapter-owned-widget-hmr.md) | Adapter-Owned Widget HMR Lifecycle | Reload observation and safe root replacement leave the binary application |
| [T0006](./T0006-native-host-cleanup-acceptance.md) | Legacy Host Cleanup and Acceptance | Duplicate paths are removed and the complete static/HMR multi-window matrix is verified |
