# Widget Runtime Phase 0 — Retained Tree & Pure CPU Core

**Spec ID:** 0001
**Status:** Draft
**Date:** 2025-07-22

## Requirement

Harbor must have a pure-CPU retained Widget tree core that manages component identity, reactive state, reconciliation, and layout constraints — without depending on GPU, windowing, or terminal subsystems.

## Solution

Create the `harbor-widget` crate under `crates/harbor-widget/` implementing the foundational data structures and algorithms that later phases (GPU rendering, input routing, text) build upon.

### Components

**Fiber Arena & Identity**

`FiberId { index: u32, generation: u32 }` stored in a generation arena. Slot reuse bumps generation so stale IDs safely fail lookups. Events, Signal subscriptions, and pointer capture all validate identity via generation before operating on a node.

**View & Key**

`View` is an immutable interface description produced by component build functions. A `Key` type enables stable identity across rebuilds for reorderable children. Reconciliation matches by `(type, Key, position)` to decide reuse vs teardown.

**Reconciliation**

Rules (from design doc §5.2):
1. Same position, same View type, same Key → reuse Fiber, State, subscriptions
2. Type or Key changed → unmount old subtree (cleanup subscriptions), create new Fiber
3. Siblings without Key match by position
4. Reorderable lists require stable Key
5. Keyless list reordering causes state loss — locked by test

**Signal**

`Signal<T>` is a fine-grained reactive state cell on the UI thread. During Fiber build, the runtime records which Signals are read. Writing a Signal marks only subscribing Fibers `BUILD_DIRTY`. Cross-thread updates go through `UiMessage` — direct cross-thread Signal writes are forbidden.

**Dirty Flags**

Four orthogonal flags (from design doc §6):

| Flag             | Trigger                         | Effect                  |
| ---------------- | ------------------------------- | ----------------------- |
| `BUILD_DIRTY`    | View, property, or state change | Reconciliation          |
| `LAYOUT_DIRTY`   | Constraint, size, or position change | Subtree re-layout |
| `PAINT_DIRTY`    | Visual property change         | SceneItem update        |
| `HIT_TEST_DIRTY` | Geometry, transform, or clip change | Hit data rebuild    |

Dirty marking propagates appropriately: a color change only marks `PAINT_DIRTY`; a title length change marks `LAYOUT_DIRTY | PAINT_DIRTY | HIT_TEST_DIRTY`; window resize marks root `LAYOUT_DIRTY` cascading down.

**Geometry**

`Size`, `Rect`, and `BoxConstraints { min: Size, max: Size }` in logical pixels (dp). Physical pixel conversion happens only at the renderer boundary in later phases.

**Runtime**

```rust
pub struct Runtime { /* private */ }

impl Runtime {
    pub fn set_root(&mut self, root: impl Component + 'static);
    pub fn dispatch(&mut self, event: UiEvent, now: Instant) -> FrameRequest;
    pub fn update(&mut self, now: Instant) -> FrameRequest;
}
```

At Phase 0, `dispatch` and `update` process dirty flags and drive reconciliation and layout. `encode` is deferred to Phase 1. `UiEvent` and `FrameRequest` are stubbed sufficiently for compilation and unit testing.

### Seams

| Seam                | Connects                               | Expects                                                                         | Provides                       |
| ------------------- | -------------------------------------- | ------------------------------------------------------------------------------- | ------------------------------ |
| Workspace membership | `harbor-widget` ↔ Cargo workspace      | Cargo.toml `[workspace]` member entry; dependency resolution from workspace root | `harbor-widget` library crate  |
| Runtime API         | `harbor-widget` → host (later `harbor-ui`) | `Component` impl as root; `UiEvent` for dispatch; `Instant` for time            | `FrameRequest` indicating whether redraw is needed |

## End-to-End Tests

### E2E: Type + Key identity preserves state across rebuilds

- **Given:** A Fiber tree with a `Counter` component at position 0, key `"a"`, holding state value 5
- **When:** Rebuild produces the same View type at position 0 with key `"a"`
- **Then:** The existing Fiber is reused; state value remains 5; no unmount/mount side effects fire

### E2E: Key change causes teardown and fresh creation

- **Given:** A Fiber tree with a `TabButton` at position 1, key `"tab-1"`, with an active Signal subscription
- **When:** Rebuild produces `TabButton` at position 1 with key `"tab-2"`
- **Then:** Old Fiber is unmounted; its Signal subscription is cleaned up; a new Fiber is created with fresh state

### E2E: Stale FiberId does not match new node at same slot

- **Given:** A Fiber with `FiberId { index: 0, generation: 1 }` is unmounted; slot 0 is reused for a new Fiber with `FiberId { index: 0, generation: 2 }`
- **When:** An event or Signal callback holds a reference to `{ index: 0, generation: 1 }` and attempts to operate on it
- **Then:** The operation is safely rejected (generation mismatch); no state corruption or crash

### E2E: Signal write only invalidates subscribing Fibers

