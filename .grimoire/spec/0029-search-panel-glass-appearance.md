# Single-Row Search Panel with Local Glass Backdrop

**Spec ID:** 0029
**Status:** Draft
**Date:** 2026-10-16
**Sources:** The conversation contract below; [ADR 0066](../adr/0066-widget-local-backdrop-blur-with-application-glass-presets.md) and [ADR 0067](../adr/0067-widget-gradient-paints-with-application-material-presets.md), applying [component ownership ADR 0064](../adr/0064-widget-and-application-component-ownership.md) and [layered input ADR 0065](../adr/0065-layered-widget-text-input.md).
**Execution:** Initially spec-only; the user subsequently invoked grimoire-loop and approved the S8 rendering refinement. This contract records requirements, not proof of implementation or acceptance.

## Source Contract

- **S1 — Requested appearance:** Put the input, result information and buttons on one row, with result information before the buttons. Default to the exact text `No Result`; show current-match/total information when matches exist. Buttons blend into the panel without their own background color. Give the overall panel a soft glass appearance, and give the input rounded corners and a translucent black fill that blends in while remaining identifiable as an input.
- **S2 — Confirmed choices:** The user selected B: real local blur of terminal content behind the panel, not merely a translucent-card approximation. Foreground input, result text and buttons remain sharp. The user chose to preserve navigation behavior: do not automatically select or navigate to the first match; show `— / N` before navigation and `i / N` after navigation.
- **S3 — Selected route:** After these choices and the ownership boundary were summarized, the user approved continuing with spec only, without ticket decomposition or implementation.
- **S4 — Existing constraints:** [Search spec 0027, R4–R7](./0027-revisioned-terminal-read-model-and-literal-search.md) requires validated search/navigation and distinguishes partial coverage from complete no-match results. [Input spec 0028](./0028-reusable-single-line-text-input.md) and ADR 0065 keep editing reusable and search-specific semantics application-owned. [Decoration spec 0009](./0009-widget-decoration-and-terminal-chrome.md) supplies existing decoration/clipping constraints; its first-version exclusions of filters and gradients do not themselves supply the new blur or gradient capabilities; this contract and ADR 0067 select the bounded gradient extension.
- **S5 — Confirmed follow-ups:** The user reported status-driven input/result movement and approved immediate search without input debounce and a busy hint only after about 200ms. The later explicit correction replaces the proposed standalone `—` pending label with the unchanged initial `No Result` text during that delay. This is a presentation fallback, not completed negative search evidence. Keep fixed result-slot geometry, do not reuse old-query counts, and retain the existing `— / N` / `i / N` navigation format.
- **S6 — Confirmed glass refinement:** After comparing the current search panel with the supplied glass reference, the user clarified that the corners should not become capsule-shaped: the goal is a more translucent glass material. The user approved preserving existing corners and layout, reusing local backdrop blur, adding a generic solid-color/linear-gradient paint abstraction shared by fills and border strokes in `harbor-widget`, and keeping the material recipe in the application. The approved recipe combines a lighter translucent base, subtle surface reflection, varying edge highlights and a soft outer shadow; the input retains a lighter translucent black surface. No comprehensive `Glass` component, refraction, dynamic lighting, noise or arbitrary-filter framework is selected. This refinement updates the existing spec rather than creating a separate contract or starting implementation.
- **S7 — Folder-glass reference and selected follow-up:** The user supplied `pi-clipboard-99fd2408-af00-49cd-a7a7-0de35812d93e.png` and requested a more transparent glass appearance like the large app folder in the middle of that phone screenshot. The user approved the interpretation: let more background color through while softly blurring its details, keep panel text/caret/buttons sharp, use only fine edge highlights with less heavy outlining/shadow, and lighten the input surface without changing the existing corners, layout or search behavior. Do not copy the reference's green cast as a fixed material color; it primarily reflects the wallpaper. A plain background need not acquire the reference's color variation. The user selected updating this existing spec only, without tickets or code changes.
- **S8 — Glass depth:** After finding the lighter material insufficient, the user requested no shadow, thicker uneven edge highlights and glass-like distortion. The user selected the recommended edge/corner-only refraction, with a stable center and undistorted foreground, emphasizing perceived three-dimensional glass thickness. This supersedes the earlier refraction exclusion and shadow/thin-highlight recipe, not the preserved geometry or search behavior. See [ADR 0068](../adr/0068-local-backdrop-edge-refraction.md).
- **S9 — Finer edge and faint dark tint:** The user found the S8 edge too thick and requested a finer highlight plus a very faint black panel background to help keep text readable. Retain varying highlight brightness, edge refraction, no outer shadow and the existing layout/foreground. This supersedes the S8 highlight thickness and prior white-only surface recipe; use restrained translucent black, not an opaque dark card. Exact width and opacity remain reversible tuning.
- **S10 — More transparent highlights and wider editor:** The user found the highlights too white/opaque and requested more transparency, a wider text input and a smaller result area. Reduce highlight opacity while retaining brightness variation, the faint black base, refraction and no shadow. Reallocate width from the fixed result slot to the editor without status-dependent movement; preserve the overall panel, button geometry and search semantics. This updates the earlier internal-layout preservation constraint only for the input/result width split.

