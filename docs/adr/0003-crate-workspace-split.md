# Split monolithic `harbor` into a Cargo workspace

Harbor is currently a single `harbor` crate. We will split it into an 8-crate Cargo workspace with strict unidirectional dependencies, following domain boundaries. The primary motivations are **reusability** (terminal, parser, and render as independent libraries) and **boundary enforcement** (pub/privacy prevents coupling creep).

## Crate map

```
Layer 0 — zero dependencies
  harbor-types      Cell, CellAttrs, Color, CursorShape, SelectionBounds,
                    TerminalSize, InputModes
  harbor-config     FONT_SIZE, TEXT_PADDING, BACKGROUND, BLINK_INTERVAL_MS,
                    SCROLLBAR_*, SELECTION_COLOR (all native types, no wgpu)
  harbor-parser     ANSI/VT event-driven parser; outputs ParseEvent stream

Layer 1 — domain core
  harbor-terminal → types + parser
                    Screen, NormalBuf, DamageTrack, SelectionModel, paste_bytes()
  harbor-pty      → types
                    WakeHandler trait, Pty, platform implementations

Layer 2 — presentation
  harbor-render   → types + config + wgpu + fontdb + fontdue
                    GpuContext, Component trait, Text, Cursor, Scrollbar,
                    Background, Decoration, FontBook, RenderSnapshot
  harbor-ui       → render + winit
                    ModalContent trait, Dialog<M>, DialogButton primitives

Layer 3 — assembly
  harbor (bin)    → terminal + pty + render + ui + winit
                    App, UiRoot, InputEncoder, PasteContent, snapshot conversion
```

Dependency direction is strictly Layer 3 → 2 → 1 → 0. No cycles.

## Key architectural decisions within the split

- **send_paste** reversed dependency resolved: terminal provides `paste_bytes(modes, text) -> Vec<u8>`; app writes result to PTY.
- **PTY→app coupling**: WakeHandler trait instead of direct `EventLoopProxy<AppEvent>`.
- **Render→Terminal decoupling**: RenderSnapshot data projection; render never references `Screen`.
- **Selection ownership**: SelectionModel (word boundaries, click chains, range calculation) lives in `harbor-terminal`; only colored overlay rendering lives in `harbor-render`.
- **Parser independence**: `harbor-parser` emits `ParseEvent` stream; zero dependencies.
- **Config purity**: `harbor-config` uses `[f64; 4]` instead of `wgpu::Color` to remain dependency-free.

## Considered alternatives

- **Fewer crates** (e.g. types+config merged, parser inside terminal): rejected to keep zero-dependency surface minimal and enable independent parser reuse.
- **No RenderSnapshot — render depends on Screen directly**: rejected; couples render to terminal internals, preventing independent evolution.
- **PTY holds EventLoopProxy directly (current state)**: rejected; would force `harbor-pty` to depend on winit and the binary crate's event type.
