# Single-Row Search Panel with Local Glass Backdrop

**Spec ID:** 0029
**Status:** Draft
**Date:** 2026-10-16
**Sources:** The conversation contract below; [ADR 0066](../adr/0066-widget-local-backdrop-blur-with-application-glass-presets.md), applying [component ownership ADR 0064](../adr/0064-widget-and-application-component-ownership.md) and [layered input ADR 0065](../adr/0065-layered-widget-text-input.md).
**Endpoint:** Spec only. This document does not authorize tickets, an implementation plan, code changes or execution of acceptance tests.

## Source Contract

- **S1 — Requested appearance:** Put the input, result information and buttons on one row, with result information before the buttons. Default to the exact text `No Result`; show current-match/total information when matches exist. Buttons blend into the panel without their own background color. Give the overall panel a soft glass appearance, and give the input rounded corners and a translucent black fill that blends in while remaining identifiable as an input.
- **S2 — Confirmed choices:** The user selected B: real local blur of terminal content behind the panel, not merely a translucent-card approximation. Foreground input, result text and buttons remain sharp. The user chose to preserve navigation behavior: do not automatically select or navigate to the first match; show `— / N` before navigation and `i / N` after navigation.
- **S3 — Selected route:** After these choices and the ownership boundary were summarized, the user approved continuing with spec only, without ticket decomposition or implementation.
- **S4 — Existing constraints:** [Search spec 0027, R4–R7](./0027-revisioned-terminal-read-model-and-literal-search.md) requires validated search/navigation and distinguishes partial coverage from complete no-match results. [Input spec 0028](./0028-reusable-single-line-text-input.md) and ADR 0065 keep editing reusable and search-specific semantics application-owned. [Decoration spec 0009](./0009-widget-decoration-and-terminal-chrome.md) supplies existing decoration/clipping constraints; its first-version exclusion of filters does not itself supply the new blur capability.
- **S5 — Confirmed follow-ups:** The user reported status-driven input/result movement and approved immediate search without input debounce and a busy hint only after about 200ms. The later explicit correction replaces the proposed standalone `—` pending label with the unchanged initial `No Result` text during that delay. This is a presentation fallback, not completed negative search evidence. Keep fixed result-slot geometry, do not reuse old-query counts, and retain the existing `— / N` / `i / N` navigation format.

## Requirements

### R1 — One-row composition

Order the search panel content as input → result information → previous-match button → next-match button → close button. Result information must no longer occupy a second row. Input, result information and buttons must be vertically aligned as one coherent search bar.
Changing status wording or count digit length must not resize the input or shift the result slot/buttons horizontally within the same panel allocation. Source: S5.

Keep the existing search overlay role. Do not obtain the new appearance by moving search into terminal text/content or replacing its established command semantics.

Sources: S1; S4; ADR 0064.

### R2 — Result presentation without automatic navigation

Use the following visible result contract:

| Search state | Required presentation |
| --- | --- |
| Newly opened or empty query | `No Result` |
| Complete search with zero matches | `No Result` |
| Valid results with N > 0 and no selected match | `— / N` |
| Valid results with a selected match | `i / N`, with i one-based and corresponding to the current selected match |
| New query pending without valid presented results, before 200ms | Keep the initial `No Result` text unchanged; work is still pending internally and no previous-query count is shown |
| Work still pending at/after 200ms | `Searching…`, optionally with currently valid same-query result information; completion/cancellation clears the pending hint |
| Refreshing with valid same-query results, or partial/limited coverage | Retain an explicit coverage qualification; delay the busy hint by 200ms, but never portray incomplete coverage as a complete result |

Counts and current position must describe the currently valid presented results, not obsolete query/session data. Any count shown during partial coverage or refresh must carry the corresponding qualification. Exact compact wording for these qualifications is not fixed here.

Search work starts immediately on committed edits; do not add input debounce or a minimum result-publication delay. Fast completions publish results without flashing `Searching…`. Pending host turns must publish the delayed hint even without another keystroke; finished, cancelled, empty or replaced queries must not inherit an old busy deadline. Source: S5.

