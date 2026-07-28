# Widget External Draw Registration

**Ticket ID:** T0002
**Source:** [Spec: 0002-terminal-render-consolidation](../spec/0002-terminal-render-consolidation.md)
**Status:** Todo

## Goal

Add `register_external_draw` to `harbor-widget`'s `BuildCx`. `Runtime` internally holds `HashMap<ExternalDrawId, Box<ExternalDrawFn>>`. `CustomPaint` registers its handler during `build()`. `Runtime::encode()` no longer needs an `external_draw` parameter — it looks up handlers internally.

## Layers

- [ ] **Parser:** None — not involved
- [ ] **Terminal:** None — not involved (used starting in T0003)
- [ ] **Widget:** `BuildCx` add field + method; `Runtime` add HashMap + setter + simplify encode() signature; `CustomPaint` call registration in build()
- [ ] **App:** None — not involved (used starting in T0003)

## Approach

1. `harbor-widget/src/view.rs` — add field to `BuildCx`: `external_draws: Vec<(ExternalDrawId, Box<ExternalDrawFn<'static>>)>`. Add method:
   ```rust
   pub fn register_external_draw(&mut self, id: ExternalDrawId, handler: Box<ExternalDrawFn<'static>>);
   ```
2. `harbor-widget/src/runtime.rs` — add field to `Runtime`: `external_draws: HashMap<ExternalDrawId, Box<ExternalDrawFn<'static>>>`.
3. During `Runtime::update()` or after the build phase, drain registrations from `BuildCx` into the HashMap.
4. `Runtime::encode()` signature change — from:
   ```rust
   pub fn encode<'a>(&'a mut self, queue: &wgpu::Queue, pass: &mut wgpu::RenderPass<'a>, viewport: Viewport, external_draw: Option<&ExternalDrawFn<'_>>);
   ```
   to:
   ```rust
   pub fn encode<'a>(&'a mut self, queue: &wgpu::Queue, pass: &mut wgpu::RenderPass<'a>, viewport: Viewport);
   ```
   When encountering `Primitive::External`, look up handler from `self.external_draws`.
5. `CustomPaint` — add field `handler: Option<Box<ExternalDrawFn<'static>>>`. In `build()`, if handler is present, call `BuildCx::register_external_draw(self.draw_id, handler)`.
6. Tests: adapt `harbor-widget/tests/external_input.rs` for parameter-free `encode()`.

## Blocked by

(None — pre-refactoring ticket)

## Blocks

- T0003 — Terminal registering its handler through CustomPaint depends on this mechanism

## Acceptance

- [ ] `Runtime::encode()` signature has no `external_draw` parameter
- [ ] `BuildCx::register_external_draw()` exists and is callable
- [ ] Runtime internal HashMap correctly stores and looks up handlers
- [ ] Existing `CustomPaint` tests and `external_input` tests pass
- [ ] Test with two CustomPaint widgets with different draw_ids: encode invokes each handler for its respective External primitive

## Out of Scope

- What specific handler CustomPaint injects (T0003 implements)
- Handler lifecycle management (`Box<dyn>` is sufficient; no arena/slotmap needed)
- Changes outside the Widget crate
