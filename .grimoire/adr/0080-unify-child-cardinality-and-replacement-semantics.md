# Unify Child Cardinality and Replacement Semantics

**Status:** Proposed
**Date:** 2026-10-23

## Context

The historical UI/API review F2 identifies inconsistent child construction: single-child setters replace existing content while `WithChildren` counts existing and incoming children together and rejects replacement; `Align` accepts multiple children but lays out only the first; leaves implement empty-list acceptance and nonempty-list rejection. The current macro collects every block into a dynamic `Children` and panics on attachment errors.

The refinement conversation accepted the following contract and the route of one F2 spec, without implementation or ticket slicing. This records an agreed direction, not implemented behavior. [ADR 0032](./0032-view-macro-rust-expression-grammar.md)'s Rust-expression grammar and evaluation-order guarantees remain applicable; [ADR 0079](./0079-defer-root-construction-and-unify-content-slot-boundaries.md)'s deferred construction and content-Fiber boundaries remain unchanged.

## Decision

1. **Component-owned capabilities.** Distinguish Leaf (no attachable child slot), SingleChild (zero or one child), and MultiChild (ordered zero or more children) through shared construction capabilities. The macro must not use a widget-name whitelist. Leaf components do not implement an attachment capability merely to reject child lists; `leaf => { ... }`, including an empty block, is a compile-time error. A leaf remains usable as an ordinary root or child expression.
2. **Setters replace; named append operations append.** SingleChild uses `.child(value)` as a setter. MultiChild uses `.children(values)` as a list setter, `.push_child(value)` to append one, and `.extend_children(values)` to append a sequence. Repeated setters replace rather than accumulate. MultiChild's old appending `.child(...)` calls must migrate to explicit append operations.
3. **A macro child block is one replacement.** An incoming valid block replaces the staged slot/list; it does not append to it. An empty block clears the slot/list. SingleChild validates the incoming collection alone: staged `a` plus incoming `b` becomes `b`, but one block containing `a` and `b` is invalid, not last-write-wins. MultiChild preserves incoming order; append operations preserve existing order followed by appended order.
4. **Expose statically provable errors through Rust types.** The construction protocol must retain enough cardinality information for static invalidity to fail compilation, rather than relying solely on a runtime enum or type-name recognition. Statically bounded `if`/`match` child lists should retain this information where possible. Dynamic collections/loops are checked at construction against the actual incoming count, never silently truncated or deferred to layout.
5. **Keep the ordinary macro's return contract.** Recoverable handwritten dynamic construction returns `ChildConstructionError`. Ordinary `view!` continues returning `View` and reports a dynamic cardinality violation by a clear construction-site panic. Do not introduce `try_view!` or propagate `Result` through Component/View/runtime as part of F2. Runtime panic for invalid dynamic input is an accepted limitation.
6. **Honor real slots.** `Align` becomes SingleChild; multiple overlaid children belong in `Stack`. `InteractiveRegion::child` and `.content(...)` are alternative settings of the same slot and replace one another; macro replacement and clearing obey the same rule. Selected state-driven content retains its own child Fiber under ADR 0079.

## Consequences

- Handwritten builders and macro child blocks have one replacement/cardinality contract; empty blocks are meaningful clearing operations, not no-ops.
- This intentionally breaks old append-style MultiChild calls, Leaf child-block syntax, staged-plus-incoming error expectations, and any tests depending on those behaviors. Affected consumers, examples and tests must migrate.
- Static diagnostics require a capability/type-checking surface shared with external components; the exact trait/generic encoding is an implementation choice, not a widget-name dispatch table.
- The macro must retain single evaluation, sibling order, children-before-parent expression evaluation, branch laziness, and explicit ownership. New cardinality checking must not eagerly execute unselected branches or build components before reconciliation.
- This decision refines the attachment contract, not the Rust-expression grammar or the construction lifecycle. F3–F8 changes, including composable Button and general constructor cleanup, remain separate.
