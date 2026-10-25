# General Composition Naming, Queries, and Root Imports

**Ticket ID:** T0003
**Source:** [spec 0041](../../spec/0041-widget-api-naming-and-configuration-validation.md) R1 (ActionHandler/ShortcutMap), R2 (except the IconButton getter), R3 (Focus), R7 (root imports), R8; [ADR 0087](../../adr/0087-widget-api-naming-and-validation-boundaries.md) D1/D3/D8; [related file](./README.md).
**Status:** Todo

## Goal

Express wrapper construction, content replacement, focus eligibility, and configuration queries with one public vocabulary; complete the recommended root imports while preserving the actual slot, focus, shortcut, and scroll semantics.

## Affected Surfaces

- `widgets/{actions,shortcuts,focus,button,interactive_region,layout_observer,padding,decorated_box,row,column,flex,sized_box,scroll_view}.rs` and the target module names.
- `lib.rs`, `widgets/mod.rs`, and the root re-exports of Stack/TextLabel plus Component/BuildCx/View.
- Real application composition, macro/cross-crate fixtures, slot/focus/scroll/shortcut regressions, and the current widget tutorials and examples.

## Approach

Migrate types/constructors/methods together with their direct callers. Add the .child setter along the existing capability protocol; do not duplicate state machines, move content Fibers, widen visibility, or create a prelude just for name consistency.

The query renames keep the distinction between "configuration/request" and "committed value"; coordinate with T0001's Result signatures and do not use this as an opportunity to add intermediate chained macro rewriting.

## Dependencies and Coordination

- **Blocked by:** none; may proceed in parallel with T0001 based on the fixed contracts and coordination.
- **Blocks:** T0006 final public/composition acceptance.
- **Coordination risks:** shares Button/InteractiveRegion/SizedBox and declarations with T0001; shares root/module exports with T0004/T0005. Each type ticket owns the exports of its own new names; this ticket only completes the general root imports and does not export types that are not yet implemented.

## Acceptance

- [ ] Actions/Shortcuts are fully migrated to ActionHandler/ShortcutMap with modules/exports/consumers synchronized, and no old type/path aliases remain; KeyChord normalization and first-match are preserved.
- [ ] Focus/ShortcutMap use new() and ActionHandler uses new(handler); all three have .child; the empty/old child constructors and the synonymous handler constructor entries are removed.
- [ ] Button/InteractiveRegion content is fully migrated to child_with; child and child_with replace each other, an empty macro block clears, and the static/dynamic count checks plus the independent content Fiber/hooks/subscriptions/unmount are unchanged.
- [ ] LayoutObserver uses from_shared_callback, preserving real shared-callback ownership and call timing; required callbacks are not replaced by meaningless defaults.
- [ ] Focus uses focus_handle/tab_order/focusable without changing focus eligibility, the meaning of disabled/read_only, or the subtree input policy.
- [ ] Fully migrate Padding get_insets, DecoratedBox get_decoration/get_clip_behavior, Row/Column/Flex get_options, and SizedBox get_requested_width/get_requested_height; keep Align get_insets and avoid mechanical naming on immutable values/Controllers.
- [ ] ScrollController requested_offset and ScrollMetrics offset keep accurate meanings before and after the actual request → successful layout commit, and request normalization plus the original scroll actions are preserved.
- [ ] Root exports include Stack/TextLabel, Component/BuildCx/View, and the existing construction/extension traits, with configuration available in its owning module; there is no new prelude, no all-internal export, and no bypass of U3's private boundaries.
- [ ] The spec E1/E7 paths for this ticket pass: public compilation, handwritten/macro construction, mounted slot/key/focus, scrolling, and real consumers; the old entries covered by this ticket are rejected, and docs/fixtures/application are migrated in step.

## Out of Scope

T0001's appearance/text API, T0004's notifications and CrossAxisAlignment, T0005's ExternalSurface; generic Props/Container, additional runtimes, cross-parent identity guarantees, or changes to shortcut policy.
