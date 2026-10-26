# Unified Child Cardinality and Attachment

**Spec ID:** 0035
**Status:** Accepted (owner-confirmed)
**Date:** 2026-10-23
**Sources:** `UI_API_REVIEW.md` F2/P0; refinement conversation accepting replacement/clearing semantics, construction-time dynamic errors; [ADR 0080](../adr/0080-unify-child-cardinality-and-replacement-semantics.md); [ADR 0032](../adr/0032-view-macro-rust-expression-grammar.md); [ADR 0079](../adr/0079-defer-root-construction-and-unify-content-slot-boundaries.md).

This contract revises the child-attachment clauses of [spec 0010](0010-desktop-terminal-tabs-and-view-macro.md): empty blocks are valid only for child-capable parents, leaf child blocks are rejected, and attachment is replacement rather than accumulation. [Spec 0034](0034-deferred-view-root-and-content-slot-boundary.md)'s deferred root/content-slot boundaries remain applicable.

## Requirements

### R1 — Component-owned child capabilities

Every child-attachment surface must follow one of these contracts, declared by the receiving component through a shared construction protocol:

| Capability | Allowed incoming count | Handwritten setting | Appending |
| --- | --- | --- | --- |
| Leaf | No attachment operation | None | None |
| SingleChild | 0–1 | `.child(value)` replaces the slot | None |
| MultiChild | 0–N, ordered | `.children(values)` replaces the list | `.push_child(value)` and `.extend_children(values)` |

Leaves remain valid ordinary Component/View expressions. They must not implement an attachment capability merely to accept an empty list or reject a nonempty list. Macro leaf blocks, including `leaf => {}`, fail compilation. The macro must not identify capabilities by widget names; external components must be able to participate through public construction APIs without accessing private render interfaces.

### R2 — Replacement and append are distinct

- Repeated SingleChild `.child(...)` calls replace the previous value; no accumulated count includes overwritten values.
- MultiChild `.children(...)` replaces the whole previous list, including when the new list is empty.
- MultiChild `.push_child(...)` appends one value. `.extend_children(...)` appends a sequence in iteration order, after all existing children. Extending by an empty sequence does not change the list.
- Old MultiChild `.child(...)` calls that append must migrate to explicit append operations. A method named `.child(...)` must not retain an append meaning on these components.
- A macro child block is one replacement operation through the same semantic protocol, not an implicit append. Existing staged content is not included in the incoming count.
- An empty block on SingleChild or MultiChild clears existing content. An ordinary value expression with no child block does not clear or reset staged content.

The following illustrate the replacement and cardinality contract:

```rust
// Final content is b, just like .child(a).child(b).
view! { Padding::all(8.0).child(a) => { b; } }

// Final content is empty.
view! { Padding::all(8.0).child(a) => {} }

// Invalid: one incoming assignment contains two children.
view! { Padding::all(8.0) => { a; b; } }

// Final ordered children are [c], not [a, b, c].
view! { Row::new().push_child(a).push_child(b) => { c; } }
```

### R3 — Static invalidity fails compilation

A statically expressed block of two or more children is rejected for SingleChild during compilation. Leaf attachment is rejected regardless of whether its block is empty, static, or dynamic. Diagnostics must identify the child block/receiving expression responsible for the violation, rather than waiting for a layout failure.

The shared protocol must retain sufficient cardinality information for these checks; lowering every input to an untyped dynamic list followed only by a runtime enum check does not satisfy R3. Statically bounded `if`/`match` lists should retain their cardinality information where possible: alternatives each yielding at most one child are valid SingleChild inputs and must not be summed as if all alternatives execute. This does not require evaluating Rust conditions or iterator lengths during macro expansion.

### R4 — Dynamic invalidity fails at construction

Dynamic collections and child-list loops validate the actual incoming count at construction. SingleChild accepts zero (clear) or one (replace), and rejects two or more even if no child was staged. Previously staged content does not turn an incoming count of one into an error.

Recoverable handwritten dynamic construction returns `ChildConstructionError` for invalid cardinality. The diagnostic identifies the receiving component, allowed cardinality and actual incoming count; the count excludes overwritten staged content. Ordinary `view!` continues returning `View` and reports dynamic invalidity by an explicit construction-site panic. Invalid lists must not be truncated, reduced to first/last child, or left for reconciliation/layout to discover.

No `try_view!` or Result-returning Component/View/runtime migration is required. Invalid dynamic input can still panic in ordinary `view!`; this limitation is explicitly accepted.

