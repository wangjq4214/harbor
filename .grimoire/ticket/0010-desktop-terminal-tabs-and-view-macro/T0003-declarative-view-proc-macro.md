# Declarative `view!` Procedural Macro

**Ticket ID:** T0003
**Source:** [Spec: 0010-desktop-terminal-tabs-and-view-macro](../../spec/0010-desktop-terminal-tabs-and-view-macro.md)
**Status:** Done

## Goal

Harbor screens can express nested static and dynamic widget trees with an internal experimental `view! { cx; root }` macro that uses ordinary Rust value expressions, explicit parent child lists, and child-list control flow while expanding to the existing Component/View model.

## Layers

- [ ] **Macro/API:** Add `harbor-widget-macros`, re-export `view!`, resolve generated support paths, and preserve source spans.
- [ ] **Widget Runtime:** Consume only T0002 construction traits; introduce no state, lifecycle, layout, paint, or event semantics.
- [ ] **Application:** Add representative compile fixtures but do not migrate the product root yet.
- [ ] **Verification:** Parser/expansion tests, `trybuild` pass/fail cases, crate path tests, and runtime equivalence tests.

## Approach

1. Parse one explicit BuildCx expression and one Component root separated by `;`.
2. Support `expression;` child values, `expression => { children }` parents, ordinary Rust blocks, and bare child-list `for`/`if`/`match`.
3. Generate calls only through `harbor_widget::__macro_support`; the macro crate must not depend on `harbor-widget`.
4. Keep constructors, builder methods, callbacks, clones, moves, hooks, and keys as ordinary explicit Rust.
5. Preserve useful spans and let Rust report type/ownership/method errors where possible.

## Blocked by

- T0002 — Supplies child conversion, attachment, and key contracts.

## Blocks

- T0007 — The product tab tree is authored with the macro.

## Acceptance

- [ ] Static leaf, single-child, and multi-child trees compile and match handwritten View/Fiber structure.
- [ ] Ordinary value expressions and nested `for`/`if`/`match` preserve child order and explicit keys.
- [ ] The macro performs no implicit clone, move closure, hook, state, key, or resource registration.
- [ ] Old separators/interpolation, malformed terminators/blocks, multiple roots, and non-Component values have focused compile-fail coverage.
- [ ] Renamed dependency or internal-crate use resolves generated paths according to the chosen support policy.
- [ ] Macro-authored and handwritten fixtures produce equivalent layout, event targets, and scene output.
- [ ] Handwritten builder APIs remain fully usable without the macro.

## Out of Scope

- `#[component]`, generated Props, hooks syntax, CSS, RSX/HTML, hot reload, reflection, automatic events, and stable external API guarantees.
