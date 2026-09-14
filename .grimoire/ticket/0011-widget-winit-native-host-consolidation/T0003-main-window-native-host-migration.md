# Main Window Uses the Native Host

**Ticket ID:** T0003
**Source:** [Spec: 0012-widget-winit-native-host-consolidation](../../spec/0012-widget-winit-native-host-consolidation.md)
**Status:** Todo

## Goal

Harbor's main window is created and driven through the adapter-owned native host, leaving `Shell`/`ActiveSession` responsible for application state, terminal/tab/PTY coordination, platform policy, and fatal decisions rather than generic Widget infrastructure.

## Surfaces

- `src/shell.rs`, `src/effects.rs`, `src/event.rs`, `src/main.rs`: main-window bootstrap, event routing, idle/redraw flow, and error handling.
- `src/backdrop.rs`, `src/chrome.rs`, and icon/window configuration: application-provided platform hooks.
- `crates/harbor-app/`: static application-root inputs, Store/Dispatcher, and terminal bridge construction.
- Terminal resource initialization consuming the GPU boundary from T0001.

## Approach

1. Replace manual main Window, Surface, Runtime, `WinitAdapter`, `WinitFrameTarget`, configure closure, and frame orchestration with construction and delegation through the T0002 host.
2. Supply title, size, icon, theme, visibility, backdrop/caption setup, composition surface customization, and fallback painting through the application configuration/hook seam.
3. Initialize terminal-specific GPU resources from host-provided Device/Queue access and preserve CustomPaint registration.
4. Keep tab models, terminals, PTYs, Store action reduction, external invalidation sources, paste coordination, and fatal/exit policy in the application.
5. Route ApplicationHandler events and idle turns to the main host, then process only application-level outcomes/actions.
6. Preserve startup visibility/retry behavior and current backdrop transparency selection.

## Dependencies and Coordination

### Blocked by

- T0002 — Supplies the owned host contract and shared GPU integration.

### Blocks

- T0005 — Real application HMR must replace the root inside the migrated main host.
- T0006 — Legacy main-window paths cannot be removed before migration.

### Coordination

- May proceed in parallel with T0004, but both change ApplicationHandler routing and shared host error/access conventions.
- Coordinate root-input ownership with T0005 so static and reloadable factories consume the same Host-owned business handles.

## Acceptance

- [ ] First resume creates the main window, shared GPU resources, Surface, Runtime, and static root through `harbor-widget::winit` host construction.
- [ ] `Shell` and `ActiveSession` no longer own separate main-window Runtime, surface configuration, scheduler adapter, or manual frame target.
- [ ] Main event, redraw, resize/DPI, idle, cursor/IME/clipboard, and surface-recovery behavior remains observable and correct.
- [ ] Acrylic/backdrop tier selection, caption/icon behavior, transparency/fallback painting, delayed show, and startup retry behavior remain application-controlled and unchanged.
- [ ] Terminal tabs and PTYs survive ordinary redraw, resize, minimize/restore, and root update cycles.
- [ ] Store/Dispatcher actions are still drained and reduced exactly once by the application after Widget event routing.
- [ ] Unrecoverable host failures are returned to application fatal/exit policy.
- [ ] Static-build application tests and a Windows main-window smoke test pass.

## Out of Scope

- Confirmation-window migration.
- HMR lifecycle migration.
- Changing terminal, tab, paste, backdrop, or process-exit product behavior.