### R5 — Real slot ownership

- `Align` is SingleChild in storage and attachment behavior. A single incoming child replaces its old child; a multi-child block is invalid. Multiple overlaid children use `Stack` instead. Valid zero-/one-child alignment behavior otherwise remains unchanged.
- Existing multi-child containers retain their layout meaning and ordered children; setting and append names follow R2. Existing leaves remain leaves: F2 does not make Button composable.
- `InteractiveRegion::child(...)` and `.content(...)` select alternatives for the same slot and replace one another. A nonempty valid macro block replaces either alternative with its supplied child; an empty block clears either alternative. Calling `.content(...)` after `.child(...)` selects the content constructor again.
- Selected state-driven content still builds in its own child Fiber with its own hooks and subscriptions, inherited theme, and standard removal/teardown semantics under ADR 0079. F2 must not restore parent hook borrowing.

### R6 — Preserve construction and runtime invariants

Valid handwritten and macro descriptions must agree on resulting widget types, keys, child order, geometry, scenes and event routing. Preserve single expression evaluation, sibling order, children-before-parent expression evaluation, lazy branch execution, explicit ownership, and the lack of implicit clone. Replacing an old value does not mean skipping its expression's established evaluation.

Components remain deferred through `IntoChildView` until reconciliation. Cardinality checks must not execute component builds prematurely or change F1's root/child/content boundaries. The protocol must remain usable through existing composition/key wrappers without losing their keys or changing their child capability. Unchanged keyed children retain state/focus according to existing reconciliation rules; newly selected replacement content is not promised the identity of content it replaces.

### R7 — Migrate consumers and expectations

Affected widgets, application code, examples, documentation and tests must use this one contract. Update old staged-plus-incoming rejection tests, leaf empty-block cases, Align multi-child usage, implicit-append builder calls and macro append assumptions. Remove leaf attachment boilerplate rather than preserving it as the implementation of a nominal Leaf capability. Keep unrelated layout, callback, constructor and control behavior outside this change.

## Solution

Use a component-declared capability/type-checking surface shared by handwritten construction and macro expansion. Preserve static child-count information until the receiving capability has been checked; use checked dynamic construction when count depends on runtime execution. The exact trait/generic encoding is not prescribed here. Capability selection and validation are not a macro widget-name registry.

Both root and nested parent expressions use the same replacement semantics. SingleChild applies a zero-/one-value assignment; MultiChild replaces an ordered list. Explicit append methods remain separate from block assignment. `Align` uses one real slot; InteractiveRegion assignment consistently selects or clears its child/content alternative while preserving the existing ContentSlot lifecycle.

### Seams

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Macro → construction protocol | Macro expansion → public widget construction | Receiving capability and static/dynamic incoming cardinality | Static diagnostics or checked replacement, uniform at root/child positions |
| Builders → construction protocol | Single-/multi-child builders → attachment rules | Explicit setting versus append intent | Consistent replacement/clearing and ordered append; recoverable dynamic errors |
| Public composition → external components/wrappers | External Components and existing key/composition wrappers → capabilities | Public child conversion/attachment without private AnyView access | Extensible protocol; preserved capability and key |
| Slot selection → reconciliation | InteractiveRegion child/content alternative → existing ContentSlot Fiber | Exactly the selected slot content | Independent hooks/theme/subscriptions and ordinary teardown, unchanged F1 boundary |
| Constructed tree → layout/runtime | Validated widget descriptions → reconciliation/layout | Valid cardinality and ordered children | No hidden extra Align children; unchanged valid-tree geometry, routing and lifecycle |

## End-to-End Tests

### Builder and macro replacement agree
- **Given:** SingleChild with staged keyed child `a` and incoming keyed child `b`.
- **When:** Equivalent handwritten replacement and macro replacement are mounted separately.
- **Then:** Both produce only `b` under the parent, with matching type/key, geometry, scene and routing. Empty replacement produces no child; supplying the staged component as an ordinary value retains `a` instead of clearing it.

### MultiChild setting versus appending
- **Given:** A Row/Column/Stack with existing children `[a, b]`.
- **When:** A block or `.children(...)` supplies `[c, d]`; a separate equivalent tree uses explicit append operations.
- **Then:** Setting yields `[c, d]`; appending yields `[a, b, c, d]`. Empty setting clears; empty extension preserves. Macro and handwritten setting mount equivalent ordered trees.