## Requirements

### R1 — One-row composition

Order the search panel content as input → result information → previous-match button → next-match button → close button. Result information must no longer occupy a second row. Input, result information and buttons must be vertically aligned as one coherent search bar.
Changing status wording or count digit length must not resize the input or shift the result slot/buttons horizontally within the same panel allocation. Source: S5.
Under S10, give the editor more width by reducing the reserved result slot. Keep that smaller slot fixed across statuses; retain existing ellipsis for overflow and the visible partial/limited qualification. Panel width and button geometry remain unchanged.

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

Preserve the current panel and input corner radii and the existing panel layout except for the S10 input/result width redistribution; do not introduce a capsule silhouette as part of the material change. The refinement baseline in `workspace.rs::search_bar` is a 12-logical-pixel panel radius and a 7-logical-pixel input radius. Search interaction and result publication remain governed by R1–R3.

Replace the heavy uniform-gray-card appearance with a lighter translucent base retaining background colors and subtle surface reflection. Under S8/S9, remove the outer shadow, use a finer, more transparent directionally varying edge highlight under S10 and a very faint translucent black surface to support text readability, not an equally bright white outline. Add actual background refraction near edges and corners, fading to no displacement toward the center; foreground text/caret/icons remain sharp and undistorted. These are material changes, not merely stronger blur. Numeric tint, blur, refraction-band/strength and highlight values remain application tuning.

Use the folder surface identified in S7 as a qualitative translucency reference: broad background color/shape remains perceptible while fine detail is softened. Do not cover it with a milky gray base, fixed green overlay or wallpaper-like patches. S8 supersedes the earlier thin-edge/soft-shadow recipe with a nonuniform highlight (refined to a finer edge under S9), no outer shadow and edge-only refraction. Keep the input lighter but distinguishable; do not copy the folder's dimensions or corner shape.

Sources: S1, S2, S6, S7, S8, S9, S10; ADRs 0066–0068; S4 decoration/clipping constraints.

### R5 — Rounded, distinguishable input

Give the input visibly rounded rather than sharp rectangular corners and a translucent black background. The input must read as an editable region distinct from the enclosing glass surface, while visually belonging to the same panel. Keep its contents sharp and readable; its fill must not turn into a fully opaque black block.

Use the established reusable input and decoration interfaces. Styling must not introduce another editor or move query limits, search commands or result handling into the editing core. Preserve the existing input radius under S6 and lighten the translucent black fill so it reads as an inset editable region within the same glass material, not a separate heavy block. Exact black opacity is not fixed.

Sources: S1, S6, S7; ADR 0065; existing `TextField.decoration()`.

### R6 — Generic capability, application-owned product choices

Provide local backdrop blur as a reusable `harbor-widget` visual/rendering capability usable by a non-terminal application without interpreting Harbor search, session or command types. Its contract must allow a consumer to compose local background filtering with sharp foreground content and the existing decoration/clipping mechanisms.

