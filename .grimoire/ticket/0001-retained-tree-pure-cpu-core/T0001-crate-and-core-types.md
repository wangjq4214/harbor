# Crate & Core Types

**Ticket ID:** T0001
**Source:** [Spec: 0001-retained-tree-pure-cpu-core](../spec/0001-retained-tree-pure-cpu-core.md)
**Status:** Todo

## Goal

The `harbor-widget` crate exists in the workspace. All core types (`FiberId`, `View`, `Key`, `Component`, `Signal`, `DirtyFlags`, geometry types) and the generation arena are defined. The `Runtime` skeleton compiles cleanly.

## Layers

- [ ] **types:** Define `FiberId`, `View` trait, `Key`, `Component` trait, `Signal<T>`, `DirtyFlags` bitflags, `Size`, `Rect`, `BoxConstraints`, `UiEvent` (stub), `FrameRequest` (stub)
- [ ] **core:** Implement generation arena (allocate, deallocate, slot reuse with generation increment); expose safe lookup by `FiberId` on the arena
- [ ] **runtime:** `Runtime` struct and `new()`; `set_root`, `dispatch`, `update` method signatures (bodies are `todo!()` or empty); `encode` is NOT exposed in Phase 0
- [ ] **tests:** Generation arena unit tests (allocate/deallocate/slot reuse/stale ID rejection); types compile verification

## Approach

1. Add `crates/harbor-widget` to the workspace root `Cargo.toml` under `[workspace].members`
2. Create `crates/harbor-widget/Cargo.toml`: no dependency on wgpu, winit, harbor-terminal, or other harbor crates. Zero external dependencies in Phase 0, or `bitflags` only
3. Create `crates/harbor-widget/src/lib.rs`: declare all modules and re-export public types
4. **`fiber.rs`**: `FiberId { index: u32, generation: u32 }`, generation arena (internal `Vec<Option<FiberSlot>>` where each slot holds `generation: u32` and `occupied: bool`). `allocate() -> FiberId` (reuses freed slots with incremented generation), `deallocate(id)` (validates generation then frees slot), `get(id) -> Option<&FiberSlot>` (returns `None` on generation mismatch)
5. **`view.rs`**: `View` trait (minimal: `fn key(&self) -> Option<Key>`), `Key` type (newtype over string or integer), `Component` trait (`fn build(&self, cx: &mut BuildCx) -> View`), `BuildCx` stub
6. **`signal.rs`**: `Signal<T>` struct (internally `Rc<RefCell<SignalInner<T>>>`), `Signal::new(initial)`, `Signal::read() -> T where T: Copy`, `Signal::set(value)`. Dependency tracking interfaces reserved (Phase 0 only defines types; tracking logic goes in T0002)
7. **`layout/mod.rs`**: `Size { width: f32, height: f32 }`, `Rect { origin: Point, size: Size }`, `Point { x: f32, y: f32 }`, `BoxConstraints { min: Size, max: Size }`
8. **`runtime.rs`**: `Runtime` struct (private fields: Fiber arena, root FiberId, etc.). `new()` constructor. Three public method signatures with `unimplemented!()` bodies: `set_root(&mut self, root: impl Component + 'static)`, `dispatch(&mut self, event: UiEvent, now: Instant) -> FrameRequest`, `update(&mut self, now: Instant) -> FrameRequest`
9. **`input/mod.rs`**: `UiEvent` stub enum (single variant `UiEvent::Noop`), `FrameRequest` stub (`FrameRequest::Redraw` and `FrameRequest::Idle`)

## Blocked by

(None — this is the pre-refactoring ticket)

## Blocks

- T0002 — Reconciliation + Reactivity (depends on Fiber arena, View/Key/Component, Signal types, Runtime skeleton)
- T0003 — Geometry & Constraints (depends on Size/Rect/BoxConstraints type definitions)

## Acceptance

- [ ] `cargo build -p harbor-widget` compiles successfully
- [ ] `cargo test -p harbor-widget` all pass
- [ ] Arena: allocate 3 slots, free slot 1, re-allocate returns `FiberId { index: 1, generation: 2 }`
- [ ] Arena: lookup with stale generation=1 ID on slot 1 returns `None`
- [ ] Arena: slot exhaustion triggers grow rather than panic
- [ ] `Signal::new(42).read()` returns `42`
- [ ] `Signal::set()` followed by `read()` returns the new value
- [ ] `BoxConstraints` is constructible, `Size` is comparable

## Out of Scope

- Reconciliation algorithm (T0002)
- Signal dependency tracking and auto-subscription (T0002)
- Dirty flag marking and propagation logic (T0002)
- BoxConstraints solving (`clamp`, `loosen`, etc.) (T0003)
- Rect operations (`contains`, `intersect`, etc.) (T0003)
- `Runtime` method implementations (T0002)
- Real `UiEvent` variants (Phase 2)
- Any Widget implementations (Phase 1)
- wgpu / winit / harbor-terminal dependencies
