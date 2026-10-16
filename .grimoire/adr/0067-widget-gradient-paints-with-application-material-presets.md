# Widget Gradient Paints with Application Material Presets

**Status:** Testing
**Date:** 2026-10-16
**Sources:** The user requested a more translucent glass appearance without changing the search panel's corners, emphasized correct widget capability abstraction, and approved the recommendation to reuse local blur and add generic gradient painting while keeping the material preset application-owned.
**Related:** [ADR 0064](./0064-widget-and-application-component-ownership.md), [ADR 0066](./0066-widget-local-backdrop-blur-with-application-glass-presets.md), [decoration spec 0009](../spec/0009-widget-decoration-and-terminal-chrome.md), [search appearance spec 0029](../spec/0029-search-panel-glass-appearance.md).

## Context

The current search panel already composes widget-owned local backdrop blur and box decoration. Its uniform translucent gray fill and uniform border do not express the subtle surface reflection and varying edge highlights requested in the glass reference. The current `BoxDecoration` and `Border` accept solid colors; rounded geometry, outer shadows and local backdrop blur already have reusable owners.

This is a need for more expressive generic painting, not for moving search composition or a fixed glass appearance into widget defaults. Decoration spec 0009 excluded gradients from its first version and explicitly left their addition as future decoration inputs; this decision selects that bounded extension without changing terminal ownership.

## Decision

1. Add a reusable paint abstraction in `harbor-widget` that supports solid colors and linear gradients and can be used by both decoration fills and border strokes. Gradient inputs describe direction and color stops, including alpha, rather than a named glass or search effect.
2. Keep rounded-edge rendering, antialiasing and correct transparent composition with the widget rendering mechanism. Preserve existing solid-color interfaces and behavior; ordinary consumers must not need to migrate merely because gradients become available.
3. Reuse `BackdropFilter`, box geometry/clipping and outer-shadow capabilities rather than introducing parallel glass-specific implementations. Background filtering remains separate from foreground painting so text, caret and icons stay sharp.
4. Keep the glass composition and its tint, opacity, blur strength, reflection/highlight direction and shadow parameters application-owned. Input styling, button appearance, layout, result semantics and commands remain application-owned as well.
5. Do not introduce search-specific paint APIs or a comprehensive `Glass` component for this change. Refraction, dynamic lighting, noise and an arbitrary-filter framework are not selected. API names, exact gradient coordinate/stop rules, GPU representation and numeric material values remain unselected.

## Consequences

- Surface gradients and directional edge highlights can also serve non-terminal cards and controls without depending on Harbor search/session types.
- Glass appearance changes can be expressed as application preset changes; generic gradient rendering correctness has one widget-layer owner.
- The new painting capability must retain the established decoration/layout/clipping and solid-color contracts. Search appearance refinement preserves the existing corners and layout rather than using a capsule shape to suggest glass.
- This extends, rather than supersedes, ADRs 0064 and 0066. It records an approved direction, not completed gradient implementation or visual acceptance.
