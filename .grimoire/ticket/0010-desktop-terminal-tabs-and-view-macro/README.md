# Desktop Terminal Tabs and Declarative View Macro

**Source:** [Spec: 0010-desktop-terminal-tabs-and-view-macro.md](../../spec/0010-desktop-terminal-tabs-and-view-macro.md)
**Ticket folder:** `.grimoire/ticket/0010-desktop-terminal-tabs-and-view-macro/`

## Overview

One ticket remains open: the multi-tab resize, scheduling, and acceptance gate. The widget foundations, keyboard interaction, the scroll and allocation observers, and the responsive side-tab product UI are implemented; their contracts are recorded in the source spec and the historical destinations below.

## Dependency Graph

```text
T0008 Acceptance <── Spec 0010 terminal allocation propagation (historical T0005)
                 <── Spec 0010 product behavior (historical T0007)
```

## Recommended Order

1. T0008 — Multi-Tab Resize, Scheduling, and Acceptance

## Ticket Index

| Ticket | Title | Outcome |
| --- | --- | --- |
| [T0008](./T0008-multi-tab-acceptance.md) | Multi-Tab Resize, Scheduling, and Acceptance | End-to-end correctness, cleanup, Windows evidence, and quality gates |

## Historical Contract Destinations

Stable historical IDs remain mapped to meaningful contract destinations. These mappings do not mark integrated acceptance passed.

| Historical ID | Contract destination |
| --- | --- |
| T0001 — Parent-Directed Flex Layout | [Spec 0010 Parent-directed layout](../../spec/0010-desktop-terminal-tabs-and-view-macro.md#parent-directed-layout) and [Flex layout table](../../spec/0010-desktop-terminal-tabs-and-view-macro.md#required-reusable-widgets-and-mechanisms); evidence in `crates/harbor-widget/tests/flex_layout.rs`. |
| T0002 — Keyed Children and Construction Protocol | [Spec 0010 Macro support API](../../spec/0010-desktop-terminal-tabs-and-view-macro.md#macro-support-api) and [Reconciliation tests](../../spec/0010-desktop-terminal-tabs-and-view-macro.md#test-plan): public child construction, explicit keys, cardinality, and state/focus-preserving sibling reconciliation. |
| T0003 — Declarative `view!` Procedural Macro | [Spec 0010 Declarative View Construction Contract](../../spec/0010-desktop-terminal-tabs-and-view-macro.md#declarative-view-construction-contract): syntax, expansion, crate paths, diagnostics, and handwritten equivalence. |
| T0004 — Desktop Interaction, Focus, Commands, and Theme | [Spec 0010 Widget Foundation](../../spec/0010-desktop-terminal-tabs-and-view-macro.md#widget-foundation): interaction states, pointer-capture release, focus traversal, shortcut and typed-action routing, and theme tokens; evidence in `crates/harbor-widget/tests/interaction_focus_actions_theme.rs` and `winit_qa.rs`. |
| T0005 — Scroll Area and Post-Layout Observation | [Spec 0010 Terminal allocation propagation](../../spec/0010-desktop-terminal-tabs-and-view-macro.md#terminal-allocation-propagation) and [Widget Foundation](../../spec/0010-desktop-terminal-tabs-and-view-macro.md#widget-foundation); evidence in `crates/harbor-widget/tests/scroll_area.rs` and `layout_observer.rs`. |
| T0006 — Multi-Terminal Tab Session Model | [Spec 0010 Application Model and Boundaries](../../spec/0010-desktop-terminal-tabs-and-view-macro.md#application-model-and-boundaries) and [Tab lifecycle](../../spec/0010-desktop-terminal-tabs-and-view-macro.md#tab-lifecycle): independent resources, stable IDs, events, active registrations, close selection, and cleanup. |
| T0007 — Responsive Side Tab Product UI | [Spec 0010 Product Behavior](../../spec/0010-desktop-terminal-tabs-and-view-macro.md#product-behavior) and [Resize and responsive layout](../../spec/0010-desktop-terminal-tabs-and-view-macro.md#product-behavior): 200dp/56dp rail, keyed tabs, and one active bridge. |
