# Fixed Tab-Rail Actions and Centered Compact Tab Icons

**Spec ID:** 0055
**Status:** Settled requirements — not implementation or runtime acceptance
**Date:** 2026-11-11
**Sources:** The owner's two-turn `grimoire-refine` request: separate the side rail into an independently scrolling upper tab area and a fixed lower icon-button area, move New tab and Save below, and show function hints on hover. The owner then selected a spec, accepted the recommendation to include New from profile with horizontal actions in the expanded rail and vertical actions in the compact rail, and required horizontally centered icons instead of numbers for compact tabs.

## Requirements

### R1 — Independently scrolling tabs, fixed bottom actions

- Split the side rail into an upper tab list and a distinct lower action area. The tab list receives the remaining height above the actions and scrolls vertically when its content overflows.
- Anchor the action area at the bottom of the rail even when only a few tabs exist. It is outside the tab list's scrolling content: scrolling to any tab must not move, hide or require scrolling to reach the actions.
- Visually separate the two areas with a restrained boundary consistent with the application design system. Tabs must not paint or receive pointer hits through the action area.
- Creating or closing tabs changes the list, not the bottom anchoring of the action area. Preserve existing tab ordering and active-tab visibility behavior.

### R2 — Compact icon-only action area

- Move all three existing rail actions into the bottom area: New terminal/tab, New from profile, and Save current session as preset. Do not leave duplicate copies in the scrolling list.
- In expanded presentation, arrange the actions horizontally. In compact presentation, arrange them vertically.
- In both presentations, use compact icon-only controls, not text-captioned full-width rows or large buttons. Compactness must retain usable interactive targets and visible keyboard focus; this contract does not invent new pixel dimensions.
- Hovering each action shows a visible tooltip describing its actual function. Hidden metadata alone does not satisfy hover help. Showing a tooltip must not resize either area or activate an action.
- Retain distinct, understandable artwork and existing hover, activation and disabled-state semantics. Tooltip wording and exact icon sizing remain presentation details; hints must distinguish ordinary New from New from profile and identify Save as saving the current session as a preset.

### R3 — Horizontally centered icons for compact tabs

- In compact presentation, replace the numeric abbreviation with a tab icon. Do not display the tab ID, its last digit, an ordinal or another numeric substitute as the compact label.
- Center each tab's main icon horizontally within its tab item. Do not retain an empty title slot that shifts the icon to one side.
- Keep active selection, keyboard focus and unread state discernible. An unread indication must not displace the main icon from horizontal center; this changes presentation, not unread policy.
- Preserve tooltip-accessible tab titles from Spec 0010 so users can identify icon-only tabs. A title whose content includes digits is valid tooltip text; the prohibition concerns numeric substitutes rendered as compact tab labels.
- Expanded tabs retain their existing title, icon, selection and close affordances. This change does not add a compact close button or remove existing close commands.

### R4 — Retained responsive and command contracts

- Retain the existing responsive rule: expanded presentation at window logical widths of at least 900, with a preferred 200dp rail; compact presentation below 900, with a 56dp rail. These values come from Spec 0010 and current `RailPresentation`, not a new breakpoint decision.
- Crossing the breakpoint changes only presentation. Preserve tab identity/order, the active session, eligible focus behavior and the existing terminal allocation/resize contract.
- The bottom New action retains ordinary shell launch behavior. New from profile opens the existing shared chooser. Save opens the existing naming/review flow; clicking the rail icon or displaying its hint never constitutes final persistence authorization.
- Retain keyboard command routing, source-aware focus, enabled/disabled admission and existing popup lifetimes. The layout change must not dispatch duplicate commands or leak consumed input to the terminal.

## Solution

The rail is an application-owned composition of two sibling regions: a height-constrained scrolling tab region above a non-scrolling bottom action region. The existing responsive presentation selects action orientation and tab content; it does not select different business commands or a new session model.

Use the established vector icon mechanism and existing generic layout, interaction, scrolling and hover-help mechanisms where applicable. This spec requires visible hover help but does not assert that current icon metadata already implements it or select a new tooltip API. Reusable rendering/interaction mechanisms remain widget-owned; tab identity, action meanings, layout composition and commands remain application-owned.

### Seams

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Tab projection | `TabUiController` / `TabSnapshot` → rail composition | Current ordered tabs, stable IDs, title, active/unread state, focus handles and responsive presentation | Correct expanded/compact content without replacing session identity or changing unread policy |
| Scroll/layout | Rail allocation → upper list and lower actions | Bounded workspace height and existing presentation width | Remaining-height tab viewport, clipping/hit isolation and independently bottom-anchored controls |
| User intent | Tab/action controls → existing `AppCommandRequest` dispatcher | Activate the selected stable tab; ordinary New; profile chooser; Save current session | The same existing Host command path and authorization/focus behavior, once per activation |
| Hover help | Control/title meaning → visible hint | Function or current tab title and actual hover target | Readable help without command execution or layout displacement |