- **Given:** Fibers A and B subscribe to Signal S1; Fiber C subscribes to Signal S2; Fiber D subscribes to none
- **When:** `S1.set(new_value)` is called
- **Then:** Only A and B are marked `BUILD_DIRTY`; C and D remain clean

### E2E: Layout satisfies BoxConstraints

- **Given:** A `SizedBox` with `width: 100, height: 50` inside a parent imposing `BoxConstraints { min: (0,0), max: (200, 100) }`
- **When:** Layout runs
- **Then:** The `SizedBox` reports `Size { width: 100, height: 50 }`; children receive clamped constraints

## Decisions

### Generation arena over Rc/Arc-based identity

- **Choice:** `FiberId { index, generation }` with a slot-reusing arena.
- **Reason:** Enables stale reference detection without reference counting overhead. A recycled slot with a new generation fails lookups from old IDs, preventing use-after-free style bugs in event callbacks and Signal subscriptions. This aligns with the design doc's explicit identity model (§5.1).
- **ADR reference:** None (no existing ADRs; this decision is captured in the design doc §5.1).

### Signal over context-passed &mut State

- **Choice:** `Signal<T>` with automatic dependency tracking during Fiber build.
- **Reason:** Avoids threading `&mut` through the entire widget tree. With Signals, writing state marks only the subscribing Fiber dirty — the runtime handles the rest. This is the pattern established in §6 of the design doc and is table-stakes for the incremental update model later phases depend on.
- **ADR reference:** None.

### Separate dirty flags over a single "dirty" boolean

- **Choice:** Four flags: `BUILD_DIRTY`, `LAYOUT_DIRTY`, `PAINT_DIRTY`, `HIT_TEST_DIRTY`.
- **Reason:** A single flag would over-invalidate — a color change would trigger unnecessary layout and hit-test work. The four-flag design is specified in §6 of the design doc and is essential for the "no continuous redraw when idle" invariant from §1.
- **ADR reference:** None.

### Logical pixels for all CPU-side geometry

- **Choice:** `Size`, `Rect`, `BoxConstraints` use logical pixels (dp). Scale factor applied only at the GPU boundary.
- **Reason:** Keeps layout, hit-testing, and DPI handling coherent. Physical pixel math in layout leads to rounding inconsistencies. Design doc §7 specifies this separation.
- **ADR reference:** None.

## Test Plan

- **Unit tests:** Generation arena allocation, deallocation, slot reuse, and stale ID rejection. Reconciliation rules for each of the five cases in §5.2. Dirty flag propagation matrix (e.g., color change → only PAINT_DIRTY; size change → LAYOUT + PAINT + HIT_TEST). BoxConstraints clamping, Size expansion, and Rect operations.
- **Integration tests:** Full `set_root` → `update` cycle: build produces Fiber tree, Signal write dirties correct subset, `update` reconciles and re-lays out. Multiple Signals interacted with by overlapping Fiber sets.
- **Manual tests:** None at this phase — pure CPU, no visual output.
- **Performance thresholds:** Reconciliation of a 1,000-node tree under 1ms. Signal write → dirty marking under 10μs (single Fiber case). These will be profiled but not gated in CI until Phase 3 when real widget trees exist.
- **Edge cases:** Empty tree (set_root with no children). Deeply nested trees (100+ levels) with partial dirty propagation. Rapid Signal writes (10,000 writes before next update) — dirty marking must be idempotent. Key collisions (same key at same level) — defined behavior: first match wins, second is treated as type/key mismatch and tears down.

## Out of Scope

- **GPU rendering, wgpu, Surface, or CommandEncoder.** These enter in Phase 1 (Scene Graph & Quad Renderer).
- **Input event routing, hit testing, focus, pointer capture.** These enter in Phase 2.
- **Text rendering, glyph atlas, font fallback, `harbor-text` extraction.** Phase 3.
- **Layout widget implementations** (Row, Column, Stack, Padding, etc.). Phase 0 provides the constraint/geometry types; widget implementations come in Phase 1.
- **`encode()` method on Runtime.** Phase 1.
- **`UiEvent` beyond a stub.** Full event types (pointer, keyboard, IME) are Phase 2.
- **Animations, frame clock, interpolation.** Phase 4.
- **Concurrent/interruptible work loop, scheduling priorities.** Phase 4; only added if profiling shows 1,000+ node trees cause input latency.
- **Macros for View construction.** First version uses plain Rust builder pattern (§5.2).

## Future Evolution

- The dirty flag system is designed for eventual concurrent work loop support (§6) — when profiling demands it, `BUILD_DIRTY` processing can be split across frames with priority scheduling.
- The generation arena may need compaction if long-running sessions with frequent mount/unmount cycles show memory growth.
- `BoxConstraints` may be extended to support `tight`, `loose`, and `expand` convenience constructors once layout widget implementations reveal common patterns in Phase 1.
- Signal batching (`Signal::batch`) may be introduced if multiple Signal writes within a single event handler cause redundant dirty marking.
