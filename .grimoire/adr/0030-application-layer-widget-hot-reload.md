# Reload Application-Layer Widgets at the Runtime Host Boundary

**Status:** Proposed
**Date:** 2026-09-12

## Context

Harbor needs a faster Windows UI-development loop without restarting the window, GPU stack, terminal sessions, or PTYs. Alternatives were reloading the entire `harbor-widget` Runtime implementation, using experimental function hot-patching, or limiting dynamic reload to application-level widget composition.

## Decision

Use `hot-lib-reloader` in stable-Rust Windows debug builds to reload only Harbor's application-level widget composition at the Runtime Host boundary. On reload, keep Host-owned resources and models alive, replace the Runtime root, and rebuild Widget/Fiber hook state rather than migrating Rust state across the dynamic-library boundary. Continue using the existing `Store<S, A>` and `Dispatcher<A>` event-turn action transport for widget intent, including `on_click`; do not add a general message bus. A background reload observer notifies the UI thread through dedicated `AppEvent` lifecycle events so the old root and its callbacks are dropped before the new library generation builds a replacement root.

## Consequences

- UI layout, styling, and application-level widget composition can change without restarting the terminal sessions or GPU/window infrastructure.
- Runtime, reconciliation, layout, and renderer implementation changes still require a normal process restart.
- Reloadable code must live behind an explicit dynamic-library boundary enabled only for debug HMR builds.
- The Host must discard the old root and all reloadable callbacks before activating the new library generation.
- Widget-local `use_state` and Fiber state reset after each reload; durable application state remains Host-owned.
- Existing action-contract type layouts remain fixed while the Host is running; changing them requires a Host restart unless a future versioned serialization boundary is introduced.
- HMR adds only reload lifecycle events to the Host event channel, not a new domain messaging abstraction.