Relevant current surfaces are [`tab_view_ui.rs`](../../crates/harbor-app/src/tab_view_ui.rs) for composition and controls and [`tab_view.rs`](../../crates/harbor-app/src/tab_view.rs) for presentation and tab focus/scroll coordination. These paths locate existing responsibilities, not a mandated implementation plan.

## End-to-End Tests

| ID | Given / When | Then |
| --- | --- | --- |
| E1 | Expanded rail with enough tabs to overflow; scroll from first to last tab and back | Only the upper list moves; all three bottom actions remain fixed and reachable; no tab paints or receives hits in the action area; actions remain horizontal (R1/R2) |
| E2 | One/few tabs; create and close tabs until the list overflows and then becomes short again | Actions stay at the bottom rather than immediately following the final tab; list height/scrolling adapts without duplicate actions (R1) |
| E3 | Hover each bottom action in both presentations, then activate it | A distinct function hint appears without moving content or dispatching commands; activation invokes ordinary New, the existing profile chooser or the existing Save flow respectively, once; Save still requires its existing final confirmation (R2/R4) |
| E4 | Compact rail with several tabs, including IDs beyond one digit and an unread background tab | Each tab has a horizontally centered main icon and no numeric abbreviation; selection/focus/unread remain distinguishable; unread updates do not shift the icon; hovering reveals the tab title (R3) |
| E5 | Resize across the existing 900-logical-pixel breakpoint in both directions | Actions switch horizontal/vertical orientation; titles switch to centered icons and back; active session, tab ordering and eligible focus behavior remain coherent; no unintended launch/save occurs (R2–R4) |
| E6 | Scroll to an offscreen tab, activate it, then use existing keyboard navigation and close commands | The intended stable tab is activated/closed under existing rules; icon-only presentation does not remove keyboard operation or let events target another tab (R1/R3/R4) |
| E7 | Resize the window height while tabs overflow, in both presentations and at existing supported DPI scales | The viewport adapts above the actions; action icons and hover hints remain usable without overlap or clipping in supported window geometry; the terminal retains its normal allocation behavior (R1–R4) |

## Decisions and Source Traceability

- **Two separate regions and icon-only actions:** Owner's initial request. This is a layout/presentation refinement, not a command or persistence redesign.
- **Three actions, horizontal expanded / vertical compact:** Prior recommendation explicitly accepted by the owner's second request, “按照建议进行”.
- **Centered compact icons, no numbers:** Owner's explicit second-turn addition. This narrows Spec 0010's earlier “icons/abbreviations” allowance to icons; the old numeric choice is no longer permitted for compact labels. Other tab/session contracts remain unchanged.
- **Retained breakpoint, title hints and session behavior:** [Spec 0010](./0010-desktop-terminal-tabs-and-view-macro.md), especially “Resize and responsive layout”, plus current `RailPresentation` and tab controls. No existing spec is rewritten by this document; this focused contract governs the changed presentation.
- **Retained chooser and save authorization:** [Spec 0052](./0052-named-session-profiles-and-quick-save.md), especially R1/R4/R5/R7, and the current commands dispatched by the three rail actions. Existing [Spec 0054](./0054-harbor-design-system-and-quick-save.md) / [application design system](../../docs/app/design-system.md) continue to govern Quick Save itself and shared visual roles; this spec changes only its rail entry presentation.
- **Vector icons and ownership:** [ADR 0057](../adr/0057-widget-vector-icons-without-font-glyphs.md) (Testing) and [ADR 0064](../adr/0064-widget-and-application-component-ownership.md) (Proposed) support established icon and mechanism/business boundaries. Their recorded statuses are not changed here; no conflicting architecture decision is selected.

## Verification and Handoff

- Verify rendered geometry and real pointer behavior, not only component names: the bottom region is outside the scroll content; action orientation is correct; compact tab icon centers stay aligned through unread/selection changes; tooltips actually appear.
- Cover both sides of the existing breakpoint, short and overflowing lists, height changes and existing 100/150/200% DPI cases from Spec 0010. Include mounted interaction checks for command dispatch and tooltip placement, and native visual checks for density, alignment and hover usability.
- No new exact button size, tooltip timing, separator material or icon asset is selected. These are reversible presentation details within the observable requirements, not unresolved business decisions.
- **Endpoint/permissions:** Spec only. No tickets, implementation plan, production/test code, commits or CONTEXT/ADR changes are authorized by this handoff. No blocking requirement question remains.
- **Knowledge recording:** No CONTEXT/ADR write authorization was supplied. This turn adds task-specific presentation requirements, recorded here; no separate durable domain/architecture batch is needed.
- **Evidence boundary:** Specification review establishes requirement coverage and source/link consistency only. Runtime, automated implementation tests and native visual acceptance have not been performed by this refinement.

## Out of Scope

Changing profile launch/save semantics, popup flows, shell/PTY ownership, tab identity/order/close policy, shortcut bindings, responsive breakpoints/rail widths or terminal themes. This spec does not redesign Quick Save, add drag/reorder/grouped tabs, or turn the rail into a drawer or bottom navigation bar.
