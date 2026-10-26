# Single-Row Search Panel with Local Glass Backdrop

**Spec ID:** 0029
**Status:** Accepted (owner-confirmed)
**Sources:** Approved search/material refinements; [ADR 0066](../adr/0066-widget-local-backdrop-blur-with-application-glass-presets.md), [0067](../adr/0067-widget-gradient-paints-with-application-material-presets.md), [0068](../adr/0068-local-backdrop-edge-refraction.md), [ownership 0064](../adr/0064-widget-and-application-component-ownership.md), [input 0065](../adr/0065-layered-widget-text-input.md).
**Material extension:** [Spec 0032](./0032-native-liquid-glass-material.md) selects a native liquid-glass material upgrade shared with the command palette, preserving this spec's search behavior, geometry and appearance invariants.

## Source Contract
Stable refinement IDs preserve meaning without execution chronology: S1 one row, exact No Result, integrated buttons/rounded translucent input; S2 actual local blur (option B), sharp foreground/no automatic navigation; S3 reserved historical refinement alias; S4 existing [search validity/coverage](./0027-revisioned-terminal-read-model-and-literal-search.md), [input](./0028-reusable-single-line-text-input.md) and [decoration](./0009-widget-decoration-and-terminal-chrome.md) contracts; S5 fixed result geometry/immediate search/delayed busy hint with unchanged initial No Result; S6 reusable fill/stroke gradients/app material, preserved non-capsule corners; S7 more transmitted background color/detail-softening, not fixed green/wallpaper imitation; S8 no shadow/edge-only refraction/stable center/foreground; S9 finer uneven highlight/faint black veil; S10 more transparent highlights and wider editor/smaller fixed result slot.
S8 supersedes earlier shadow/refraction exclusion; S9/S10 supersede thick/opaque highlights and white-only base. Only S10 changes input/result width split, not panel/button geometry/search semantics. The temporary S7 attachment pi-clipboard-99fd2408-af00-49cd-a7a7-0de35812d93e.png showed the central large app-folder material, not opaque phone widgets; written requirements survive its expiry. No pixel-exact reproduction or measurable physical material values.

## Requirements
### R1 — One-row composition
Order input → result → previous → next → close, vertically aligned on one row, no second-row result. Status/count digit changes cannot resize input or move result/buttons within the same allocation. S10 reallocates width from result to editor; smaller reserved result slot remains fixed with ellipsis and visible partial/limited qualification. Overall panel width/button geometry unchanged. Preserve overlay role/commands, never terminal source text.

### R2 — Result presentation without automatic navigation
| State | Presentation |
| --- | --- |
| Open/empty, or complete zero-match | Exact No Result |
| Valid N>0, no selected match | — / N |
| Successfully selected match | i / N, one-based current index |
| New query pending before 200ms without valid results | Unchanged No Result; pending internally, no previous-query count |
| Pending at/after 200ms | Searching… optionally with valid same-query information |
| Refresh/partial/limited | Explicit coverage qualification; busy hint delayed 200ms; incomplete never portrayed complete |
Counts/selection always describe current valid query/session. Search starts immediately on committed edits, no input debounce/minimum result delay. Fast completion shows no busy flash. Pending host turns publish delayed hint without another key; completion/cancel/empty/replacement clears old deadlines.
Receiving results must not select/scroll automatically. Successful previous/next navigation updates ordinal, preserving ordering/wraparound; invalidation clears stale ordinal. No old counts during new-query fallback; fallback label is not complete-negative evidence.

### R3 — Buttons integrated into the panel
Recognizable icon controls with transparent backgrounds in all rest/hover/pressed/disabled states, no independent tile. Retain visible keyboard-focus/interaction feedback via foreground/focus treatment. Apply app search style, not global IconButton default changes.

### R4 — Real local soft-glass backdrop
Blur actual behind-panel content including embedded terminal pixels, not mere translucency/shadow blur/window Acrylic. Foreground input/result/caret/icons stay sharp and undistorted; live background changes update, never frozen capture. Filter only intended rounded region under applicable clips, not whole terminal/unrelated foreground.
Preserve 12-logical-pixel panel and 7-pixel input radii/non-capsule silhouette and layout except S10 split. Material transmits broad background colors/shapes while softening fine terminal detail. Use subtle surface reflection, fine more-transparent directionally varying bright/dim edge highlights, very faint neutral translucent black veil and actual live edge/corner bending/compression fading to zero at stable center. No outer shadow, heavy milky gray card, equally bright white outline, opaque dark backing, fixed green overlay or invented wallpaper patches; uniform backgrounds need no reference-like color variation.
Input stays lighter/distinguishable; don't copy reference dimensions/corner shape. Tint/opacity/blur/refraction band/strength/highlight numbers remain reversible app tuning, not fixed thresholds/physical optics.

