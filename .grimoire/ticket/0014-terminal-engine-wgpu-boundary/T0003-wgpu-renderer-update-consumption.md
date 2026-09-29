# wgpu Renderer Update Consumption

**Ticket ID:** T0003
**Source:** [Spec 0016](../../spec/0016-terminal-engine-wgpu-renderer-boundary.md), [ADR-0045](../../adr/0045-gpu-independent-terminal-core-boundary.md), [#176](https://github.com/wangjq4214/harbor/issues/176)
**Status:** In Progress

## Goal

The current concrete wgpu renderer draws from a coherent engine update, never mutates parser/screen state, and safely recovers full or incremental rendering when draws are skipped or fail.

## Affected Surfaces

- **Renderer orchestration:** `crates/harbor-terminal/src/render/{pipeline,text,background,decoration,selection,cursor,scrollbar}.rs` and integration with the T0001 update boundary.
- **Projection/appearance:** Render viewport, DPI, palette, selection bounds, cursor, scrollbar and preedit overlay handling.
- **GPU initialization:** Existing host-injected GPU access and failure return path, coordinated with T0002 session ownership.

## Approach

Use the existing wgpu pipeline, `TerminalSnapshot` and `UpdateDamage` concepts; avoid a universal renderer interface. A snapshot read does not acknowledge visual changes. A renderer may acknowledge an update only once its retained state is sufficient to draw it; hidden, skipped or failed draws must permit replay or reconstruction. Preserve incremental/full upload behavior and invalidate affected projections on resize, viewport/DPI or palette changes. Keep blink presentation separate from engine-owned timing and preserve live/retained drawing.

## Dependencies and Coordination

- **Blocked by:** T0001 provides a GPU-free update and timing interface that this renderer must consume.
- **Blocks:** T0004 cannot migrate the application draw path before the renderer handles the new boundary.
- **Coordination risks:** T0002 also changes terminal construction; align GPU init failure and PTY endpoint ownership, but shared files alone do not require serializing the tickets.

## Acceptance

- [x] Rendering and upload paths receive an explicit coherent update, with no mutable parser/screen state access from the renderer.
- [ ] Focused tests exercise correct full and incremental uploads; changes to resize, DPI/viewport and palette invalidate the affected layers, while selection, cursor, scrollbar and preedit remain correct.
- [ ] Output accumulated during hidden or skipped drawing and a failed draw appears fully on the next usable live frame; a snapshot read alone never marks it consumed.
- [ ] Retained drawing, cursor blink phase and appearance remain compatible with the existing renderer; the core-only target still excludes GPU dependencies.
- [ ] Renderer initialization errors release partially created GPU resources; integration with session endpoint ownership and host failure policy is verified in T0004.

## Out of Scope

- A second renderer, software/SVG output, renderer marketplace or generic `dyn TerminalRenderer`/GAT pass API.
- Moving device, window, surface, frame submission or presentation ownership into the terminal core.
