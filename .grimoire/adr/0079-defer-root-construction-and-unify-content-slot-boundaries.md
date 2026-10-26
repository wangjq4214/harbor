# Defer Root Construction and Unify Content Slot Boundaries

**Status:** Testing
**Date:** 2026-10-23

## Context

The historical UI/API review (finding F1/P0) identified an inconsistency in view
construction boundaries:

- `view!` macro roots eagerly call `Component::build(cx)` while child components are deferred
  through `IntoChildView` until reconciliation assigns their Fiber.
- An already-built `View` is accepted as a child but rejected as a macro root
  (`crates/harbor-widget-macros/tests/ui/fail/view_root.rs`).
- Root components needing UI extraction cannot be treated as uniform tree descriptions; callers
  must reason about which `cx` and whose hook sequence applies at each nesting position (the
  application tab row uses a zero-padding wrapper specifically to isolate a stateful region's
  Fiber).
- `InteractiveRegion::content` runs its content constructor with the owning region's
  `BuildCx`, borrowing the parent component's hook sequence.

Constraints from the runtime: `crates/harbor-widget/src/fiber/reconcile.rs` materializes deferred
children by building the component against the child's own Fiber, but a component build result is
expected to `decompose()` into a concrete view — a deferred build return panics. Evaluation-order
and construction-order guarantees (single evaluation, sibling order, children-before-parent
construction, context-before-root-build) are fixed by ADR 0032 and macro tests.

Alternatives considered:

- Preserve the eager root (status quo) — rejected: it forces every caller to distinguish hook
  ownership per position and blocks uniform UI extraction.
- Introduce a second root/child description type or unroll deferral inside the macro — rejected:
  it duplicates the Component/View description model the macro was explicitly not allowed to
  replace (ADR 0032).
- Let stateful content slots keep borrowing the parent hook sequence — rejected: hook ownership
  becomes invisible and slot content cannot manage its own state.

Scope: F1 only at this stage. Child-count contracts (F2), callback thread model (F3), and other
review findings are handled by separate decisions.

## Decision

1. **Uniform description protocol.** `view!` roots produce the same deferred component
   description used for children (via the `IntoChildView` conversion family); an already-built
   `View` is accepted at the root position like any child.
2. **Root Fiber allocation at the reconciliation top level.** The Runtime/mount path allocates the
   root Fiber and builds the root component against its own `BuildCx` through an internal
   deferral/mount step. This deferral is a runtime mount concern, not macro-unrolled recursion.
3. **Keep `Component::build -> View`.** No second UI-description type is introduced.
   *Revision (2026-10-23, during implementation):* a build return that is itself a deferred
   description is no longer rejected at the `decompose()` boundary; reconciliation materializes
   it in place on the same Fiber (identity, theme, and hook sequence stay single), with the
   number of chained deferrals bounded — exceeding the bound fails loudly instead of looping.
   Unbounded macro-side or structural unrolling remains rejected. Nearly every mounted root
   component ends its build in one `view!` tree, so the panic path could not survive migration
   without forcing callers to hand-assemble concrete roots, which the F1 boundary exists to
   remove.
4. **Content slots have component boundaries.** A state-driven content slot
   (`InteractiveRegion::content` today; the pattern thereafter) builds in its own child Fiber with
   its own hook sequence and Signal subscriptions. It never borrows the parent component's hook
   sequence, and its lifecycle (build on parent pass, state preservation by Fiber identity, hooks
   and subscriptions released on removal) follows standard child-Fiber reconciliation.
5. **No explicit context in the macro.** `view!` takes only the tree description; UI composition
   expressions do not carry a `BuildCx` parameter. Component logic that reads state, hooks, or
   theme lives inside `Component::build` / hook closures as today. The macro still identifies
   `harbor-widget` for its conversion protocols and remains syntax sugar with no new runtime
   semantics.
6. **Ordering guarantees are re-derived, not silently changed.** Single evaluation, sibling order,
   and children-before-parent construction must be re-fixed in the implementing spec and macro
   tests; no side-effect-order change may ride along with this refactor.

This decision partially supersedes [ADR 0032](./0032-view-macro-rust-expression-grammar.md):
the child expression grammar from that ADR is retained, while its root-constructs-directly and
`view! { cx; root }` signature clauses are replaced by the rules above.

## Consequences

- Enables: extracting stateful UI helpers without reassigning `cx`, treating every composition
  position as the same tree description, `View` values as roots, and state-local content slots.
- Requires: a top-level root mount/deferral step in the Runtime; reconciliation verification
  covering state ownership, Key/type identity, theme inheritance, external resource registration,
  subscription cleanup, unmount, and HMR old-generation callback release order; migration of all
  `view!` call sites in one wave; a revision of the macro contract clauses in
  [spec 0010](../spec/0010-desktop-terminal-tabs-and-view-macro.md) through the implementing
  spec.
- Constrains: component build returns stay concrete; content slots add child Fibers, so the F1
  spec must show they do not perturb focus stops, keyed identity, or event routing beyond the
  parent's intended region. Evaluation-order and hook-ownership guarantees must be re-tested, not
  assumed unchanged.
