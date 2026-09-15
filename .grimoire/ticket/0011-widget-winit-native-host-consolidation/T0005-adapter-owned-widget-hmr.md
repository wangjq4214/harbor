# Adapter-Owned Widget HMR Lifecycle

**Ticket ID:** T0005
**Source:** [Spec: 0012-widget-winit-native-host-consolidation](../../spec/0012-widget-winit-native-host-consolidation.md)
**Status:** In Progress

## Goal

Optional Windows debug Widget HMR is a reusable `harbor-widget::winit` host capability: the adapter owns reload observation, safe teardown barriers, generation activation, root replacement, Runtime update, and redraw while Harbor supplies only the application root factory and Host-owned state handles.

## Surfaces

- `crates/harbor-widget/Cargo.toml` and `crates/harbor-widget/src/winit/`: optional HMR dependency, lifecycle state, UI-thread wake bridge, root replacement, and tests.
- Workspace/root feature wiring and `src/hot_reload.rs`, `src/event.rs`, `src/shell.rs`: reduce application HMR code to root-factory/configuration and adapter work routing.
- `crates/harbor-app/`: fixed static/dynamic root contract and Host-owned Store/Dispatcher inputs.
- HMR feasibility, lifecycle, callback, and static-parity tests.

## Approach

1. Move `hot-lib-reloader` ownership and generic observer/lifecycle state behind an optional widget winit/HMR feature, retaining the existing Windows debug-only activation boundary.
2. Accept an application-supplied reloadable/static root factory and stable Host-owned input handles without importing Harbor actions or UI types.
3. Encapsulate background observation and UI-thread wake work so the application event channel transports adapter work but does not implement generation ordering.
4. Before releasing the reload barrier, cancel or isolate stale input ownership, remove the old root, run the required Runtime teardown/update work, and destroy old Views, Fibers, registrations, and callbacks while their library generation remains loaded.
5. Activate only a valid replacement generation, install one new root, update through the ordinary host scheduler, and request redraw.
6. Preserve Store state, application models, windows, shared GPU resources, terminals, PTYs, and event-turn action semantics; reset Widget/Fiber-local state.
7. Keep static builds on the same host/root contract without an observer.

## Dependencies and Coordination

### Blocked by

- T0003 — Integrated acceptance requires the real application root to live in the adapter-owned main host.

### Blocks

- T0006 — Application-owned generic HMR code and feature wiring cannot be removed until replacement is proven.

### Coordination

- Reuse the stable root inputs established during T0003; do not create a second application state/action transport.
- HMR wake events must coexist with terminal output and other application events without leaking blocker or loader types into business reducers.

## Acceptance

- [x] Generic reload observation, lifecycle state, teardown barrier, Runtime root replacement, and redraw scheduling reside in the optional `harbor-widget::winit` HMR capability.
- [x] Harbor application code supplies root factories and Host-owned state handles but does not coordinate prepare/ready generations or barrier release.
- [ ] Old Views, Fibers, external draw/schedule registrations, pointer/focus ownership, and callbacks are unreachable before the old dynamic generation unloads.
- [ ] A valid generation installs exactly one replacement root and redraws without recreating Window, Surface, Instance, Adapter, Device, Queue, terminal sessions, or PTYs.
- [x] Widget/Fiber-local state resets; Store-published application state remains available; Dispatcher actions retain FIFO drain-once behavior.
- [x] Failed builds, rapid reloads, startup without a session, closed event proxies, and application exit cannot deadlock reload or activate invalid code.
- [x] Shared contract/signature/layout changes remain documented as requiring application restart.
- [x] Default/non-HMR and unsupported-target builds contain no active observer and preserve static behavior.
- [ ] Repeated real Windows reloads pass the existing TypeId, duplicate dependency, tracing, callback lifetime, and library-generation feasibility gate.


## Verification Evidence

Automated verification passes for the adapter lifecycle and static parity:

- `cargo test -p harbor-widget --features hmr`
- `cargo build -p harbor-app-ui` followed by `cargo test -p harbor --features widget-hot-reload`
- `cargo check -p harbor --features widget-hot-reload --release`
- `cargo test --workspace`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `git diff --check`

The ticket remains **In Progress** because the interactive Windows watcher has not yet been run through repeated real DLL generations. Consequently, the unload-time reachability proof, no-resource-recreation observation, and final feasibility gate remain unchecked above.
## Out of Scope

- Hot-reloading `harbor-widget`, terminal/PTY implementations, or generic Runtime/rendering code.
- Preserving Widget/Fiber hook state across generations.
- Release-mode or cross-platform HMR guarantees.
- A message bus, reducer framework, FFI framework, or versioned serialization contract.
