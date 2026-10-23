# Deferred View Root and Content Slot Boundary

**Spec ID:** 0034
**Status:** In Progress
**Date:** 2026-10-23
**Sources:** `UI_API_REVIEW.md` finding F1/P0; [ADR 0079](../adr/0079-defer-root-construction-and-unify-content-slot-boundaries.md) (partial supersede of [ADR 0032](../adr/0032-view-macro-rust-expression-grammar.md)); this spec revises the root/signature clauses of [spec 0010](0010-desktop-terminal-tabs-and-view-macro.md) §"Macro contract".

## Requirements

- **R1 — Uniform construction protocol.** At every composition position the `view!` macro sends
  values through the same protocol: a `Component` becomes a deferred view via the
  `IntoChildView` conversion family, and an already-built `View` passes through unchanged. A
  `Component` root is deferred, not eagerly built; an already-built `View` is accepted as the
  macro root. The current `view_root.rs` rejection must become an accepted-and-verified case.
- **R2 — Root Fiber owned by the Runtime.** The Runtime/mount path allocates the root Fiber at
  the reconciliation top level and materializes the root component against that Fiber's own
  `BuildCx` (hooks, subscriptions, theme, external registrations attach to the root Fiber). The
  materialized root tree, state ownership, keyed identity, event routing, and rendered output
  must be equivalent to today's eager-root builds for unchanged application code.
- **R3 — Macro without a context parameter.** `view!` takes only the tree description; UI
  composition expressions do not pass a `BuildCx`. State, hook, and theme access stays inside
  `Component::build` / hook closures. The old `view! { cx; root }` signature is rejected with a
  focused diagnostic pointing at the removed separator/context, not a generic parse error.
- **R4 — Content slots own their component boundary.** A state-driven content slot (today
  `InteractiveRegion::content`; this is the model for subsequent slots) builds in its own child
  Fiber with its own hook sequence and Signal subscriptions. The slot content must not consume
  the parent component's hook serial numbers; parent rebuilds do not shift or re-run content
  hooks; removing the slot releases its hooks, subscriptions, and external registrations; theme
  inherits from the parent region.
- **R5 — Explicit order guarantees.** Single evaluation per expression, sibling order, and
  children-before-parent construction remain guaranteed and are re-fixed in macro/runtime tests
  (per [ADR 0079](../adr/0079-defer-root-construction-and-unify-content-slot-boundaries.md)
  D6). The superseded context-before-eager-root-build ordering is explicitly retired by ridding
  the root of eager construction; no other evaluation-order change may ride along.
- **R6 — One-wave migration.** All in-repo `view!` call sites (app tabs/search/palette/
  confirmation/appearance, widget tests, macro tests, docs) migrate to the new signature in one
  wave; the repository maintains a single macro contract without dual syntax or a compatibility
  layer.
- **R7 — Bounded materialization of build returns.** Component `build` return values keep the
  `View` type with no second description type. A build return that is itself a deferred
  description is materialized in place on the producing Fiber (single identity, theme, and hook
  sequence), with chained deferrals bounded: exceeding the bound fails loudly rather than
  looping. Structural or macro-side unbounded unrolling stays rejected. *(Revision
  2026-10-23 during implementation; replaces the earlier "deferred build return stays rejected"
  wording, which could not be reconciled with R6's one-wave migration of root components whose
  build ends in a single `view!` tree.)*

## Solution

Within [ADR 0079](../adr/0079-defer-root-construction-and-unify-content-slot-boundaries.md)'s
boundary and the Component/View runtime (no second description model):

- The macro expands roots and children through the same conversion expression (deferred
  component description via `IntoChildView`, `View` passthrough) and drops the context and
  semicolon-separated root context from `expand.rs` root handling. Macro parsing/expansion
  remains syntax sugar per ADR 0032's grammar rules for children.
- The Runtime mount path gains an internal top-level deferral step: allocate the root Fiber,
  bind it a `BuildCx`, run the root component, and reconcile the materialized concrete tree. The
  existing deferred-children path in `fiber/reconcile.rs` is reused; the deferred-build-return
  rejection at `decompose()` is kept.
- `InteractiveRegion` (and content slots following the same pattern) obtain a dedicated child Fiber
  for slot content: the content constructor runs against the child Fiber's `BuildCx`,
  hooks/subscriptions are stored on that Fiber, and removal follows standard child-Fiber
  teardown. The region's interaction state, activation, focus, and capture behavior are
  unchanged.
- Application code moves `cx`-dependent UI composition into functions/components invoked through
  the macro's deferred protocol; the tab row's zero-padding isolation wrapper is removed once
  stateful regions no longer need it to guard Fiber separation (removal verified against the
  R2 equivalence tests, not assumed).

