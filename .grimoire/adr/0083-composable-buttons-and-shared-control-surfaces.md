# Composable Buttons and Shared Control Surfaces

**Status:** Proposed
**Date:** 2026-10-23
**Sources:** historical UI/API review F4/P1; refinement conversation approving the recommended responsibility split, composable Button contract, constructor migration, single-line ellipsis and retained InteractiveRegion style entry point.
**Related:** [ADR 0057](./0057-widget-vector-icons-without-font-glyphs.md), [ADR 0064](./0064-widget-and-application-component-ownership.md), [ADR 0079](./0079-defer-root-construction-and-unify-content-slot-boundaries.md), [ADR 0080](./0080-unify-child-cardinality-and-replacement-semantics.md), [ADR 0081](./0081-preserve-click-naming-and-standardize-disabled-builders.md), [ADR 0082](./0082-ui-thread-local-widget-callbacks.md).

## Context

Button currently delegates a fixed label to InteractiveRegion. InteractiveRegionView estimates label width from scalar count and paints the text itself, while TextLabel already measures renderer advances. IconButton composes an Icon through a state-driven content slot but separately stores and forwards the same interaction configuration. This limits custom button content and creates competing text measurement paths.

Application tabs, search and palette already use InteractiveRegion style composition. Removing that entry point would force unrelated caller restructuring. F4 requires separating responsibilities without copying the shared input state machine or mechanically adding public types and Fibers.

The user accepted the recommended design and selected one spec without ticket slicing or implementation. This records the selected direction, not realized behavior or test acceptance.

## Decision

1. **Separate interaction, surface and content responsibilities within reusable widget composition.** The existing shared interaction state machine remains authoritative for retained interaction state, activation, focus and pointer capture. Shared control-surface logic owns background, border, internal padding, minimum dimensions and focus-ring presentation. TextLabel and Icon own content measurement and drawing. InteractiveRegion's event/render implementation must no longer contain its own label measurement/painting path. Do not copy input handling into Button or IconButton.
2. **Make Button a single-slot composition with explicit conveniences.** Use `Button::new().child(custom_content)`, `Button::text(text)` and `Button::icon(icon, text)`; the last paints both an icon and visible text. Remove the old `Button::new(text)` signature and migrate its consumers without a compatibility entry point. State-driven `.content(...)` and `.child(...)` select the same replaceable slot under ADR 0080, and state-driven content retains the independent child-Fiber ownership of ADR 0079. This is a targeted F4 constructor change, not general F6 cleanup.
3. **Keep IconButton as a convenience with a distinct theme role, not an independent behavioral implementation.** It shares interaction and surface capabilities with Button while retaining `theme.icon_button`, its icon-only rendering, label metadata and existing icon configuration. Button uses `theme.button` unless explicitly styled. Built-in content resolves foreground from control state; custom content retains ownership of its own colors.
4. **Retain `InteractiveRegion::style(...)` as a composition entry point.** Separate the implementation responsibilities behind that entry point rather than requiring callers to add a new public surface component. Existing state-driven application composition remains supported. Private implementation/type choices are open; responsibility separation does not require one public type or an extra Fiber for every responsibility. The existing independent state-driven content Fiber is still required.
5. **Preserve the established interaction/API contracts.** Button/IconButton retain `on_click`, `disabled`, stable focus-handle forwarding and UI-local callbacks under ADRs 0081/0082. Nested first refusal, capture/release, keyboard timing and lifecycle/teardown ordering remain unchanged. Generic widget mechanisms do not acquire application business commands or visual policy.

## Consequences

- Button content becomes composable without a second interaction state machine; IconButton can retain its useful icon-only API without drifting in behavior or surface configuration.
- The fixed-label construction API intentionally breaks. Consumers, tests and documentation migrate to `Button::text(...)`; no indefinite dual constructor model is selected.
- Built-in text uses the TextLabel measurement path and single-line ellipsis under narrow constraints, as specified by the accepted F4 contract. Correct measurement may change old dimensions; layout and visual evidence are required rather than claiming pixel equivalence with character-count estimates.
- Keeping InteractiveRegion's style facade avoids mandatory application restructuring while still requiring internal responsibility separation. Merely adding Button child support and leaving region-owned label drawing does not fulfill the decision.
- No universal style system, mandatory new public component hierarchy, text-shaping redesign or application-to-widget ownership inversion is selected.
- Implementation and acceptance remain future work. See [spec 0037](../spec/0037-composable-buttons-and-content-measurement.md) for the requirements and verification contract.