### Static diagnostics and public extensibility
- **Given:** Built-in and external SingleChild/MultiChild components, existing key wrappers, and leaf components.
- **When:** Compile fixtures supply SingleChild with two static children, Leaf with empty/nonempty blocks, or valid zero-/one-child and multi-child blocks.
- **Then:** Illegal forms fail at the receiving block/expression; legal forms compile. External implementations need no private AnyView or macro whitelist changes. Renamed dependency fixtures remain supported.

### Dynamic zero, one and many
- **Given:** A SingleChild with staged `a` and a runtime-controlled loop yielding zero, one or two children.
- **When:** The same incoming collection is used by recoverable handwritten construction and ordinary `view!`.
- **Then:** Zero clears; one replaces; two returns ChildConstructionError in the recoverable path and panics at macro construction with the receiver, permitted cardinality and incoming count two. No invalid tree reaches layout. Each selected expression runs once in order.

### Conditional cardinality and evaluation
- **Given:** SingleChild blocks whose `if/else` or `match` alternatives each yield zero or one child, with side-effect probes in conditions, selected/unselected child expressions and parent construction.
- **When:** Different branches are selected and mounted.
- **Then:** Valid alternatives compile and produce zero or one child, not the sum of all branches. Conditions and selected expressions run once, unselected branches do not run, children-before-parent evaluation remains intact, and component builds remain deferred.

### Align does not retain ignored children
- **Given:** Align configured with `a`, then assigned `b` or an empty block.
- **When:** Mounted and laid out using equivalent builder and macro forms.
- **Then:** Only `b` or no child exists, respectively, and ordinary zero-/one-child alignment is preserved. Two static block children fail compilation; a dynamic count two fails construction. Stack continues to retain and lay out all its ordered children.

### Interactive content replacement and teardown
- **Given:** InteractiveRegion with state-driven content owning hooks/subscriptions and an external registration, under an inherited theme.
- **When:** Parent rebuild replaces it with a child or clears it through a block, and later selects `.content(...)` again.
- **Then:** Only the selected alternative mounts; removed slot resources follow normal teardown, no stale content continues receiving updates, and reselected content uses its own child Fiber rather than parent hooks. Existing slot-isolation/theme tests remain valid.

## Decisions

- **Choice:** Setters/block assignment replace; empty assignment clears; only named append methods append.
  **Source:** Accepted refinement conversation; ADR 0080 D2–D3. This resolves the review's overwrite-versus-error inconsistency without treating a multi-value assignment as repeated setters.
- **Choice:** Public capability-based static validation, no widget-name whitelist and no leaf attachment boilerplate.
  **Source:** UI_API_REVIEW F2 and accepted refinement; ADR 0080 D1/D4.
- **Choice:** Keep ordinary `view! -> View`; dynamic cardinality errors panic there but remain recoverable in handwritten checked construction.
  **Source:** Explicitly accepted refinement limitation; ADR 0080 D5.
- **Choice:** SingleChild Align and mutually replacing InteractiveRegion child/content settings without altering content Fiber ownership.
  **Source:** UI_API_REVIEW F2, accepted refinement; ADR 0080 D6 and ADR 0079 D4.

## Test Plan

- Extend `crates/harbor-widget/tests/child_construction.rs` and `crates/harbor-widget/src/macro_tests.rs` for replacement, clearing, append ordering, errors, evaluation order, key preservation and runtime equivalence.
- Extend macro parser/expansion and trybuild fixtures for leaf-block rejection, static over-count, bounded conditional alternatives, public external capabilities and renamed dependencies. Check diagnostics, not merely failed compilation.
- Exercise dynamic 0/1/2 counts via runtime-selected data, including staged slots, so static checking cannot substitute for dynamic validation tests.
- Retain existing keyed insertion/removal/reorder, focus, theme, content-hook isolation, subscription/unmount and external-registration regression coverage; add replacement/clearing lifecycle cases.

## Out of Scope

- Findings F3–F8: callback threading/naming, composable Button, layout extension helpers, general constructor/import cleanup, numeric validation policy and named parameter types.
- Reworking F1's deferred descriptions, Fiber identity/materialization or content-slot lifecycle.
- A new DSL, widget-name whitelist, `try_view!`, or Result-returning runtime construction model.

## Limitations

Ordinary `view!` can panic for an invalid runtime-dependent child count. Compile-time checks do not require evaluating arbitrary Rust expressions or iterator lengths. Trait signatures and static-cardinality encoding remain implementation details constrained by the observable contract above, not unconfirmed behavioral assumptions.
