# Desktop Terminal Tabs and Declarative View Macro

**Source:** [Spec: 0010-desktop-terminal-tabs-and-view-macro.md](../../spec/0010-desktop-terminal-tabs-and-view-macro.md)
**Ticket folder:** `.grimoire/ticket/0010-desktop-terminal-tabs-and-view-macro/`

## Overview

These tickets deliver one Windows-first vertical slice: reusable widget foundations, an internal experimental `view! { cx; root }` macro, application-owned multi-terminal sessions, a responsive side tab rail, and product-level resize/scheduling/resource evidence. They do not build a complete Flutter-compatible widget catalog.

## Dependency Graph

```text
T0001 Layout foundation ──> T0005 Scroll/layout observation
T0001, T0004, T0005 ───────> T0007 Product UI ──> T0008 Acceptance
T0005 ────────────────────────────────────────> T0008 Acceptance
```

T0005 follows T0001. T0007 consumes layout, interaction/theme, scrolling, and the preserved construction, macro, and tab/session contracts below. T0008 remains the independent final integration and evidence gate, consuming T0005, T0007, and the tab/session contract.

## Recommended Order

1. T0001 — Parent-Directed Flex Layout
2. T0004 — Desktop Interaction, Focus, Commands, and Theme
3. T0005 — Scroll Area and Post-Layout Observation
4. T0007 — Responsive Side Tab Product UI
5. T0008 — Multi-Tab Resize, Scheduling, and Acceptance

## Ticket Index

| Ticket | Title | Outcome |
| --- | --- | --- |
| [T0001](./T0001-parent-directed-flex-layout.md) | Parent-Directed Flex Layout | Correct fixed rail plus expanded terminal allocation |
| [T0004](./T0004-desktop-interaction-focus-theme.md) | Desktop Interaction, Focus, Commands, and Theme | Reusable tab-item behavior and keyboard commands |
| [T0005](./T0005-scroll-area-layout-observer.md) | Scroll Area and Post-Layout Observation | Overflowing rails and coalesced terminal allocation feedback |
| [T0007](./T0007-responsive-side-tab-ui.md) | Responsive Side Tab Product UI | Macro-authored expanded/compact rail and active terminal |
| [T0008](./T0008-multi-tab-acceptance.md) | Multi-Tab Resize, Scheduling, and Acceptance | End-to-end correctness, cleanup, Windows evidence, and quality gates |

## Historical Contract Destinations

Stable historical IDs remain mapped to meaningful contract destinations. These mappings do not mark integrated acceptance passed.

| Historical ID | Contract destination |
| --- | --- |
| T0002 — Keyed Children and Construction Protocol | [Spec 0010 Macro support API](../../spec/0010-desktop-terminal-tabs-and-view-macro.md#macro-support-api) and [Reconciliation tests](../../spec/0010-desktop-terminal-tabs-and-view-macro.md#test-plan): public child construction, explicit keys, cardinality, and state/focus-preserving sibling reconciliation. |
| T0003 — Declarative `view!` Procedural Macro | [Spec 0010 Declarative View Construction Contract](../../spec/0010-desktop-terminal-tabs-and-view-macro.md#declarative-view-construction-contract): syntax, expansion, crate paths, diagnostics, and handwritten equivalence. |
| T0006 — Multi-Terminal Tab Session Model | [Spec 0010 Application Model and Boundaries](../../spec/0010-desktop-terminal-tabs-and-view-macro.md#application-model-and-boundaries) and [Tab lifecycle](../../spec/0010-desktop-terminal-tabs-and-view-macro.md#tab-lifecycle): independent resources, stable IDs, events, active registrations, close selection, and cleanup. |
