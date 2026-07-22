# Geometry & Constraints

**Ticket ID:** T0003
**Source:** [Spec: 0001-retained-tree-pure-cpu-core](../spec/0001-retained-tree-pure-cpu-core.md)
**Status:** Todo

## Goal

`Size`, `Rect`, `Point`, and `BoxConstraints` provide complete logical-pixel geometry operations and constraint-solving functions, with all behavior covered by pure-CPU tests.

## Layers

- [ ] **types:** Extend `Size`, `Rect`, `Point`, `BoxConstraints` defined in T0001 with methods as needed — the types themselves exist from T0001; this ticket adds operations
- [ ] **core:** `BoxConstraints` solving: `clamp`, `loosen`, `tighten`, `constrain`; `Size` arithmetic: `expand`, `shrink`, `min`, `max`; `Rect` operations: `contains`, `intersect`, `union`
- [ ] **runtime:** None — constraint solving is pure functions, no Runtime dependency
- [ ] **tests:** At least one happy-path + one boundary-condition test per constraint-solving function; `BoxConstraints` invariant tests (e.g., `constrain` output always satisfies constraints)

## Approach

### 1. `Size` Methods (`layout/mod.rs`)

```rust
impl Size {
    pub const ZERO: Size = ...;
    pub fn new(width: f32, height: f32) -> Self;
    pub fn is_zero(&self) -> bool;
    pub fn is_finite(&self) -> bool;
    // Per-axis min/max
    pub fn min(&self, other: Size) -> Size;
    pub fn max(&self, other: Size) -> Size;
    // Clamp within range
    pub fn clamp(&self, min: Size, max: Size) -> Size;
}
```

### 2. `Point` Methods (`layout/mod.rs` or `layout/geometry.rs`)

```rust
impl Point {
    pub const ORIGIN: Point = ...;
    pub fn new(x: f32, y: f32) -> Self;
}
```

### 3. `Rect` Methods

```rust
impl Rect {
    pub fn new(origin: Point, size: Size) -> Self;
    pub fn from_min_max(min: Point, max: Point) -> Self;
    pub fn contains(&self, point: Point) -> bool;
    pub fn intersect(&self, other: &Rect) -> Option<Rect>;
    pub fn union(&self, other: &Rect) -> Rect;
    pub fn is_empty(&self) -> bool;
}
```

### 4. `BoxConstraints` Solving (`layout/constraints.rs`)

```rust
impl BoxConstraints {
    pub fn new(min: Size, max: Size) -> Self;
    pub fn tight(size: Size) -> Self;         // min == max == size
    pub fn loose(max: Size) -> Self;          // min == ZERO, max == max
    pub fn expand(width: f32, height: f32) -> Self; // min == ZERO, max == infinity-like

    // Core solving
    pub fn constrain(&self, size: Size) -> Size;   // clamp size to [min, max]

    // Constraint transforms
    pub fn loosen(&self) -> BoxConstraints;         // min = ZERO, max unchanged
    pub fn tighten(&self, size: Size) -> BoxConstraints; // new min = constrain(size) for axes with tight min

    // Child constraints
    pub fn deflate(&self, padding: impl Into<EdgeInsets>) -> BoxConstraints;
    pub fn constrain_width(&self, width: f32) -> f32;
    pub fn constrain_height(&self, height: f32) -> f32;

    // Invariant checks
    pub fn is_satisfied_by(&self, size: Size) -> bool;
    pub fn is_tight(&self) -> bool;  // min == max
    pub fn has_bounded_width(&self) -> bool;
    pub fn has_bounded_height(&self) -> bool;
}

// EdgeInsets — minimal for deflate; full version deferred to Phase 1
```

### 5. Behavior Details

- If `BoxConstraints::is_tight()`, `constrain` returns the single valid size
- `constrain` clamps width and height independently to `[min, max]`
- `max` may be `f32::INFINITY` (unbounded axis)
- `deflate` shrinks `max` by padding and adjusts `min` accordingly

### 6. Test Strategy

Each method covers at minimum:
- Happy path (normal values)
- Zero / boundary values (0.0 width/height, INFINITY max, ZERO rect)
- NaN handling (`is_finite` returns false; critical paths assert or debug_assert)

## Blocked by

- T0001 — `Size`, `Rect`, `Point`, `BoxConstraints` type definitions

## Blocks

(None)

## Acceptance

- [ ] `BoxConstraints::tight(Size::new(100.0, 50.0)).constrain(Size::new(200.0, 25.0))` returns `Size::new(100.0, 50.0)`
- [ ] `BoxConstraints::new(Size::ZERO, Size::new(200.0, 100.0)).constrain(Size::new(300.0, 50.0))` returns `Size::new(200.0, 50.0)`
- [ ] `BoxConstraints::tight(size).is_tight()` is `true`
- [ ] `BoxConstraints::tight(size).is_satisfied_by(size)` is `true`
- [ ] `BoxConstraints::tight(size).is_satisfied_by(size + delta)` is `false`
- [ ] `loosen()` sets min to ZERO, leaves max unchanged
- [ ] `deflate(padding)` correctly shrinks max and adjusts min
- [ ] `Rect::contains`: point inside, on boundary, outside — all correct
- [ ] `Rect::intersect`: overlapping rects return correct intersection; non-overlapping returns `None`
- [ ] `Rect::union`: bounding rect of both inputs
- [ ] `Rect::is_empty()`: zero-size rect returns `true`
- [ ] `Size::clamp(min, max)` works correctly in all three directions
- [ ] Zero-value and invariant tests: `Size::ZERO.is_zero()`, empty rect `contains` always `false`, tight constraint `constrain` returns the unique size

## Out of Scope

- Any Widget layout implementation (Row, Column, Padding, etc.) — Phase 1
- RenderNode struct — Phase 1
- Actual layout pass (recursive layout on Fiber tree) — Phase 1
- Physical pixel conversion (scale factor) — Phase 1
- Full EdgeInsets implementation (only what `deflate` needs) — Phase 1
- `Offset`, `Transform`, `Affine` types — Phase 1
- Hit-testing geometry (hit regions) — Phase 2
