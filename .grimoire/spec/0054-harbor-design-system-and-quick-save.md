# Harbor Design System and Liquid-Glass Quick Save

**Spec ID:** 0054
**Status:** Draft — scope and design direction confirmed; not runtime acceptance
**Date:** 2026-11-11
**Sources:** The owner approved Astryx-referenced, native liquid-glass foundations with Quick Save first, then placement under `docs/`. The latest screenshot refinement approved summary-first review, on-demand correction, retained naming/confirmation stages and revision of the existing specifications only. In the subsequent conversation, the owner approved palette-input-row naming geometry and spaced environment segments with neutral glass hover, no independent pressed appearance, retained lightly accented selection/marker and visible keyboard focus; only these existing specification documents are to be updated. On 2026-11-26, the owner approved upgrading the shared `design_system` `Primary` and `Secondary` button roles across Quick Save and confirmation overlays ([Spec 0057](./0057-in-window-confirmation-overlay.md)) to true native liquid-glass pill buttons (tinted glacier-blue glass `Primary`, clear neutral glass `Secondary` in idle/hover/pressed, specular `BackdropRim` without flat 1px borders, and softened focus rings); spec-only update.

## Canonical specification

The complete foundations, page rules, integration boundaries, acceptance scenarios and source traceability live in the [Harbor application design system](../../docs/app/design-system.md). Maintain that body only to prevent duplicate requirements from drifting.

This entry retains Spec 0054's identity. The summary-first, naming/selector, and 2026-11-26 liquid-glass pill button refinements update [0053](./0053-quick-save-review-ui-and-reusable-controls.md) and [0057](./0057-in-window-confirmation-overlay.md)'s affected presentation clauses in tandem with the canonical body. Naming geometry, segment states, and liquid-glass pill button roles are specified in canonical D5/Q1/Q3/Q4 and E1/E3/E13/E14; this entry does not duplicate that contract. [0052](./0052-named-session-profiles-and-quick-save.md)'s two-stage flow, return-to-edit capability and save-safety contract remain.

Relocation and the specification refinements performed no implementation or native QA and authorized no CONTEXT/ADR writes. Subsequent explicitly authorized `grimoire-loop` implementation and automated/offscreen GPU verification for the 2026-11-26 liquid-glass pill button refinement are recorded in the [canonical design-system evidence](../../docs/app/design-system.md#liquid-glass-pill-button-refinement-checks) (interactive native visual acceptance remains `NOT RUN`). Automated/offscreen success does not establish native visual acceptance. No tickets, implementation-plan artifacts, commits or CONTEXT/ADR changes are authorized by these refinements.
