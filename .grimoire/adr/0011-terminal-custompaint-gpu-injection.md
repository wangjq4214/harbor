# Terminal Renders via CustomPaint GPU Injection

**Status:** Completed
**Date:** 2025-07-28

## Context

`harbor-terminal` must own terminal state (screen grid, parser, selection) and GPU rendering (text atlas, cursor, scrollbar, background), but must NOT own the GPU device/surface or the window. The widget tree handles layout, hit testing, and event routing. The terminal is a pure engine injected into a `CustomPaint` widget. Alternatives:

- **Terminal implements AnyView directly** — couples terminal to widget trait, blurs "escape hatch" semantics
- **Runtime stores a single global external draw callback** — doesn't scale to multiple custom paint widgets (split panes, future overlays)
- **App wires encode/each-frame callbacks** — leaks orchestration to the binary layer

## Decision

1. `CustomPaint` wraps `Terminal` as an opaque engine. `CustomPaint` remains the explicit "escape hatch" marker.
2. During `build()`, `CustomPaint` registers a `Box<ExternalDrawFn>` into `BuildCx` under its `ExternalDrawId`.
3. `Runtime` stores a `HashMap<ExternalDrawId, Box<ExternalDrawFn>>` and invokes the correct handler when encoding `Primitive::External` items. No per-frame callback parameter needed.
4. `Terminal` does NOT hold `GpuContext`. `CustomPaint` receives the GPU context separately and injects it into `Terminal.render()` at draw time.

## Consequences

- `harbor-widget` gains one method on `BuildCx` (registration) and one field on `Runtime` (handler map).
- Multiple `CustomPaint` widgets coexist — each registers its own handler under its own ID.
- The binary crate (`src/app.rs`) has zero glue code: create `Terminal`, wrap in `CustomPaint::new(draw_id)`, set as root.
- The "escape hatch" semantics are explicit and auditable via `CustomPaint` usage.
