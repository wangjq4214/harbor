# Keyed Children and Construction Protocol

**Ticket ID:** T0002
**Source:** [Spec: 0010-desktop-terminal-tabs-and-view-macro](../../spec/0010-desktop-terminal-tabs-and-view-macro.md)
**Status:** Todo

## Goal

Dynamic widget children have stable keyed identity, and both handwritten builders and generated macro code use one public child-construction protocol without exposing `AnyView`.

## Layers

- [ ] **Widget API:** Add `IntoChildView`, ordered `Children`, `WithChildren`, `ComponentExt::keyed`, and hidden macro-support re-exports.
- [ ] **Fiber and Reconciliation:** Complete keyed sibling insert/delete/reorder matching and duplicate-key diagnostics.
- [ ] **Layout/Renderer:** Preserve existing child order, layout, paint order, and retained SceneItem identity after correct reconciliation.
- [ ] **Runtime Host:** None; TabId integration follows later.
- [ ] **Verification:** Compare keyed/unkeyed behavior, hook/focus preservation, stale generations, cardinality, and handwritten compatibility.

## Approach

1. Convert Components to deferred child Views through `IntoChildView`; pass existing Views through unchanged.
2. Let `Children` collect one value, ordered iterators, and dynamic control-flow results.
3. Give macro-generated code a uniform `WithChildren` attachment operation while preserving current fluent `.child` APIs.
4. Implement a generic keyed wrapper/extension that does not require public `AnyView`.
5. Reconcile keyed siblings by key and compatible widget type across positions; retain positional matching only for unkeyed siblings.
6. Detect duplicate sibling keys deterministically rather than silently migrating state.

## Blocked by

- (none)

## Blocks

- T0003 — The view macro expands through these construction traits.
- T0007 — Dynamic TabItems require stable keyed identity.

## Acceptance

- [ ] Closing the middle keyed child preserves the later child's Fiber, hooks, focus eligibility, and retained identity.
- [ ] Inserting or reordering keyed children does not migrate state between keys.
- [ ] Duplicate sibling keys produce a deterministic diagnostic and never alias one Fiber.
- [ ] Unkeyed children retain documented positional reconciliation.
- [ ] Single-child attachment rejects multiple children clearly; multi-child attachment preserves source order.
- [ ] External code can key and compose public Components without naming or implementing crate-private `AnyView`.
- [ ] Existing handwritten widget builders remain source-compatible unless a separately documented correction is required.

## Out of Scope

- Global keys, cross-parent reparenting, automatic keys, reflection, and a stable third-party DSL contract.
