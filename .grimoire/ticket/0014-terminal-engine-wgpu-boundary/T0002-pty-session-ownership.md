# PTY Session Ownership

**Ticket ID:** T0002
**Source:** [Spec 0016](../../spec/0016-terminal-engine-wgpu-renderer-boundary.md), [ADR-0045](../../adr/0045-gpu-independent-terminal-core-boundary.md), [ADR-0043](../../adr/0043-acknowledged-pty-resize-barrier.md)
**Status:** Done

## Goal

A terminal session adapter owns PTY endpoints, reader coordination, resize and teardown independently of GPU resources, while the logical engine retains terminal state and protocol behavior.

## Affected Surfaces

- **Session I/O and lifecycle:** `crates/harbor-terminal/src/{lib,io}.rs` and existing `harbor-pty` endpoint/control capability boundary.
- **Resize integration:** Existing PTY barrier, prepared screen resize and reader reaping paths.
- **Ownership documentation and tests:** Map engine, session adapter, renderer, host and teardown responsibilities; test endpoint ownership across construction, resize, failures and close.

## Approach

Keep the engine/session separation compatible with the T0001 update interface. Preserve the acknowledged resize ordering and PTY/model failure consistency from ADR-0043 rather than changing transport or spawning another parser thread. Retain a facade where needed for existing application callers during migration, without committing to its permanent existence. Coordinate the renderer-init failure seam with T0003: endpoints must remain safely owned or be torn down when GPU initialization fails.

## Dependencies and Coordination

- **Blocked by:** T0001's engine boundary is required to distinguish logical state ownership from session/PTY effects at integration.
- **Blocks:** T0004 cannot finish caller migration without a session owner that safely constructs, resizes and closes PTY resources.
- **Coordination risks:** T0003 touches construction and failure paths in `lib.rs`; implementations can proceed in parallel against the shared ownership contract, then test integrated failure handling before T0004 is accepted.

## Acceptance

- [ ] An ownership map identifies the logical engine, PTY session adapter, concrete renderer, application host and exact teardown responsibilities, including failure before a session is fully started.
- [ ] Session-owned endpoints/readers are safely released on close and construction failure; no leaked reader or premature close of a still-live hidden tab.
- [ ] Existing acknowledged PTY resize barrier, bytes-before/after ordering, minimum geometry, model/PTY consistency on failure and retry semantics remain covered by focused tests.
- [ ] Engine state/input/selection and PTY I/O behavior remain compatible for existing facade callers during migration; the core-only dependency boundary remains intact.

## Out of Scope

- PTY transport redesign, a new parser thread or changes to the screen-storage/reflow contract.
- Final application widget migration and Windows runtime evidence (T0004 and T0005).
