# Terminal Absorb Rendering

**Ticket ID:** T0003
**Source:** [Spec: 0002-terminal-render-consolidation](../spec/0002-terminal-render-consolidation.md)
**Status:** Todo

## Goal

`harbor-terminal` absorbs all GPU rendering modules from `harbor-render` (Text, Background, Cursor, Decoration, Scrollbar, Selection, GpuContext), making them internal implementation. Create a new `Terminal` struct holding Screen + all rendering components. `CustomPaint` wraps Terminal and registers its draw handler during `build()`. App layer replaces `UiRoot` with `CustomPaint::new(terminal.draw_id())`.

## Layers

- [ ] **Parser:** None — T0001 completed
- [ ] **Terminal:**
  - Migrate 7 modules from `harbor-render/src/` to `crates/harbor-terminal/src/`: `text.rs`, `background.rs`, `cursor.rs`, `decoration.rs`, `scrollbar.rs`, `selection.rs`, `gpu.rs`
  - Create `Terminal` struct: holds `Screen`, `TerminalParser`, all rendering components (`Text`, `Background`, `Cursor`, `Decoration`, `Scrollbar`, `Selection`)
  - `Terminal::render(draw_id, rect, pass, gpu)` — coordinates `prepare` + `draw` for all components
  - Create `ExternalDrawFn` closure during `new()`: `Box::new(move |id, rect, pass| terminal.render(id, rect, pass, &gpu))`
  - Cargo.toml: add `wgpu`, `harbor-text` (glyph atlas); remove `harbor-render` dependency
  - `GpuContext` stays in terminal; `EventResult` and `Component` trait move in or dissolve
- [ ] **Widget:**
  - `CustomPaint` accepts handler during construction; `build()` registers via `BuildCx`
- [ ] **App:**
  - `src/app.rs` — replace `UiRoot::new()` with `Terminal::new()` + `CustomPaint::new(draw_id)` + `runtime.set_root(...)`
  - `src/app/ui.rs` — inline or remove UiRoot orchestration (component lifecycle moves into Terminal)
  - `render_frame()` — remove UiRoot.prepare/draw, keep only `runtime.encode()`
  - `src/app/input.rs` — remove UiRoot.handle_event, route through widget Runtime instead

## Approach

### 3.1 Migrate render modules

1. Copy `harbor-render/src/{text,background,cursor,decoration,scrollbar,selection,gpu}.rs` into `crates/harbor-terminal/src/render/`.
2. Fix internal references: ensure no circular dependency from `harbor_terminal` → migrated modules.
3. `EventResult` and `Component` trait — move into terminal, keep private or dissolve into direct method calls.

### 3.2 Restructure Terminal struct

4. `crates/harbor-terminal/src/lib.rs` — new `Terminal` struct:
   ```rust
   pub struct Terminal {
       screen: Screen,
       parser: TerminalParser,
       text: Text,
       background: Background,
       cursor: Cursor,
       decoration: Decoration,
       scrollbar: Scrollbar,
       selection: Selection,
       draw_id: ExternalDrawId,
       // GpuContext NOT stored — injected at render time
   }
   ```
5. `Terminal::new(size, gpu, font_book, metrics)` — initialize Screen + all render components, allocate `draw_id`.
6. `Terminal::render(&mut self, draw_id: ExternalDrawId, rect: Rect, pass: &mut RenderPass, gpu: &GpuContext)` — replicate former `UiRoot` prepare-then-draw ordering.
7. `Terminal::draw_id(&self) -> ExternalDrawId` — for CustomPaint to consume.

### 3.3 Public API (final shape for this ticket)

```rust
impl Terminal {
    pub fn new(size: TerminalSize, gpu: &GpuContext, font_book: FontBook, metrics: TextMetrics) -> Self;
    pub fn draw_id(&self) -> ExternalDrawId;
    pub fn render(&mut self, draw_id: ExternalDrawId, rect: Rect, pass: &mut wgpu::RenderPass, gpu: &GpuContext);
    pub fn resize(&mut self, size: TerminalSize, gpu: &GpuContext);
    // PTY and input added in T0004
}
```

### 3.4 App adaptation

9. `src/app.rs` — `AppRuntime` struct: remove `ui: Option<UiRoot>`, add `terminal: Option<Terminal>`.
10. `resumed()` — create `Terminal::new()` → `CustomPaint::new(terminal.draw_id())` → `runtime.set_root(...)`.
11. `window_event()` — remove UiRoot.handle_event dispatch; defer to widget Runtime dispatch + drain (T0004).
12. `render_frame()` — remove `ui.prepare()` / `ui.draw()`, only call `runtime.encode(queue, pass, viewport)`.

## Blocked by

- T0001 — needs new VtHandler trait signature
- T0002 — needs BuildCx registration + parameter-free Runtime encode

## Blocks

- T0004 — PTY I/O and input events depend on Terminal struct existing

## Acceptance

- [ ] `harbor-terminal` compiles, containing text/background/cursor/decoration/scrollbar/selection/gpu modules
- [ ] `Terminal::new()` → `CustomPaint::new(draw_id)` → set root, static content (hardcoded bytes) renders via widget tree
- [ ] App no longer imports `harbor_render::*`
- [ ] `UiRoot` no longer instantiated (may remain undeleted; T0005 cleans up)
- [ ] `runtime.encode()` has no external_draw callback parameter — uses internal lookup
- [ ] `harbor-render` crate still exists but app no longer depends on it (T0005 deletes)

## Out of Scope

- PTY I/O reads and writes (T0004)
- Keyboard/mouse input events (T0004)
- Deleting harbor-render crate (T0005)
- Paste confirmation dialog widget reimplementation (T0005 and beyond)
- `harbor-text` font loading changes (keep as-is; harbor-terminal directly depends on harbor-text)
