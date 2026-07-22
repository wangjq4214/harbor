# Widget Runtime Phase 0 — Retained Tree & Pure CPU Core

**Source:** [Spec: 0001-retained-tree-pure-cpu-core](../spec/0001-retained-tree-pure-cpu-core.md)
**Ticket folder:** `.grimoire/ticket/0001-retained-tree-pure-cpu-core/`

## Overview

Phase 0 establishes the pure-CPU core of the `harbor-widget` crate: a generation arena for Fiber identity and lifecycle, View/Key for immutable interface descriptions, Signal for fine-grained reactive state, five reconciliation rules driving incremental updates, a four-flag dirty system preventing over-invalidation, and logical-pixel geometry with constraint solving. Completion is verified through pure-CPU tests covering tree structure, state reuse, dirty propagation, and constraint satisfaction — no GPU or windowing system required.

## Layers

The project's architectural layers:

1. **types** — Data structures: `FiberId`, `View`, `Key`, `Component`, `Signal<T>`, `DirtyFlags`, `Size`, `Rect`, `BoxConstraints`
2. **core** — Algorithms: generation arena, reconciliation rules, Signal dependency tracking, dirty flag propagation, constraint solving
3. **runtime** — Public API: `Runtime` struct, `set_root`, `dispatch`, `update`, `FrameRequest`
4. **tests** — Verification: unit tests for all algorithms, integration tests for full cycles

Every ticket cuts through all confirmed layers.

## Dependency Graph

### Blocking relationships

| Ticket | Blocks     | Reason                                                       |
| ------ | ---------- | ------------------------------------------------------------ |
| T0001  | T0002, T0003 | All type definitions and the crate skeleton must exist before any implementation begins |
| T0002  | —          |                                                              |
| T0003  | —          |                                                              |

### Parallel groups

| Group | Tickets    | Reason                                                     |
| ----- | ---------- | ---------------------------------------------------------- |
| A     | T0002, T0003 | Zero file overlap: T0002 modifies fiber.rs, signal.rs, runtime.rs; T0003 modifies layout/. No runtime contract overlap. |

## Recommended Order

1. T0001 — Crate & Core Types (pre-refactoring)
2. T0002 ∥ T0003 — Reconciliation + Reactivity ∥ Geometry & Constraints (parallel)

## Ticket Index

| Ticket ID | File                                  | Title                        | Summary                                                        |
| --------- | ------------------------------------- | ---------------------------- | -------------------------------------------------------------- |
| T0001     | [T0001-crate-and-core-types.md](./T0001-crate-and-core-types.md)       | Crate & Core Types           | Cargo.toml / lib.rs scaffold, all type definitions, generation arena, Runtime skeleton |
| T0002     | [T0002-reconciliation-and-reactivity.md](./T0002-reconciliation-and-reactivity.md) | Reconciliation + Reactivity  | Five reconciliation rules, Signal dependency tracking, four-flag dirty propagation, Runtime implementation |
| T0003     | [T0003-geometry-and-constraints.md](./T0003-geometry-and-constraints.md) | Geometry & Constraints       | BoxConstraints solving, Size/Rect operations, pure-CPU layout computation |
