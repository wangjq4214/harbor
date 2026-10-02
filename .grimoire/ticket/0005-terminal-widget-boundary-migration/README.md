# Terminal/Widget Boundary Migration

**Source:** [Spec: 0005-terminal-widget-boundary-migration](../../spec/0005-terminal-widget-boundary-migration.md)
**Ticket folder:** `.grimoire/ticket/0005-terminal-widget-boundary-migration/`

## Overview

These tickets remove the `harbor-terminal` → `harbor-widget` dependency without changing observable terminal rendering, input, scrollback, cursor, PTY, or cross-window gate behavior. `harbor-terminal` gains its own render and input contracts; a root-level `TerminalWidgetBridge: Component` adapts the existing `CustomPaint` integration. The App remains the Runtime Host and retains lifecycle, deferred-input drain, and cross-window policy.

The tickets follow ADRs 0005, 0009, 0011, and 0015. The pre-existing ADR 0012 constraint against terminal's winit dependency conflicts with the explicitly scoped source spec; this work retains winit only as the recorded temporary phase constraint and does not broaden its use.

## Layers

The project's architectural layers (confirmed during decomposition):

1. **Terminal engine (`harbor-terminal`)** — terminal-owned render/input boundaries, wgpu rendering, and PTY semantics.
2. **Widget external-paint (`harbor-widget`)** — `CustomPaint`, external-draw registration, and deferred widget input.
3. **Runtime Host / bridge (`src/`)** — `TerminalWidgetBridge`, terminal lifecycle, input drain, and cross-window policy.
4. **Verification** — terminal/unit tests, root integration tests, and workspace dependency validation.

Every ticket cuts through all confirmed layers.

## Dependency Graph

### Blocking relationships

| Prerequisite | Consumers | Reason |
| --- | --- | --- |
| [Spec 0005 terminal boundary and bridge contracts](../../spec/0005-terminal-widget-boundary-migration.md#solution) | T0003 | Input consumes `TerminalEvent` and targets the bridge-owned external draw identifier. |
| T0003 | — | Final slice completes input behavior and removes the dependency. |

### Parallel groups

None. T0003 is the only retained ticket in this group.

## Recommended Order

1. T0003 — bridged input, cross-window gate preservation, and dependency elimination, using the Spec 0005 boundary and bridge contracts.

## Ticket Index

| Ticket ID | File | Title | Summary |
| --- | --- | --- | --- |
| T0003 | [T0003-bridge-input-and-remove-widget-dependency.md](./T0003-bridge-input-and-remove-widget-dependency.md) | Bridge input and remove widget dependency | Preserve routed input and gate behavior while eliminating all terminal widget dependencies. |

## Historical Contract Destinations

The following stable IDs identify retired planning slices; their contracts remain in the source spec. This mapping does not close T0003's unresolved input-policy or acceptance work.

| Historical ID | Surviving contract |
| --- | --- |
| 0005/T0001 | [Terminal-owned `RenderTarget` and `TerminalEvent` boundary](../../spec/0005-terminal-widget-boundary-migration.md#solution) |
| 0005/T0002 | [Root-level bridge Component and external-paint integration](../../spec/0005-terminal-widget-boundary-migration.md#use-a-root-level-bridge-component) |
