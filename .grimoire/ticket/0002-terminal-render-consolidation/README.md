# Terminal Render Consolidation

**Source:** [Spec: 0002-terminal-render-consolidation.md](../../spec/0002-terminal-render-consolidation.md)
**Ticket folder:** `.grimoire/ticket/0002-terminal-render-consolidation/`

## Overview

Merge `harbor-render` GPU rendering into `harbor-terminal`, making the terminal a self-contained engine (screen state + wgpu rendering + PTY I/O). The terminal renders into the widget tree via `CustomPaint`, with zero orchestration glue in the app layer. Delete `harbor-render` and `harbor-ui` crates. Shrink `harbor-parser`'s public API to three items.

## Layers

1. **Parser** — `harbor-parser`: VT byte stream → typed actions (VtHandler callbacks)
2. **Terminal** — `harbor-terminal`: Screen state + wgpu rendering pipelines + PTY I/O
3. **Widget** — `harbor-widget`: Widget tree, CustomPaint, Runtime, BuildCx
4. **App** — `src/`: Winit event loop, window, composition root

Every ticket cuts through all confirmed layers.

## Dependency Graph

```
T0001 ─┐
        ├─ (parallel) ─► T0003 ─► T0004 ─► T0005
T0002 ─┘
```

### Blocking relationships

| Ticket | Blocks | Reason |
|--------|--------|--------|
| T0001 | T0003 | Terminal depends on the new VtHandler trait signature |
| T0002 | T0003 | CustomPaint.build() calls BuildCx registration; Runtime needs the handler HashMap |
| T0003 | T0004 | PTY I/O writes into Terminal struct; depends on Terminal owning Screen + pipelines |
| T0004 | T0005 | Must confirm new path fully works before deleting old crates |

### Parallel groups

| Group | Tickets | Reason |
|-------|---------|--------|
| A | T0001, T0002 | Parser and Widget share no files; no runtime contract overlap |

## Recommended Order

1. T0001 ∥ T0002 — Pre-refactoring (parallel)
2. T0003 — Terminal absorbs rendering + CustomPaint wrapping
3. T0004 — PTY I/O + input events
4. T0005 — Cleanup

## Ticket Index

| Ticket ID | File | Title | Summary |
|-----------|------|-------|---------|
| T0001 | [T0001-parser-api-shrink.md](./T0001-parser-api-shrink.md) | Parser API Shrink | Shrink harbor-parser public API to three items: VtHandler, Params, Parser |
| T0002 | [T0002-widget-external-draw-registration.md](./T0002-widget-external-draw-registration.md) | Widget External Draw Registration | Add register_external_draw to BuildCx; Runtime holds HashMap<id, fn> |
| T0003 | [T0003-terminal-absorb-rendering.md](./T0003-terminal-absorb-rendering.md) | Terminal Absorb Rendering | Migrate 7 render modules, restructure Terminal, CustomPaint wrapper, app adapt |
| T0004 | [T0004-pty-io-and-input.md](./T0004-pty-io-and-input.md) | PTY I/O and Input Events | Internal reader thread, PTY writes, external input event path |
| T0005 | [T0005-cleanup.md](./T0005-cleanup.md) | Cleanup | Delete harbor-render/ui crates, remove Component trait, UiRoot, dead code |
