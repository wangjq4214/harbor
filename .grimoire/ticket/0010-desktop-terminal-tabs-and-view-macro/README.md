# Desktop Terminal Tabs and Declarative View Macro

**Source:** [Spec: 0010-desktop-terminal-tabs-and-view-macro.md](../../spec/0010-desktop-terminal-tabs-and-view-macro.md)
**Ticket folder:** `.grimoire/ticket/0010-desktop-terminal-tabs-and-view-macro/`

## Overview

These tickets deliver one Windows-first vertical slice: reusable widget foundations, an internal experimental `view!(cx, ...)` macro, application-owned multi-terminal sessions, a responsive side tab rail, and product-level resize/scheduling/resource evidence. They do not build a complete Flutter-compatible widget catalog.

## Dependency Graph

```text
T0001 Layout foundation ───────────────┬──> T0005 Scroll/layout observation ──┐
                                      │                                      │
T0002 Key/children construction ──> T0003 view! macro ───────────────────────┤
                                      │                                      │
T0004 Interaction/theme/commands ─────┼──────────────────────────────────────┤
                                      │                                      v
T0006 Tab/session host model ──────────┴───────────────────────────────> T0007 Product UI
                                                                            │
                                                                            v
                                                                     T0008 Acceptance
```

T0001, T0002, T0004, and T0006 can be developed as separate conceptual slices, but T0001/T0002 both touch View/Fiber contracts and should not mutate the same checkout concurrently. T0003 follows T0002. T0005 follows T0001. T0007 consumes every preceding contract. T0008 is the final integration and evidence gate.

## Recommended Order

1. T0001 — Parent-Directed Flex Layout
2. T0002 — Keyed Children and Construction Protocol
3. T0003 — Declarative `view!` Procedural Macro
4. T0004 — Desktop Interaction, Focus, Commands, and Theme
5. T0005 — Scroll Area and Post-Layout Observation
6. T0006 — Multi-Terminal Tab Session Model
7. T0007 — Responsive Side Tab Product UI
8. T0008 — Multi-Tab Resize, Scheduling, and Acceptance

## Ticket Index

| Ticket | Title | Outcome |
| --- | --- | --- |
| [T0001](./T0001-parent-directed-flex-layout.md) | Parent-Directed Flex Layout | Correct fixed rail plus expanded terminal allocation |
| [T0002](./T0002-keyed-children-construction.md) | Keyed Children and Construction Protocol | Stable dynamic identity and macro-ready child attachment |
| [T0003](./T0003-declarative-view-proc-macro.md) | Declarative `view!` Procedural Macro | Readable static and dynamic widget tree syntax |
| [T0004](./T0004-desktop-interaction-focus-theme.md) | Desktop Interaction, Focus, Commands, and Theme | Reusable tab-item behavior and keyboard commands |
| [T0005](./T0005-scroll-area-layout-observer.md) | Scroll Area and Post-Layout Observation | Overflowing rails and coalesced terminal allocation feedback |
| [T0006](./T0006-multi-terminal-tab-model.md) | Multi-Terminal Tab Session Model | Independent Terminal/PTY resources and tab-qualified events |
| [T0007](./T0007-responsive-side-tab-ui.md) | Responsive Side Tab Product UI | Macro-authored expanded/compact rail and active terminal |
| [T0008](./T0008-multi-tab-acceptance.md) | Multi-Tab Resize, Scheduling, and Acceptance | End-to-end correctness, cleanup, Windows evidence, and quality gates |