The application owns the search panel composition, result semantics, glass preset, transparent-button style and input decoration. Existing styling APIs must be reused where sufficient; no search-specific widget API or global control-default change is required for those appearance choices.

The mechanism must compose with embedded terminal drawing without moving terminal content ownership or search behavior into the widget layer. Reuse the existing `BackdropFilter`, decoration/clipping capabilities, extending local sampling with optional edge refraction rather than creating parallel glass-specific mechanisms. Public API spelling and GPU implementation strategy are not selected.

Sources: S6, S8; ADRs 0066–0068, applying ADRs 0064 and 0065.

### R7 — Shared generic gradient painting

Extend widget decoration painting with one reusable paint abstraction supporting solid colors and linear gradients, usable for both box fills and border strokes. Gradient inputs describe direction and color stops including alpha, not search-specific or glass-specific properties. A non-terminal consumer must be able to use the same capability without application search/session types.

The widget layer owns gradient rendering, rounded-edge antialiasing and transparent composition. Preserve existing solid-color styling interfaces and behavior, optional/no-fill decoration, layout-neutral decoration and the established clipping and paint-order contracts. Ordinary solid-color consumers must not require migration because gradients become available. This does not request per-edge border widths, an inset-shadow API or new layout behavior.

The application composes these paints with local background filtering and optional edge refraction for the R4 material. Its preset owns color, opacity, gradient/highlight direction, blur and refraction parameters. Do not introduce search-specific rendering APIs, change global control defaults or introduce a comprehensive `Glass` component. Public paint type/API names, gradient coordinate/stop rules and GPU representation remain implementation design details rather than selected requirements.

Sources: S6, S8; ADRs 0067 and 0068; S4 decoration constraints.

## Solution Boundary and Seams

Current source anchors are `crates/harbor-app/src/workspace.rs::search_bar`, `crates/harbor-app/src/search.rs`, and the widget `TextField`, `IconButton`, decoration and rendering surfaces. These identify existing responsibilities, not a mandated file layout or implementation sequence.

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Result presentation | Application search state → application search bar | Valid presented match count, optional selected position, and running/refresh/coverage state | The R2 presentation, without introducing automatic navigation or hiding incomplete coverage |
| Search interaction | Reusable input/icon controls → application command boundary | Committed edits, submit/navigation and close intent | Existing search actions and focus behavior; the style change does not replace editing or command routing |
| Glass composition | Application panel composition → widget local-backdrop capability | Consumer-selected region/geometry and visual parameters, with separately composed foreground content | Local filtering of content behind the panel while its foreground stays sharp |
| Embedded rendering | Widget backdrop rendering ↔ embedded terminal drawing | Terminal pixels behind the panel in scene order, applicable clips and changing terminal output | Actual local blur including terminal presentation, without terminal-specific blur semantics or stale captured output |
| Existing styling | Application appearance → `TextField.decoration()` / `IconButton.style()` | Rounded translucent input decoration and transparent state backgrounds | Product-specific appearance without changing generic defaults or editing behavior |
| Decoration paints | Application material preset → widget decoration/renderer | Consumer-selected solid or linear-gradient fill/stroke paint, gradient direction and color stops including alpha, existing geometry/clips | Shared generic fill/stroke painting with correct translucent rounded edges, preserving solid-color interfaces and layout behavior |
| Edge refraction | App material parameters → widget backdrop compositor | Rounded bounds, bounded edge-band/strength, live background pixels and unchanged foreground | Background bends/compresses near edges/corners only; center, foreground and hit testing remain stable, with clipping/alpha/DPI preserved and blur-only callers unchanged |

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
| Translucent material | Observe the panel over contrasting terminal text and color regions, including after background changes | Background colors remain perceptible through the softened material; subtle surface reflection and varying edge highlights replace the heavy uniform-gray-card appearance. Foreground stays sharp, and finer uneven highlights and a faint translucent black veil separate the panel without an outer shadow or opaque backing. A uniform background is not expected to acquire reference-image colors. |
| Folder-reference comparison | Show the panel over contrasting broad color regions and fine terminal text; compare with the S7 folder material, then change the background colors and output | More of the background's broad color/shape variation comes through instead of being masked by a uniform gray base; fine terminal detail stays softly blurred while panel text/caret/icons remain sharp. Finer highlights vary in brightness and no outer shadow is drawn, and the lighter input remains identifiable. The material follows changing background colors rather than keeping a fixed green cast. |
| Plain-background limit | Show the same panel over a uniform neutral terminal background | The panel remains lightly translucent, readable and separated by varying edge highlights without an outer shadow; no artificial wallpaper-like color patches or fixed green tint are added to imitate the reference. |
| Glass depth | Observe contrasting lines/color boundaries through straight edges and rounded corners; update background while search remains open | Visible local bending/compression follows the edges and updates live, fades to a stable center, and never distorts foreground. Finer highlights have bright and dim portions; no outer shadow is emitted. |
| Refraction isolation and compatibility | Compare refraction enabled/disabled at fractional DPI, resized/small shapes, rounded ancestor clips and translucent backgrounds | Only the selected background band changes; center/outside/foreground remain unchanged relative to the blur-only baseline. No out-of-bounds sampling, extra opacity or external-draw replay; existing blur-only consumers retain behavior. |
| Preserved geometry | Compare panel/input outlines and allocations before and after the material refinement | Panel and input retain their existing radii and layout except for S10: a wider editor and smaller fixed result slot; neither becomes capsule-shaped and no material decoration shifts the controls. |
| Shared gradient paints | Use the same generic paint capability for a non-terminal box fill and border, then change direction/color/alpha inputs | Both uses reflect the selected gradient, with correct translucent rounded edges and applicable clipping; no search types or dedicated glass renderer are needed. |
| Solid-color compatibility | Render existing solid-color and no-fill decorations beside gradient decorations | Existing callers keep their interfaces and behavior, including fill absence, geometry, clipping, shadow order and layout neutrality. |