### R5 — Rounded, distinguishable input
Use existing reusable field/decoration: rounded translucent black inset surface, light enough to blend with glass while identifiable/readable, never opaque block. Preserve input radius/sharp text/selection/caret. No second editor or transfer of query limits/commands/result handling into editing core.

### R6 — Generic capability, application-owned product choices
Widget-owned local backdrop capability works in non-terminal roots without search/session/command types. Compose consumer-selected geometry/filter/clipping with sharp foreground and embedded terminal drawing, without moving terminal content/search ownership into widgets. Reuse BackdropFilter/decoration; extend optional edge refraction, no parallel glass mechanism. App owns composition/result semantics/material/transparent buttons/input style and parameter choices; reuse existing styling APIs where sufficient.

### R7 — Shared generic gradient painting
One reusable solid/linear-gradient paint for both box fills and border strokes; direction/color stops including alpha, no search/glass-specific inputs. Widget owns rounded-edge antialiasing/transparent composition. Existing solid APIs/callers/no-fill, layout-neutral decoration, clipping and shadow/background/child/border order remain compatible; ordinary callers need no migration.
App composes paints/filter/refraction and chooses color/opacity/direction/strength. No per-edge widths/inset-shadow API/new layout, comprehensive Glass component, dedicated search renderer or changed global defaults. Public names/gradient coordinate-stop rules/GPU strategy remain implementation choices.

## End-to-End Verification
| Scenario | Required observation |
| --- | --- |
| Open/clear/complete no match | One row/exact No Result, no stale positive count/ordinal. |
| Fast search | Immediate work, unchanged pending label then result under 200ms, no busy flash. |
| Slow/cancel | Busy at threshold without another key/frame trigger; finish/clear/close/replace leaves no old timer; fixed geometry. |
| Results/navigation | — / N without selection/viewport movement; button/key previous-next including wrap gives correct i / N. |
| Partial/refresh/invalidation | Explicit incomplete qualification, current counts only, stale selected ordinal removed. |
| Buttons | Rest/hover/press/focus/disable stay background-transparent, recognizable and operable with focus feedback. |
| Real terminal glass | Contrasting fine text/live changes softened only behind panel, foreground sharp; no Acrylic/translucency-only substitute. |
| Input | Rounded light translucent black, readable editing/caret/selection, no opaque block. |
| Generic backdrop | Non-terminal changing scene/foreground obey same local-filter/sharp-content contract. |
| Material/reference | Broad color/shape transmits, fine details soften, faint black base/finer varying transparent highlights/no shadow; changed background colors follow naturally, uniform neutral background gains no fixed green/artificial patches. |
| Depth/isolation | Live edges/corners bend background, fade to stable center; compare enabled/disabled at fractional DPI/resizes/small shapes/rounded ancestor clips/translucent scenes: no outside/foreground/hit-test distortion, out-of-bounds sampling, extra opacity or external-draw replay; blur-only compatibility. |
| Geometry | Original panel/input radii/layout preserved except wider editor/smaller fixed result allocation; no material-induced control shift/capsule. |
| Paint compatibility | Non-terminal gradient fill and border respond to direction/color/alpha with correct rounded clipping; solid/no-fill interfaces/geometry/shadow order/layout unchanged. |

## Evidence and Limits
[Application composition](../../crates/harbor-app/src/workspace.rs), [material preset](../../crates/harbor-app/src/appearance.rs), [search status](../../crates/harbor-app/src/search.rs) and [generic filter](../../crates/harbor-widget/src/widgets/backdrop_filter.rs) contain implementation; source/test existence is not a fresh visual pass. Focused app/widget tests must cover state/deadlines/navigation/composition/style/unchanged editing/commands, generic paints and bounded sampling.
Under validation policy, collect actual blur over terminal/live output, clipping/alpha/foreground/input readability, reference-relative translucency and edge depth/resize/DPI evidence. Follow existing performance obligations; no new numeric threshold. Record executed versus NOT RUN honestly; native material acceptance is not established by spec maintenance/GPU tests alone.
No whole-panel warping, dynamic lighting, noise, arbitrary-filter framework, changed whole-window Acrylic/global control/search policy, replacement editor or pixel-exact reference copy. Existing S4 constraints remain except explicitly extended local blur/gradients/refraction.