### Seams

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Macro → Widget construction | `harbor-widget-macros` `expand.rs` → `harbor-widget` conversion protocols | `IntoChildView`-family protocols available unchanged at root and child positions | Uniform deferred description/root `View` passthrough |
| Runtime mount → Reconciliation | Runtime/mount entry (`view.rs` consumers, `fiber/reconcile.rs`) → root Fiber | A deferred root description | Top-level root Fiber allocation, materialization, then ordinary reconciliation |
| Content slot → Child Fiber reconciliation | `InteractiveRegion` content slot → `fiber/reconcile.rs` | Slot content built during the parent pass | Own child Fiber, own hooks/subscriptions, standard teardown |
| Macro contract tests | `harbor-widget-macros` parser/expand/`trybuild` fixtures → runtime equivalence tests | Re-fitted signatures and rejected old signatures | Compile-time and execution-time contract evidence |

## End-to-End Tests

### Extracted stateful root helper
- **Given:** An application view that currently extracts UI by setting up a wrapper component only
  to isolate a stateful region's Fiber (e.g. tab row's zero-padding wrapper).
- **When:** The helper runs through the macro's root/child positions without a context parameter.
- **Then:** Hook/state ownership matches the helper written as a normal component; the wrapper is
  unnecessary; tree type/keys, fiber order, layout, event routing, and scene output compare equal
  to a handwritten equivalent (runtime-equivalence standard of spec 0010).

### Built view accepted at root
- **Given:** An already-built `View` value in scope.
- **When:** It is supplied as the `view!` root.
- **Then:** It is reconciled with preserved identity (no wrapper Fiber, no re-keying); the same
  value as a child behaves identically.

### Content slot state independence
- **Given:** A state-driven content slot using hooks, inside a region that itself uses hooks.
- **When:** The parent region rebuilds while the slot content does not; then reverts.
- **Then:** Slot hook values survive parent passes with stable call order (no serial shift), the
  region's hooks are untouched by the slot's hook count, and the slot's subscriptions/focus
  behavior do not leak into the region.

### Slot teardown and external registration
- **Given:** A content slot registering an external draw/schedule provider (e.g. a terminal
  bridge-like content, such as a tab's TerminalWidgetBridge).
- **When:** The slot content is removed by a parent rebuild, or the runtime unmounts, or HMR
  rolls an old generation.
- **Then:** The slot child Fiber's hooks/subscriptions/external registrations are released in the
  same teardown order child Fibers follow today; no stale callback survives generation roll.

### Macro contract fixtures
- **Given:** `trybuild` fixtures for the new root forms and the removed signature.
- **When:** `view! { cx; root }` style inputs and non-Component roots are compiled.
- **Then:** The removed signature and non-Component roots fail with focused diagnostics;
  component roots, `View` roots, child forms, control flow, spans, single evaluation,
  sibling order, and children-before-parent construction all hold as before.

## Decisions

- **Choice:** Roots defer like children; Runtime owns root Fiber allocation.
- **Reason/source:** [ADR 0079](../adr/0079-defer-root-construction-and-unify-content-slot-boundaries.md) D1/D2.
- **Choice:** Keep `Component::build -> View`; no second description type.
- **Reason/source:** [ADR 0079](../adr/0079-defer-root-construction-and-unify-content-slot-boundaries.md) D3.
- **Choice:** Content slots build in their own child Fiber.
- **Reason/source:** [ADR 0079](../adr/0079-defer-root-construction-and-unify-content-slot-boundaries.md) D4.
- **Choice:** Macro signature drops the context parameter.
- **Reason/source:** [ADR 0079](../adr/0079-defer-root-construction-and-unify-content-slot-boundaries.md) D5, settled in this refinement conversation.

## Test Plan

- Extend `macro_tests.rs`, `tests/child_construction.rs`, and the `trybuild` case set for the new
  root forms and rejection diagnostics; keep the old parser cases for child grammar/regression.
- Runtime state/subscription/teardown characterization for root fibers and slot fibers: state
  ownership, Key/type identity, theme inheritance, external registration, unmount, HMR
  generation callback release — as mandated by
  [ADR 0079](../adr/0079-defer-root-construction-and-unify-content-slot-boundaries.md).
- Focus/traversal characterization: slot child Fibers must not add focus stops, perturb keyed
  identity, or change event routing outside the region's intended hit area.
- Evaluation-order characterization: single evaluation, sibling order, children-before-parent
  construction — re-derived and re-fixed rather than assumed unchanged.
- Migration wave verification: `cargo build`/test across macro, widget, and application crates
  after one-wave migration; un-migrated call sites must fail at compile time, not run.

## Out of Scope

- Findings F2–F8 of `UI_API_REVIEW.md` (child cardinality, callback thread model, Button
  composition, combination methods, constructor/naming, validation strategy, named parameter
  types) — separate decisions.
- Deferred build returns inside component bodies; deep chained deferral.
- Mobile navigation, touch-first gestures, stable third-party widget DSL (spec 0010 exclusions
  carry over).
- Implementation sequencing across [spec 0010](0010-desktop-terminal-tabs-and-view-macro.md)'s
  U1–U5 plan; this spec fixes the contract, not the ticket order.