Receiving matches must not automatically select a match or change the terminal viewport. Navigation updates the displayed position after successful selection/navigation. Preserve existing previous/next ordering and wraparound behavior. If existing invalidation clears the selected match, do not retain its old ordinal in the display.

Sources: S1, S2, S5; S4 search validity and coverage contracts; existing `search.rs::navigate_search` behavior.

### R3 — Buttons integrated into the panel

Previous, next and close remain recognizable icon controls, but have no independent background fill or resting button tile. Their backgrounds remain transparent across interaction states rather than reintroducing the existing solid hover/pressed/disabled backgrounds. Preserve interaction and keyboard-focus feedback without requiring a separate background fill; the exact foreground/focus treatment is not fixed.

Apply this as the application's search-button style, not as a change to every `IconButton` default.

Sources: S1; ADR 0064; existing `IconButton.style()` and state-resolved `ControlStyle`.

### R4 — Real local soft-glass backdrop

The overall panel, not just the input, must have the requested soft, translucent glass appearance. Its background must locally blur the content behind it, including terminal text drawn through the embedded terminal presentation. Merely making the panel translucent, blurring a shadow, or relying on Windows Acrylic behind the window does not satisfy this requirement.

The panel's own input text, caret, result information and button icons remain sharp. The blur is confined to the panel's intended backdrop region and respects its geometry and applicable clipping; it must not blur the whole terminal or unrelated foreground content. Changes to terminal content behind the visible panel must be reflected in the backdrop rather than leaving a frozen capture.

The application chooses the panel's glass visual parameters. Exact tint, opacity, radius, blur strength, border and shadow values are not fixed by this contract; they must be evaluated against the requested soft-glass appearance and foreground readability rather than represented as user-approved numbers.

Sources: S1, S2; ADR 0066; S4 decoration/clipping constraints.

### R5 — Rounded, distinguishable input

Give the input visibly rounded rather than sharp rectangular corners and a translucent black background. The input must read as an editable region distinct from the enclosing glass surface, while visually belonging to the same panel. Keep its contents sharp and readable; its fill must not turn into a fully opaque black block.

Use the established reusable input and decoration interfaces. Styling must not introduce another editor or move query limits, search commands or result handling into the editing core. Exact radius and black opacity are not fixed.

Sources: S1; ADR 0065; existing `TextField.decoration()`.

### R6 — Generic capability, application-owned product choices

Provide local backdrop blur as a reusable `harbor-widget` visual/rendering capability usable by a non-terminal application without interpreting Harbor search, session or command types. Its contract must allow a consumer to compose local background filtering with sharp foreground content and the existing decoration/clipping mechanisms.

The application owns the search panel composition, result semantics, glass preset, transparent-button style and input decoration. Existing styling APIs must be reused where sufficient; no search-specific widget API or global control-default change is required for those appearance choices.

The mechanism must compose with embedded terminal drawing without moving terminal content ownership or search behavior into the widget layer. Public API spelling and GPU implementation strategy are not selected.

Sources: ADR 0066, applying ADRs 0064 and 0065.

## Solution Boundary and Seams

Current source anchors are `crates/harbor-app/src/workspace.rs::search_bar`, `crates/harbor-app/src/search.rs`, and the widget `TextField`, `IconButton`, decoration and rendering surfaces. These identify existing responsibilities, not a mandated file layout or implementation sequence.

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Result presentation | Application search state → application search bar | Valid presented match count, optional selected position, and running/refresh/coverage state | The R2 presentation, without introducing automatic navigation or hiding incomplete coverage |
| Search interaction | Reusable input/icon controls → application command boundary | Committed edits, submit/navigation and close intent | Existing search actions and focus behavior; the style change does not replace editing or command routing |
| Glass composition | Application panel composition → widget local-backdrop capability | Consumer-selected region/geometry and visual parameters, with separately composed foreground content | Local filtering of content behind the panel while its foreground stays sharp |
| Embedded rendering | Widget backdrop rendering ↔ embedded terminal drawing | Terminal pixels behind the panel in scene order, applicable clips and changing terminal output | Actual local blur including terminal presentation, without terminal-specific blur semantics or stale captured output |
| Existing styling | Application appearance → `TextField.decoration()` / `IconButton.style()` | Rounded translucent input decoration and transparent state backgrounds | Product-specific appearance without changing generic defaults or editing behavior |

