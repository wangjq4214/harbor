# Application-Layer Widget Hot Reload

**Spec ID:** 0011
**Status:** Draft
**Date:** 2026-09-12

## Requirement

Harbor must support a Windows debug-development mode that reloads application-level widget composition without restarting the Runtime Host, window, GPU resources, terminal sessions, PTYs, or Host-owned application models.

Reloading must replace the widget root and rebuild Widget/Fiber hook state. Widget actions such as `on_click` must continue to reach the Host through the existing `Store<S, A>` and `Dispatcher<A>` event-turn action transport; HMR must not introduce a general-purpose message bus.

## Solution

Place the application-level widget composition behind a debug-only dynamic-library boundary managed by `hot-lib-reloader`. The Runtime Host remains responsible for the winit event loop, Runtime ownership, window and GPU lifetimes, terminal and PTY resources, application models, action reduction, and platform effects.

A background reload observer reports reload lifecycle transitions to the UI thread through dedicated variants of the existing Host `AppEvent` channel. Before activating a new library generation, the UI thread replaces or removes the old Runtime root so its Views, Fibers, external registrations, and callbacks—including `on_click` closures—are dropped. After reload, the Host asks the new application UI generation for a replacement root, installs it in the existing Runtime, processes the resulting Runtime effects, and requests redraw through the existing winit integration.

Reloadable widgets continue to subscribe to Host-published state through `Store::watch` and capture only `Dispatcher<A>` in action callbacks. The Host continues to drain and reduce actions at its established event-turn boundary. Types shared across the executable and dynamic library are treated as a fixed development-session contract: changing their function signatures or memory layouts requires restarting and rebuilding the Host.

The non-HMR build path remains free of reload observation and uses the normal statically linked application UI path. Changes to `harbor-widget` Runtime, reconciliation, layout, renderer behavior, or the shared Host/UI action contract are not hot-reloaded by this mode.

### Seams

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Dynamic UI boundary | Runtime Host ↔ reloadable application UI library | A successfully built Windows debug dynamic library and fixed shared function/type layouts for the current Host process | A replacement application widget root while leaving Host-owned resources and models outside the library |
| Reload lifecycle | `hot-lib-reloader` observer → Host `AppEvent` loop | Reload lifecycle notifications may originate off the UI thread | UI-thread serialization of old-root teardown, new-root installation, Runtime update, and redraw |
| Widget state publication | Runtime Host → reloadable widgets | Host-owned state is published through the existing `Store<S, A>` | Declarative state reads through `Store::watch` without transferring model ownership |
| Widget action transport | Reloadable widgets → Runtime Host | Callbacks capture the existing write-only `Dispatcher<A>` and keep the action contract unchanged during a development session | FIFO one-shot widget intent for Host-owned reduction and effects, including intent emitted by `on_click` |
| Runtime replacement | Runtime Host ↔ Harbor Widget Runtime | Old reloadable Views, Fibers, registrations, and callbacks are dropped before the replacement generation becomes active | Root replacement and redraw without replacing the Runtime, window, GPU resources, terminal sessions, or PTYs |

## End-to-End Tests

### E2E: Reload application UI without restarting Host resources

- **Given:** Harbor is running on Windows in HMR debug mode with an active terminal session.
- **When:** Application-level widget layout, styling, or composition is changed and the reloadable library builds successfully.
- **Then:** The running Host installs the new widget root and redraws it without recreating the process, window, GPU resources, terminal session, or PTY; Host-owned tab and terminal state remains available.

### E2E: Widget-local state resets on reload

- **Given:** The active widget tree contains Fiber or `use_state` state and Host-owned application state.
- **When:** A new UI library generation is loaded.
- **Then:** The old root is unmounted, widget-local state starts from the new root's initial state, and Host-owned application state is retained and republished to the new tree.

### E2E: Actions continue through Dispatcher after reload

- **Given:** A reloaded widget has an `on_click` callback that dispatches an existing application action.
- **When:** The user activates the widget after reload.
- **Then:** The callback enqueues the action through `Dispatcher`, the Host drains it at the normal event-turn boundary, and the existing Host-owned reducer and effects execute exactly once.

### E2E: Old callbacks cannot run after generation replacement

