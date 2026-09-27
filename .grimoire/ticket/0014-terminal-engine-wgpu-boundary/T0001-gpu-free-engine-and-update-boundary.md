# GPU-Free Engine and Update Boundary

**Ticket ID:** T0001
**Source:** [Spec 0016](../../spec/0016-terminal-engine-wgpu-renderer-boundary.md), [ADR-0045](../../adr/0045-gpu-independent-terminal-core-boundary.md), [#176](https://github.com/wangjq4214/harbor/issues/176)
**Status:** Done

## Goal

A terminal engine owns logical state, input, selection, transient preedit, damage and timing and can compile/test without GPU/widget dependencies while exposing a coherent read-only renderer update boundary.

## Affected Surfaces

- **Core model and events:** `crates/harbor-terminal/src/{lib,screen,model,pointer,types}.rs` and related parser/input modules; isolate logic from renderer ownership.
- **Timing:** Move blink/frame-demand state needed by the engine out of GPU-only cursor ownership while preserving the existing host-neutral scheduling behavior.
- **Build configuration:** `crates/harbor-terminal/Cargo.toml` and module exports; supply a core-only build/test target whose actual dependency graph excludes wgpu, arboard, winit and `harbor-widget`.
- **Update boundary:** Preserve `TerminalSnapshot` and `UpdateDamage` semantics along with selection, preedit and appearance information without exposing mutable screen/parser internals.

## Approach

Start with feature isolation inside the existing crate. Do not claim a core-only target merely because `new_headless` exists; compile and inspect its transitive dependency graph. If this cannot reliably establish the required boundary, a core crate is allowed by ADR-0045. Taking a snapshot must not itself consume visual changes; the update boundary must support recovery after skipped or failed drawing. Keep the concrete renderer and existing application path building during migration.

## Dependencies and Coordination

- **Blocked by:** None; #169 and #170 are closed prerequisites whose behavior must be preserved.
- **Blocks:** T0002 and T0003 integration needs the GPU-free engine/update and timing interface. These consumers may coordinate on the agreed contract while work is in progress.
- **Coordination risks:** `lib.rs` currently owns I/O and renderer together; coordinate ownership changes with T0002 and T0003, and avoid premature removal of the compatibility path.

## Acceptance

- [x] Before changing the existing render/session boundary, capture a reproducible baseline dependency graph and one-/multi-session performance measurements for T0005's comparison.
- [x] Core-only check and tests compile and exercise parser, screen state, input, selection, damage and timing without wgpu, arboard, winit or `harbor-widget` in the dependency graph; record the reproducible commands and graph.
- [x] A coherent engine update is available without a mutable parser/screen handle; reading it does not clear unconsumed visual changes.
- [x] Frame demand and cursor blink reset still work without creating GPU resources, including input/cursor movement, preedit and synchronized-output eligibility behavior.
- [x] Existing rendered configuration and facade callers continue to build during migration; no PTY/resize or visual semantics change is introduced by the isolation itself.

Evidence: [T0001 core-boundary verification](../../../docs/verification/terminal-core-boundary-t0001.md). The recorded performance baseline is a CPU-side proxy; GPU/ConPTY runtime and post-T0004 comparison remain T0005 work.

## Out of Scope

- Migrating PTY lifetime ownership or all application callers (T0002 and T0004).
- Introducing a universal renderer abstraction or a second backend.