## End-to-End Verification

| Scenario | Given / when | Required observation |
| --- | --- | --- |
| Open and clear | Open search, enter a query, then clear it | Input, result and previous/next/close buttons stay on one row; initial and cleared states show `No Result`. |
| Complete no match | Finish a search with no retained matches | Show `No Result`; no obsolete positive count or selected ordinal remains. |
| Fast search | Commit a query that completes within 200ms | Work starts without debounce; pending new results keep `No Result` unchanged, then the final result appears without flashing `Searching…`. |
| Slow search and cancellation | Leave work pending across 200ms without another key; then finish, clear, close or replace the query | The delayed busy hint requests a frame at the threshold; no old busy hint/timer survives completion or cancellation. Input/result/button geometry remains fixed. |
| Results before navigation | Finish a query with N > 0 without pressing navigation | Show `— / N`; do not automatically select a match or move the terminal viewport. |
| Navigation | Activate next/previous by button or existing keyboard route, including wrapping | Successful navigation displays the correct one-based `i / N`; ordering and wraparound remain unchanged. |
| Incomplete or refreshing search | Observe a bounded/partial search and a refresh while terminal output changes | Running/limited coverage remains identifiable; neither zero nor positive incomplete counts masquerade as a complete result. |
| Invalidated selection | Select a result, then replace/clear the query or invalidate its presented results | The display follows current valid results and does not retain a stale ordinal. |
| Integrated buttons | Rest, hover, press, keyboard-focus and, when applicable, disable the search buttons | No independent background tile appears; icons remain recognizable, focus/interaction feedback is observable, and existing actions still work. |
| Real glass over terminal | Put contrasting terminal text behind the panel, then update that text | Underlying terminal content is visibly softened only in the backdrop region and updates with output. Panel text, caret and icons remain sharp. Translucency or system Acrylic alone is not accepted as proof. |
| Input surface | Focus and edit the input against contrasting terminal content | Rounded corners and a translucent black fill distinguish the editable area without an opaque block; input, selection and caret remain usable. |
| Generic mechanism | Compose the local-backdrop capability in a non-terminal widget example with changing background content and foreground text/icons | The same local-filter/sharp-foreground contract works without application search/session types. |

Use focused application/widget tests for state publication, navigation-driven display, composition, style states and unchanged editing/command behavior. Use visual/runtime evidence for real blur over terminal drawing, live-output updates, rounded clipping, foreground sharpness and input readability. Existing project validation obligations still apply; this spec-only step runs no implementation or runtime acceptance tests. No new numeric performance threshold is established here.

## Decisions and Limitations

- **Real blur rather than visual approximation:** Explicit user choice B; recorded in ADR 0066.
- **No automatic first-match selection or scrolling:** Explicit user choice to preserve current navigation behavior, including the confirmed `— / N` before navigation.
- **One contract:** Application styling and generic local blur share the observable glass-panel outcome; this spec does not split them into independent approval paths.
- **No invented material recipe:** The soft-glass direction is confirmed, but exact visual parameters remain implementation tuning, not established Xiaomi material values or pixel-exact reference requirements.
- **No new rendering architecture:** Blur API, algorithm and render-pass/target arrangement remain open within the approved ownership boundary. No silent translucency-only fallback is approved as fulfillment of R4.
- **Scope:** Search-panel appearance/result presentation and the required reusable local-blur capability only. This does not select a general arbitrary-filter framework, alter window Acrylic policy, change global widget styling, change search matching/coverage policy, or replace the existing input implementation. The selected endpoint excludes tickets and implementation.