- **Given:** The old widget root contains callbacks and external registrations whose executable code belongs to the old dynamic-library generation.
- **When:** A reload lifecycle begins.
- **Then:** Root teardown completes on the UI thread before the replacement root is installed, and subsequent input can invoke only callbacks belonging to the active root generation.

### E2E: Ordinary build does not enable HMR

- **Given:** Harbor is built without the HMR development feature.
- **When:** The application starts and UI source files or dynamic-library outputs change.
- **Then:** No reload observer is active, no HMR lifecycle event is produced, and the normal statically linked UI behavior is unchanged.

### E2E: Shared contract change requires restart

- **Given:** An HMR Host is already running.
- **When:** a reload changes an exported function signature or the layout of a shared input, state, or action type.
- **Then:** The change is not accepted as an in-process compatibility guarantee; the development workflow requires rebuilding and restarting the Host before using the changed contract.

## Decisions

### Reload only application-level widget composition

- **Choice:** Use `hot-lib-reloader` only in Windows debug development, retain Host-owned resources and models, and rebuild the widget root and Fiber state on reload.
- **Reason:** This provides a short UI feedback loop without moving Runtime, window, GPU, terminal, or PTY ownership across an unstable Rust dynamic-library boundary.
- **ADR reference:** [0030-application-layer-widget-hot-reload](../adr/0030-application-layer-widget-hot-reload.md)

### Reuse Store and Dispatcher instead of adding a message bus

- **Choice:** Widget callbacks continue to emit typed intent through the existing `Dispatcher<A>` while the Host publishes state through `Store<S, A>`.
- **Reason:** The existing boundary already preserves FIFO action delivery and Host ownership of reducers and side effects; another bus would duplicate the same responsibility.
- **ADR reference:** [0029-widget-store-state-action-boundary](../adr/0029-widget-store-state-action-boundary.md)

### Preserve Runtime Host ownership

- **Choice:** Reload notifications enter through the Host event loop, while root replacement and Runtime mutation occur on the UI thread; reloadable code does not own window, surface, device, queue, terminal, or PTY lifetimes.
- **Reason:** This preserves the established per-window Runtime and borrowed platform-resource architecture.
- **ADR reference:** [0015-runtime-owned-frame-presentation](../adr/0015-runtime-owned-frame-presentation.md)

## Test Plan

- **Feasibility gate:** Before migrating the complete application UI, exercise a minimal reloadable root through repeated successful rebuilds on Windows. Verify that `TypeId`, duplicate dependency state, callback destruction, `tracing` callsite registration, and dynamic-library generation handling do not crash or corrupt the Runtime.
- **Integration tests:** Verify reload lifecycle events are handled on the UI thread; old roots are unmounted before replacement; Runtime effects are folded after replacement; Store state remains readable; Dispatcher actions retain FIFO, drain-once behavior.
- **Regression tests:** Run the existing Runtime root-replacement, external-registration, Store/Dispatcher, tab model, terminal allocation, input routing, and frame-presentation tests in the non-HMR configuration.
- **Manual Windows tests:** Keep a terminal process running while changing layout and `on_click` implementation; confirm terminal output, input, selection, tabs, window identity, and GPU presentation continue after reload.
- **Failure diagnostics:** Verify reload and root-replacement failures are visible through Host diagnostics without transferring fatal-error policy into `harbor-widget`.
- **Edge cases:** Reload while the window is minimized, while input focus or pointer capture exists, after a tab action is queued, and when old roots contain external terminal draw callbacks.

## Out of Scope

- Hot-reloading `harbor-widget` Runtime, reconciliation, layout, renderer, winit adapter, terminal engine, PTY implementation, or Host policy.
- Preserving or migrating Widget/Fiber hook state across library generations.
- Changing shared function signatures, input structures, state types, action enums, or other cross-library layouts without restarting the Host.
- Introducing a general application message bus, reducer framework, or effect system.
- Production or release-mode dynamic loading.
- Cross-platform HMR guarantees beyond the selected Windows debug-development mode.
- Function hot-patching through Subsecond or application integration through Dexterous Developer.

## Future Evolution

- Re-evaluate a versioned serialization or FFI-stable shared contract only if changing action or input schemas without restarting the Host becomes a requirement.
- Re-evaluate function hot-patching if dynamic-library `TypeId`, global-state, `tracing`, or callback-lifetime behavior fails the feasibility gate.
- Consider additional platform support only after the Windows lifecycle and safety contract is proven.
