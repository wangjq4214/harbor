# Declarative `view!` Procedural Macro

**Ticket ID:** T0003
**Source:** [Spec: 0010-desktop-terminal-tabs-and-view-macro](../../spec/0010-desktop-terminal-tabs-and-view-macro.md)
**Status:** Done

## Goal

Harbor screens can express nested static and dynamic widget trees with an internal experimental `view!(cx, component => { children })` macro that expands to the existing Component/View model.

## Layers

- [ ] **Macro/API:** Add `harbor-widget-macros`, re-export `view!`, resolve generated support paths, and preserve source spans.
- [ ] **Widget Runtime:** Consume only T0002 construction traits; introduce no state, lifecycle, layout, paint, or event semantics.
- [ ] **Application:** Add representative compile fixtures but do not migrate the product root yet.
- [ ] **Verification:** Parser/expansion tests, `trybuild` pass/fail cases, crate path tests, and runtime equivalence tests.

## Approach

1. Parse one explicit BuildCx expression and one root component expression separated from children by `=>`.
2. Support nested nodes, empty children, `{ expression }`, `for`, `if/else`, and `match`.
3. Build the root concretely with the supplied context and convert children to deferred Views.
4. Generate calls only through `harbor_widget::__macro_support`; the macro crate must not depend on `harbor-widget`.
5. Keep constructors, builder methods, callbacks, clones, moves, hooks, and keys as ordinary explicit Rust.
6. Preserve useful spans and let Rust report type/ownership/method errors where possible.

## Blocked by

- T0002 — Supplies child conversion, attachment, and key contracts.

## Blocks

- T0007 — The product tab tree is authored with the macro.

## Acceptance

- [ ] Static leaf, single-child, and multi-child trees compile and match handwritten View/Fiber structure.
- [ ] Interpolation and nested `for`/`if`/`match` preserve child order and explicit keys.
- [ ] The macro performs no implicit clone, move closure, hook, state, key, or resource registration.
- [ ] Invalid arrows/blocks, multiple roots, invalid interpolation, and non-Component children have focused compile-fail coverage.
- [ ] Renamed dependency or internal-crate use resolves generated paths according to the chosen support policy.
- [ ] Macro-authored and handwritten fixtures produce equivalent layout, event targets, and scene output.
- [ ] Handwritten builder APIs remain fully usable without the macro.

## Out of Scope

- `#[component]`, generated Props, hooks syntax, CSS, RSX/HTML, hot reload, reflection, automatic events, and stable external API guarantees.
