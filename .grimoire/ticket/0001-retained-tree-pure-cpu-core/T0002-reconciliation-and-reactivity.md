# Reconciliation + Reactivity

**Ticket ID:** T0002
**Source:** [Spec: 0001-retained-tree-pure-cpu-core](../spec/0001-retained-tree-pure-cpu-core.md)
**Status:** Todo

## Goal

The Runtime accepts a View tree, executes reconciliation (reusing or rebuilding Fibers), Signal writes automatically mark subscribing Fibers `BUILD_DIRTY`, and the update cycle processes only dirty nodes with correct ancestor dirty cascading.

## Layers

- [ ] **types:** No new types — uses `FiberId`, `View`, `Key`, `Component`, `Signal<T>`, `DirtyFlags`, `UiEvent`, `FrameRequest` defined in T0001
- [ ] **core:** Five reconciliation rules, Signal dependency collection and subscription, four-flag dirty propagation, `update` dirty-queue processing
- [ ] **runtime:** `set_root` builds the initial Fiber tree; `dispatch` executes events and handles dirty marking after Signal writes; `update` traverses dirty Fibers and runs reconciliation
- [ ] **tests:** One test per reconciliation rule; Signal-invalidates-only-subscribers test; dirty flag propagation matrix test; `set_root` → write Signal → `update` full-cycle test

## Approach

### 1. Reconciliation (implement in `fiber.rs`)

Implement the five reconciliation rules (design doc §5.2):

1. Same position, same View type, same Key → reuse Fiber (preserve state, subscriptions, children)
2. Type or Key changed → unmount old subtree (clean up Signal subscriptions), create new Fiber
3. Siblings without Key match by position
4. Keyed lists match by Key (out-of-range children unmounted, new children mounted)
5. Keyless list reordering causes state loss — do NOT attempt smart diff; position-based only

Matching logic: iterate old and new children, build a match table by `(View::type_id(), Key, position)`. Match by Key first for exact hits, then fill remaining by position.

On unmount, clean up the Fiber: unsubscribe from all Signals (call `Signal::unsubscribe` for each subscription).

### 2. Signal Dependency Tracking (implement in `signal.rs`)

- Maintain a `current_fiber_id: Cell<Option<FiberId>>` in `BuildCx`. The Runtime sets this during Fiber build
- On `Signal::read()`, if `BuildCx::current_fiber_id` is `Some`, record the `(Signal, FiberId)` pair in the Signal's subscriber list
- On `Signal::set()`, iterate subscribers, mark each FiberId `BUILD_DIRTY`, and propagate upward to root (mark ancestors)
- `Signal::unsubscribe(fiber_id)` removes from subscriber list
- On Fiber unmount, call unsubscribe for every subscribed Signal

### 3. Dirty Flag System (`fiber.rs` or separate `dirty.rs`)

Four-flag bitflags:

```rust
bitflags! {
    pub struct DirtyFlags: u8 {
        const BUILD_DIRTY    = 0b0001;
        const LAYOUT_DIRTY   = 0b0010;
        const PAINT_DIRTY    = 0b0100;
        const HIT_TEST_DIRTY = 0b1000;
    }
}
```

Propagation rules:
- Marking `BUILD_DIRTY` automatically marks the parent `BUILD_DIRTY` (cascading up to root)
- Marking `LAYOUT_DIRTY` automatically marks `HIT_TEST_DIRTY` (layout changes affect hit testing)
- Root resize → mark root `LAYOUT_DIRTY | HIT_TEST_DIRTY`

### 4. Runtime Implementation (`runtime.rs`)

**`set_root(root)`**:
- Allocate root Fiber via arena
- Trigger initial build for root Fiber (call `Component::build`, set `current_fiber_id` in `BuildCx` to collect Signal dependencies)
- Mark root `BUILD_DIRTY`
- Return `FrameRequest::Redraw`

**`dispatch(event, now)`**:
- Process event (Phase 0: Signal-write-only scenario; no real event routing)
- Collect all dirty Fibers resulting from Signal writes during this event loop tick
- Return `FrameRequest::Redraw` if any dirty nodes, `FrameRequest::Idle` otherwise

**`update(now)`**:
- Traverse all `BUILD_DIRTY` Fibers (walk from root downward, or collect dirty leaves upward then process from root)
- For each dirty Fiber, run reconciliation (compare old and new View children)
- Clear processed Fibers' `BUILD_DIRTY`
- Return `FrameRequest::Redraw` if anything changed, `FrameRequest::Idle` if no dirty nodes remain

### 5. Idle Guarantee

After `update` completes, if no dirty nodes, no animations, and no external draw requests remain, `dispatch` returns `FrameRequest::Idle`. The host uses this to skip `encode` (Phase 1+) and avoid requesting redraws.

## Blocked by

- T0001 — All type definitions, generation arena, Runtime skeleton

## Blocks

(None)

## Acceptance

- [ ] Same type + same Key → Fiber reused, state preserved
- [ ] Type changed → old Fiber unmounted (Signal subscriptions cleaned up), new Fiber created
- [ ] Key changed (same type) → old unmounted, new created
- [ ] Keyless list matches by position; reordering causes state loss
- [ ] Children added/removed: new children mount, removed children unmount
- [ ] Signal read during build auto-subscribes the current Fiber
- [ ] Signal write marks only subscribing Fibers `BUILD_DIRTY`; non-subscribers unaffected
- [ ] Unmounting a Fiber cleans up its Signal subscriptions (subsequent Signal writes do not affect unmounted nodes)
- [ ] Dirty flag cascades upward to root (child dirty → parent dirty → ... → root dirty)
- [ ] `Runtime::new()` followed by `update()` returns `FrameRequest::Idle` (no tree)
- [ ] `set_root` → `update` → tree built, `update` returns `FrameRequest::Redraw`
- [ ] Write Signal → `dispatch` → `update` → only dirty subtree is reconciled
- [ ] Idle state (no Signal writes, no events) → `update` returns `FrameRequest::Idle`

## Out of Scope

- Event routing (pointer event capture/bubble, focus traversal) — Phase 2
- Real `UiEvent` variants (currently stub only) — Phase 2
- Widget implementations (Row, Column, Text, Button, etc.) — Phase 1
- Layout computation (layout pass) — Phase 1 (Phase 0 only marks `LAYOUT_DIRTY`, does not perform actual layout)
- Scene graph, GPU rendering — Phase 1
- Animation frame clock — Phase 4
- Interruptible work loop / scheduling priorities — Phase 4
- Signal batch write optimization
