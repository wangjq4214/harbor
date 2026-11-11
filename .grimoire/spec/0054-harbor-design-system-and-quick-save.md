# Harbor Design System and Liquid-Glass Quick Save

**Spec ID:** 0054
**Status:** Draft — scope and design direction confirmed; not runtime acceptance
**Date:** 2026-11-11
**Sources:** The owner approved Astryx-referenced, native liquid-glass foundations with Quick Save first, then placement under `docs/`. The latest screenshot refinement approved summary-first review, on-demand correction, retained naming/confirmation stages and revision of the existing specifications only. In the subsequent conversation, the owner approved palette-input-row naming geometry and spaced environment segments with neutral glass hover, no independent pressed appearance, retained lightly accented selection/marker and visible keyboard focus; only these existing specification documents are to be updated.

## Canonical specification

The complete foundations, page rules, integration boundaries, acceptance scenarios and source traceability live in the [Harbor application design system](../../docs/app/design-system.md). Maintain that body only to prevent duplicate requirements from drifting.

This entry retains Spec 0054's identity. The summary-first and subsequent naming/selector refinements update [0053](./0053-quick-save-review-ui-and-reusable-controls.md)'s affected presentation clauses in tandem with the canonical body. Naming geometry and segment states are specified in canonical Q1/Q3 and E1/E3/E13; this entry does not duplicate that contract. [0052](./0052-named-session-profiles-and-quick-save.md)'s two-stage flow, return-to-edit capability and save-safety contract remain.

Relocation and the summary-first/naming-selector specification refinements performed no implementation or native QA and authorized no CONTEXT/ADR writes. Geometry and selector-state checks were NOT RUN at the specification handoff. The owner subsequently invoked `grimoire-loop` explicitly for implementation and scoped verification; [current evidence and remaining limitations](../../docs/app/design-system.md#namingselector-implementation-checks) are maintained in the canonical body, not duplicated here. Automated/offscreen success does not establish native visual acceptance. No tickets, implementation-plan artifacts, commits or CONTEXT/ADR changes are authorized by these refinements.
