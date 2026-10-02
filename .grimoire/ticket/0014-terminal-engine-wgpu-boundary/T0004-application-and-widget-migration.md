# Application and Widget Migration

**Ticket ID:** T0004
**Source:** [Spec 0016](../../spec/0016-terminal-engine-wgpu-renderer-boundary.md), [ADR-0045](../../adr/0045-gpu-independent-terminal-core-boundary.md), [#176](https://github.com/wangjq4214/harbor/issues/176)
**Status:** In Progress

## Goal

The application uses the separated session and renderer without mixed ownership, preserving existing terminal, widget and host behavior. A migration-only compatibility facade may be removed after its callers are migrated.

## Affected Surfaces

- **App terminal integration:** `crates/harbor-app/src/{terminal_view,tab_manager,ui}.rs` and terminal construction/input/resize callers.
- **Widget bridge and host:** External paint/live-versus-retained draw, terminal event routing, frame-demand scheduling and host-managed GPU access.
- **Lifecycle:** Per-tab session retention, active/inactive tab presentation, close, startup and GPU initialization failure policy.

## Approach

Migrate callers from the facade to the separated owner(s) without leaving both the facade and application responsible for the same PTY or GPU resource. Keep facade compatibility while any caller needs it; removal is allowed only after all existing call paths and tests are migrated. Preserve the existing widget paint order and host surface ownership, and integrate the [session ownership contract's](../../../docs/architecture/terminal-session-ownership.md) construction/teardown constraints and T0003 renderer failure handling at the application boundary.

## Dependencies and Coordination

- **Blocked by:** Safe PTY endpoint/session ownership, resize ordering and teardown from [Spec 0016](../../spec/0016-terminal-engine-wgpu-renderer-boundary.md) and the [session ownership map](../../../docs/architecture/terminal-session-ownership.md), plus T0003's renderer consuming the engine update, are required for an integrated host path. The ownership map does not establish integrated failure/close acceptance; verify those constraints here and in T0005.
- **Blocks:** T0005 needs the migrated application to gather final runtime evidence.
- **Coordination risks:** Bridge/render changes span `terminal_view.rs`, tab lifetimes and frame scheduling; do not create an alternate source for blink demand or bypass the host's input/presentation policy.

## Acceptance

- [ ] All existing `harbor-app` and terminal widget call paths use one clear owner for engine state, PTY session and renderer; no resource is owned twice or leaked.
- [ ] Tab create, switch/hide, background output, resize/DPI, input/selection, cursor, scrollbar, preedit, wake/frame demand and retained/live drawing preserve observable behavior.
- [ ] GPU initialization failure leaves PTY endpoints safely released or correctly owned; close reaps reader and frees GPU resources with correct ordering.
- [ ] The facade works for any remaining transitional callers; if none remain, it can be removed without breaking the application or core-only target.
- [ ] Focused application/widget integration tests cover active versus inactive updates, resize retry, renderer failure where testable and restored drawing after skipped frames.

## Out of Scope

- A new UI feature, rendering backend, pane/search implementation or window/GPU ownership change.
- Retaining a compatibility facade as a permanent architectural requirement.