Use focused application/widget tests for state publication, navigation-driven display, composition, style states and unchanged editing/command behavior. Use visual/runtime evidence for real blur over terminal drawing, live-output updates, rounded clipping, foreground sharpness and input readability. Existing project validation obligations still apply; spec publication alone is not evidence of runtime acceptance. No new numeric performance threshold is established here.

## Decisions and Limitations

- **Real blur rather than visual approximation:** Explicit user choice B; recorded in ADR 0066.
- **No automatic first-match selection or scrolling:** Explicit user choice to preserve current navigation behavior, including the confirmed `— / N` before navigation.
- **One contract:** Application glass refinement and its generic painting capability share the observable glass-panel outcome; update this spec rather than split them into independent approval paths. Source: S6.
- **Preserved geometry:** S6 explicitly rejects capsule reshaping; preserve the current panel and input corners and layout except for the S10 input/result width redistribution.
- **No invented material recipe:** Exact opacity, color, gradient and refraction values remain implementation tuning, not established reference-image material values or pixel-exact reproduction requirements. The existing corner radii are preserved rather than tuned.
- **Reference identity and portability:** S7 refers to the large rounded app-folder surface, not the phone's opaque clock/weather widgets. The supplied attachment was read from `C:\Users\DEREKW~1\AppData\Local\Temp\pi-clipboard-99fd2408-af00-49cd-a7a7-0de35812d93e.png`. This is a temporary conversation source, not an archived repository asset; the written S7/R4 requirements and verification cases retain the approved target if the attachment expires. No material parameter can be measured reliably from this reference alone, and no pixel-exact copy is required.
- **Selected abstraction, not a fixed GPU architecture:** ADR 0067 selects shared generic solid/linear-gradient paints and reuse of existing blur/decoration mechanisms. API names, gradient coordinate/stop rules and render-target/pass representation remain open. No silent translucency-only fallback is approved as fulfillment of R4.
- **Scope:** Search appearance/result presentation, reusable local blur, generic gradient fill/stroke and optional bounded edge refraction. Whole-panel warping, dynamic lighting, noise, a comprehensive `Glass` component, arbitrary filters, changed window Acrylic policy/global widget defaults/search semantics and a replacement input implementation remain excluded. S8 supersedes S6/S7 only for shadow removal, thicker highlights and edge refraction.
