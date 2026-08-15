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

| Ticket | Blocks | Reason |
| --- | --- | --- |
| T0001 | T0002, T0003 | Both observable paths consume the terminal-owned boundary contracts. |
| T0002 | T0003 | Input routing must target the bridge Component and its bridge-owned draw identifier. |
| T0003 | — | Final slice completes input behavior and removes the dependency. |

### Parallel groups

None. T0002 and T0003 share `TerminalWidgetBridge`, `src/app.rs`, and the terminal boundary contracts, so parallel work would overlap files and runtime contracts.

## Recommended Order

1. T0001 — terminal-owned boundary foundation (pre-refactoring)
2. T0002 — terminal rendering through `TerminalWidgetBridge`
3. T0003 — bridged input, cross-window gate preservation, and dependency elimination

## Ticket Index

| Ticket ID | File | Title | Summary |
| --- | --- | --- | --- |
| T0001 | [T0001-terminal-boundary-foundation.md](./T0001-terminal-boundary-foundation.md) | Terminal-owned boundary foundation | Establish widget-free render and input contracts shared by the migration slices. |
| T0002 | [T0002-render-through-terminal-widget-bridge.md](./T0002-render-through-terminal-widget-bridge.md) | Render through TerminalWidgetBridge | Render the terminal in the existing widget allocation through a root Component bridge. |
| T0003 | [T0003-bridge-input-and-remove-widget-dependency.md](./T0003-bridge-input-and-remove-widget-dependency.md) | Bridge input and remove widget dependency | Preserve routed input and gate behavior while eliminating all terminal widget dependencies. |
