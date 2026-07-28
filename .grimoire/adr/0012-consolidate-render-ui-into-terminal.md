# Consolidate harbor-render and harbor-ui into harbor-terminal

**Status:** Implementing
**Date:** 2025-07-28

## Context

The project has five crates in the terminal rendering chain: `harbor-parser`, `harbor-terminal`, `harbor-render`, `harbor-ui`, `harbor-widget`. The user wants to simplify: one crate for terminal state + rendering, one for parsing, one for the widget framework. Alternatives:

- **Keep all five** — many small crates but hard to follow the rendering flow
- **Merge only harbor-ui** — still leaves render/terminal boundary unclear
- **Merge all into one** — loses the clean parser/widge separation

## Decision

Delete `harbor-render` and `harbor-ui` crates. Merge their responsibilities into `harbor-terminal`:

- Terminal state (Screen, selection, damage tracking) stays in `harbor-terminal`
- GPU rendering (Background, Cursor, Decoration, Scrollbar, Selection, Text components) moves into `harbor-terminal`
- Modal dialog logic (`harbor-ui`) moves to `harbor-widget` or is reimplemented as widgets
- `harbor-terminal` depends on `wgpu`, `harbor-parser`, `harbor-text` for the glyph atlas

## Consequences

- Three crates remain in the terminal domain: `harbor-parser` → `harbor-terminal` → consumed by app via `harbor-widget` CustomPaint
- `harbor-terminal` gains a `wgpu` dependency (for pipelines, buffers, render passes)
- `harbor-terminal` does NOT depend on `winit` — window management stays in the binary crate
- The Component trait and EventResult type from `harbor-render` move to `harbor-terminal` or dissolve
- Confirmation dialogs (`harbor-ui`) need a widget-based reimplementation
