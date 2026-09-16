# Use Rust-Expression Grammar for the Experimental View Macro

**Status:** Completed
**Date:** 2026-09-16

## Context

The experimental `view!` macro currently requires `view!(cx, root => { ... })`, represents every leaf as `expression => {}`, and reserves `{ expression }` for interpolation and child-collection flattening. Issue #132 proposes replacing that grammar to reduce syntax noise while preserving ordinary Rust expressions, explicit ownership, existing Component/View construction semantics, and evaluation order. Backwards compatibility with the experimental grammar is not required.

## Decision

Adopt `view! { cx; root }`. A semicolon-terminated ordinary Rust expression inserts one child through `IntoChildView`; `expression => { children }` attaches a child list through `WithChildren`; and bare `for`, `if`/`else`, and `match` organize child lists. Rust blocks are ordinary expressions and require a trailing semicolon when used as values. Roots remain Component-only and build directly with `Component::build`; already-built `View` roots are unsupported. Remove the interpolation-only `IntoChildren` protocol rather than retaining an unused collection-flattening compatibility surface.

## Consequences

- Repository macro invocations, tests, examples, and documentation must migrate together; the old comma separator and delimiter-free interpolation syntax are rejected with focused diagnostics.
- Child collections must be expanded explicitly with child-list control flow instead of implicit flattening.
- Leaf roots preserve direct root build and `BuildCx` semantics, while child components remain deferred through `IntoChildView`.
- Components that depend on `WithChildren::with_children` being called with an empty list require explicit review before replacing `expression => {}` with `expression;`.
- Expansion must preserve source spans, single evaluation, sibling order, children-before-parent construction, and context-before-root-build ordering.
- This decision changes only declarative syntax and construction support; action, HMR, host-resource, layout, event, and reconciliation architecture remain unchanged.
