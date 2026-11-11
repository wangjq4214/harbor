# Harbor Design System and Liquid-Glass Quick Save

**Related spec:** [0054 — requirements entry](../../.grimoire/spec/0054-harbor-design-system-and-quick-save.md)
**Status:** Draft — scope and design direction confirmed; not implementation or visual acceptance
**Date:** 2026-11-11
**Sources:** The owner requested a redesigned Save Session page, liquid-glass styling and an Astryx-referenced design system. They accepted reusable application-wide foundations with Quick Save as the first complete example, initially selecting specifications only, then requested moving the design system into `docs/` and separately invoked implementation. In the latest screenshot refinement, they accepted the recommended summary-first review with on-demand correction, retained naming/confirmation stages, and revision of the existing specifications only.
**Source snapshot:** `066741e`, with a clean working tree before the initial specification. Inspection covered specifications, ADRs, Quick Save composition, application appearance recipes and generic control themes. The initial refinement had no fresh screenshot or native application observation.

**Naming/selector refinement source:** In the subsequent refinement conversation, the owner approved matching naming geometry to the command palette input row, maintaining visible gaps between environment segments, neutral liquid-glass hover without an added emphasis color, no separate pressed appearance, and retained lightly accented selection/checkmark and visible keyboard focus. The specification endpoint was updating this body and the existing 0053/0054 entries only; no code, tickets or CONTEXT/ADR writes. The owner then explicitly invoked `grimoire-loop` for scoped implementation and checks; the specification-stage restriction is not a claim that implementation has been visually accepted.

This is the canonical body of the application design system and Quick Save specification. `.grimoire/spec/0054` retains a traceability entry only. Relocation and translation do not change the agreed requirements or imply runtime acceptance.

## 1. Outcomes, Scope and Existing Contracts

### 1.1 Selected outcome

This document combines reusable design foundations and the complete Quick Save application rules. Both share one design direction and acceptance lifecycle rather than requiring independently approved specifications.

- **Visual goal:** A restrained, refined, understandable desktop tool. Liquid glass expresses surfaces and hierarchy; not every control becomes a transparent bubble.
- **System goal:** Consistent semantic colors, typography, spacing, corners, materials, component states, motion boundaries and readability, instead of accumulating isolated page styles.
- **First consumer:** Input-only naming and the confirmation page share one visual family. Confirmation is the main redesign target.
- **Adoption boundary:** Application-wide reuse does not mean redesigning every page or migrating every component in this delivery.
- **Authority:** The summary-first and naming/selector refinements each selected specification work only. Subsequent explicit `grimoire-loop` invocations separately authorized their scoped implementation and verification, including the naming/selector correction now recorded below. This authorizes related code, tests and implementation documentation, not CONTEXT/ADR writes, tickets, implementation-plan artifacts, commits, unrelated changes or unsupported runtime-acceptance claims.

### 1.2 Relationship to existing specifications

| Source | Retained contract | Addition here |
| --- | --- | --- |
| [0053 — Quick Save UI](../../.grimoire/spec/0053-quick-save-review-ui-and-reusable-controls.md) | Two-stage flow, meaningful review, glass transparency, component/business boundaries, selectable long values and safety | Shared visual roles and the jointly revised summary-first presentation, on-demand correction and local name-edit action |
| [0052 — Session presets](../../.grimoire/spec/0052-named-session-profiles-and-quick-save.md) | Capture, correction, agents, environment acknowledgements, final authorization, conflict-safe writes and relaunch | Clear presentation of those states, not changed business behavior |
| [0032 — Native liquid glass](../../.grimoire/spec/0032-native-liquid-glass-material.md) | Native material, background sampling, sharp foreground, clipping, alpha composition and existing-consumer compatibility | Surface/control hierarchy using the selected material family, not a different optical algorithm |

0053's exclusion of a complete design system describes its historical delivery scope, not a permanent prohibition. The owner-authorized 0054 extension fills that scope. The latest approved refinement updates both documents' presentation clauses: always-visible correction fields and the three-action footer are replaced by summary-first review, on-demand correction, local name editing and a two-action footer. Search, palette and notification geometry/behavior remain unchanged. No ADR is superseded.

## 2. Astryx Reference and Native Adaptation

References: [Astryx website](https://astryx.atmeta.com/), [Tokens](https://astryx.atmeta.com/docs/tokens), [Theming](https://astryx.atmeta.com/docs/theme), [How Astryx works](https://astryx.atmeta.com/blog/how-astryx-works) and [official repository](https://github.com/meta/astryx). Public documentation research established the organizational reference; homepage component counts, unpinned numerical values and web screenshots are not Harbor acceptance baselines.

| Borrowed principle | Harbor contract | No implicit commitment |
| --- | --- | --- |
| Purpose-based color/style roles | Distinguish primary/supporting text, surfaces, emphasis, feedback and interaction; use equivalent roles consistently | No copied brand palette, complete CSS variable scheme or theme catalog |
| Separation of themes and component appearance | Generic controls consume styles; application recipes define Harbor's identity | No React, StyleX, WebView or Astryx runtime dependency |
| Composable components with consistent rules | Related inputs, choices, buttons and feedback share rules; pages own business composition | No port of the entire Astryx component catalog |
| Complete interaction states | Cover keyboard focus, hover, press, selection, disabled and asynchronous feedback | Do not directly transplant web ARIA/API or loading behavior into native save admission |

Astryx guides design-system organization; Harbor retains its already-selected native liquid-glass path. This document copies no upstream code, font or asset, and claims no equivalent accessibility implementation or certification. External documentation evolves: actual future code/asset reuse requires checking its specific version and license.

### Dialog and form references for the summary-first refinement

- [Fluent 2 Dialog](https://fluent2.microsoft.design/components/web/react/core/dialog/usage): focused tasks, nonredundant actions and a visible header/footer around a scrolling body support Q1/Q4.
- [Carbon Forms](https://www.carbondesignsystem.com/building-blocks/core/patterns/forms): short dialog forms and conditionally relevant fields support Q2/Q3. Its guidance against accordion/tab-based dialog forms is not permission to hide required choices or warnings in "Advanced".
- Astryx remains the reference for shared tokens, semantic roles and composable styling. No web runtime, copied footer geometry, new dismissal behavior or library adoption follows from these references.

These sources inform the owner-approved Harbor adaptation; they do not themselves establish Harbor domain facts or override its explicit correction, safety and popup-lifetime contracts.

## 3. Design Foundations

Role names below are specification vocabulary, not mandated Rust types or public API spellings. Numeric values, fonts, hues and identifiers remain reversible tuning within the selected direction, not unverified pixel values presented as approved standards.

### D1 — Semantic colors and surfaces

| Role | Use and constraint |
| --- | --- |
| Primary foreground | Titles, important values and main content; sharp and unaffected by background refraction |
| Secondary foreground | Supporting explanations and provenance; subordinate but readable |
| Label foreground | Persistent field meaning; placeholders cannot replace labels |
| Primary action / on-primary | Main task submission and its legible foreground; distinct from ordinary selection |
| Selection | Caller-owned single/boolean choice; hover must not imply selection |
| Focus indicator | Keyboard location, distinct from emphasis, pressing and selection |
| Error / warning / result | Blockers, risk and actual outcome; text conveys meaning rather than color alone |
| Panel surface | Shared live liquid-glass recipe containing the task, not a fixed background image |
| Input surface | A recognizable inset translucent editing area, distinct from its panel |
| Divider / boundary | Major grouping or control boundaries; not an equally strong outline everywhere |
| Hover / pressed / disabled | Consistent feedback for equivalent roles; disabled remains readable and never implies success |

Equivalent meanings must not change color arbitrarily between sections. Explicit consumer overrides remain possible without duplicating independent material implementations.

Terminal themes remain separate from application appearance. Changing terminal colors must not map that theme directly to form, button or rail semantic colors; natural transmission through glass is not theme coupling. [ADR 0058](../../.grimoire/adr/0058-terminal-themes-separate-from-application-appearance.md) establishes the day/night appearance boundary but does not choose defaults, system following, switching UI or persistence. This specification adds none of those policies and does not turn a dark example into a global mode decision.

### D2 — Typography and information density

Use reusable roles rather than arbitrary per-location scaling:

| Role | Relative hierarchy and use |
| --- | --- |
| Page title | Most prominent; identifies the task |
| Section heading | Visibly larger than labels/body; establishes a scanning path |
| Field label | Persistent, clear meaning of an input or value |
| Body / field value | Task information; compactness cannot sacrifice reading or editing |
| Supporting text | Subordinate but readable; necessary warnings/errors are not disposable fine print |

Uppercase or color changes alone do not establish heading hierarchy. Use UI text measurements and layout, not terminal-cell estimates for wrapping. Terminal font settings must not drive application-form typography.

Explanations, validation and errors wrap within available width. Long paths and non-secret arguments remain fully inspectable, selectable and copyable. Do not replace read-only editors with non-selectable labels or write visual line breaks into source values.

### D3 — Spacing, corners and boundaries

- Define consistent spacing roles for labels/fields, field groups, sections, panel insets and actions. **Inter-section whitespace exceeds spacing between related elements within a section.**
- Equivalent controls share height, padding and corner rules, with intentional local overrides rather than independent trial values everywhere.
- Outer panels and inset controls have separate corner roles. They need not all have the same radius or become capsules.
- Use restrained dividers where needed for major separation and final actions; grouping does not require a line at every section boundary. Prefer typography and related spacing, remove empty sections, and do not surround groups with equally prominent glass cards.
- Panels size compactly to content within the window. Large forced blank regions are not refinement; shrinking labels, targets or warnings into unusability is not compactness.
- Exact dimensions are not new fixed thresholds. Role-based descriptions do not override existing consumer-specific dimensions.

### D4 — Liquid-glass hierarchy

| Layer | Visual requirement |
| --- | --- |
| Outer task panel | Shared backdrop softening, edge-local refraction and restrained bright/dim rim response; transmit live broad colors while keeping the center stable |
| Inputs and interaction regions | Recognizable translucent surfaces and state feedback; subordinate to the panel, without requiring strong refraction on every field |
| Content groups | Primarily headings, spacing and dividers, not nested equally prominent glass shells |
| Foreground | Sharp titles, text, icons, caret and selection; optics must not alter hit regions |

Do not imitate glass with near-opaque fills, a dark full-window scrim, a thick white outline or fixed gradient wallpaper. Increasing blur/transparency alone does not deliver this design system or redesign. Material and foreground hierarchy must work together over quiet and busy terminal content.

Retain [ADR 0070](../../.grimoire/adr/0070-shared-application-glass-presets-with-local-overrides.md)'s shared recipes/local overrides and [ADR 0076](../../.grimoire/adr/0076-native-liquid-glass-material-port.md)'s native in-window sampling: no other-application capture, page-specific parallel renderer or new TOML appearance settings.

### D5 — Components and state rules

| Component / pattern | Appearance and content contract | State contract |
| --- | --- | --- |
| Primary button | Solid primary surface establishes priority; complete, readable label | Normal, hover, pressed, focus, disabled; truthful pending state for asynchronous submission |
| Secondary / local action | Lower visual weight near affected content; name/location editing and disclosure are quiet text actions, explicit correction confirmation is secondary | Clear interaction/focus feedback despite subordinate emphasis |
| Text field | Persistent label, recognizable surface, nearby help/error; placeholder is an example | Editing, focus, error and disabled meanings remain clear; preserve IME and selection |
| Segmented single choice | Explicit current selection, not a character-marked button string | Selected is not hovered; retain mouse, keyboard, disabled and focus contracts. Quick Save's environment segments use Q3's neutral hover and no separate pressed appearance |
| Checkbox | Distinct checked/unchecked artwork and associated text | Render caller state, not a competing copy; disabled cannot change it |
| Disclosure | Clear expanded/collapsed affordance within the reading hierarchy | No draft loss or confirmation changes; necessary warnings remain outside collapsed content |
| Field help / error | Adjacent to the relevant field or acknowledgement; complete readable text | Saving cannot be blocked only by a grey button; state changes must not clip feedback |
| Save feedback | Failure near final actions; truthful status | Pending is not success; failure retains the draft and retry path |

Keyboard focus must be visible without requiring pointer hover. Selection, warnings, errors and disabled meaning cannot rely only on hue; use control shape, markers or text. Ordinary buttons retain the existing arrow cursor, editors the I-beam; web inspiration is not permission to apply hand cursors everywhere.

**Motion boundary:** Retain and unify existing interaction feedback. No new liquid fusion, stretching, bouncing, pointer lighting or continuous material animation is required. Response/state comprehension must not depend on finishing a decorative animation. New timing curves, durations and system motion-preference integration are not selected here.

## 4. Quick Save Page Rules

### Q1 — Two-stage task, not one-step submission

Naming retains its compact input-led entry and an adjacent, Tab-focusable checkmark using the search control's transparent idle and hover/press glass treatment. Opening or editing the field must not add inline hints/errors or resize the panel. Explicit invalid submissions use the existing floating notifications without taking focus; editing starts a new validation episode. Enter/checkmark opens review without writing a file. Do not merge naming and final confirmation or add wizard stages.

Naming matches the command palette's width, horizontal placement/margins and content insets under the same window and DPI conditions. Its height matches the palette's input row only, not the full panel with the command list. Keep the adjacent advance checkmark within this compact naming row; do not add list-height space or change the command palette to achieve the match.

Confirmation remains in the existing window, not a separate native window, fullscreen settings page or general preset editor. Preserve explicit final Save and Cancel. Move the existing return-to-naming action next to the displayed name and label its purpose, for example "Edit name"; it retains the draft and is not an inline rename or another footer action. This satisfies spec 0052's Return to edit capability without retaining the ambiguous old placement/caption.

### Q2 — Summary first, correction when needed

For a recognized, usable context, display one meaningful summary of the recipe that would be saved, with a local Edit action. Do not initially show the environment selector, directory editor, correction help/button or a duplicate "Approved relaunch context" block.

The following is a short local-preset information schematic, not a pixel mockup, fixed text or localization requirement:

```text
Save preset
typst                                      Edit name

Launch location                            Edit
Local · PowerShell
D:\Code\typst

Launch contents
Project shell only
No environment overrides will be saved

> Source and restoration details

                               Cancel   Save preset
```

- Preserve the reading order: title/name, launch location, applicable agent and environment choices, supporting details, final actions.
- A missing agent does not reserve an Agent section; use a concise shell-only explanation. With no configured overrides available, use a concise exclusion statement rather than a full empty Environment variables section.
- When an agent or configured overrides are available, keep applicable inclusion choices, review values, required acknowledgements and safety warnings in the main review, adjacent to their content. Do not collapse actionable choices into a generic Advanced section or treat zero currently selected overrides as proof that no choices exist.
- Supporting provenance starts as one compact disclosure row, not a heading plus a large detached button/blank area. Expanded content retains source program, unreplayed startup arguments and restoration limitations; required warnings never move inside this disclosure.
- Summaries retain complete inspectable, selectable and copyable non-secret values. State what will launch rather than exposing internal approval terminology; source provenance must not imply that source arguments will be replayed.

### Q3 — Explicit correction without duplicate competing values

- Unknown or unusable recognition opens the correction fields directly, with a visible reason; do not present an approved-looking summary or require discovering an Edit link to unblock saving.
- For a usable summary, its Edit action reveals correction fields in that location within the review, not a nested dialog or new wizard step.
- Correction uses the existing Local pwsh / WSL bash / Direct SSH bash single-choice control, persistent directory label and environment-specific help. Local has no destination field, WSL identifies a distribution, and SSH identifies a destination. Preserve the existing path interpretation and environment/agent consequences.
- **Environment-segment appearance:** Keep a clear visible gap between adjacent segment surfaces in every state, including one selected segment beside a different hovered segment; the blocks must not touch or visually merge. Hover uses lightweight neutral native liquid glass with live background transmission, no added emphasis-color tint and no opaque grey fill. The selected segment retains its light emphasis tint and selection marker, so hovering another segment never looks like selecting it. Pressing adds no separate darkening, color change or inset/depressed effect: retain the applicable hover or selected appearance until successful activation updates the caller-owned selection. Keep keyboard focus visible independently of hover, preserve disabled behavior and do not change mouse/keyboard activation semantics. Gap size and optical values remain reversible tuning, not new numerical thresholds.
- Describe the secondary correction action by its result, for example "Confirm launch location", rather than "Apply context correction". Typing/selecting is not approval. Unknown or changed unconfirmed values visibly block Save with a nearby explanation.
- While correcting, do not show an old approved summary as a second apparently current recipe. Invalid confirmation retains the fields and adjacent errors. Successful explicit confirmation returns that area to the updated summary; it does not save the preset, label correction as automatic recognition, or satisfy any independent safety acknowledgement.
- The review is a presentation of the existing stable draft, correction validation and save-admission rules, not a second recipe owner. Name editing, disclosure and rendering transitions do not discard draft changes or confer authorization.
- Agent inclusion, argument acknowledgement and configured-override opt-in retain their independent meanings and defaults. Suspected credentials remain subject to visible handling without disclosure or persistence; final Save cannot bypass an unmet gate.

### Q4 — Visual hierarchy, scrolling and final actions

- Keep the review title/name area and final action area visible while the body scrolls. Long content must not scroll the task identity or Save out of view in place of body scrolling.
- Short reviews size to content without forced empty sections or large blank regions. Narrow layouts may wrap labels/actions; header/footer layout must not cover or make body fields, warnings or keyboard targets unreachable in constrained windows.
- The footer contains Cancel then Save, right-aligned when space permits. Save is the sole solid primary action. Name editing stays beside the name; location editing stays beside the location; correction confirmation stays beside its fields.
- Use quiet text actions for editing/disclosure and subordinate control surfaces for fields. Reduce competing outlines and redundant dividers rather than adding more glass cards or merely increasing heading sizes and spacing. Retain discernible inputs, explicit selection and keyboard focus.
- Expand details, correct context, validate and resize without clipping leading text, hiding mandatory warnings or altering optical hit regions. Non-secret paths and arguments remain fully inspectable/copyable.
- Blockers have nearby reasons, and write failures remain visible near the footer. Pending says saving and retains existing interaction admission, not imported web loading/focus policy.
- Return to naming preserves the draft; Cancel writes nothing; failure retains the valid retry path. No presentation transition authorizes additional writes or silently resets independent confirmations.

## 5. Solution and Integration Boundaries

Use **shared design roles -> application appearance recipes -> generic component appearance/state -> page business composition**. This describes responsibilities and exchanges, not new crates, theme buses, component directories or implementation steps.

| Seam | Connects | Input | Guarantee |
| --- | --- | --- | --- |
| Roles/recipes | Design contract -> application appearance | Semantic roles, shared material, intentional local differences | Consistent product identity; unspecified material fields inherit shared defaults |
| Generic components | Application styles/current values -> widget | Styles, labels, selected/checked, enabled, callbacks | Reusable rendering/interaction; callbacks express intent without duplicate business state |
| Material rendering | Application glass recipe -> widget backdrop | Painted in-window content, geometry, DPI, clips, bounded parameters | Background optics do not alter foreground, hits or terminal state |
| Page composition | Draft/validation -> Quick Save | Name, usable/unknown context, pending corrections, agent/overrides, independent approvals and write result | Summary or correction presentation of the same stable draft; visible blockers, adjacent feedback and one final save entry |
| Editing/lifecycle | Existing editors/focus/exclusive popup -> controls | Current valid events and flow lifetime | IME, selection/copy, return/focus restoration; stale events cannot submit a replacement flow |
| Persistence | Explicit final approval -> existing save path | Validated current draft and established authorization | Conflict-safe write, truthful outcome or retained-draft failure |

[ADR 0064](../../.grimoire/adr/0064-widget-and-application-component-ownership.md)'s mechanism/business boundary remains. Generic controls cannot depend on `ProjectRecipe`, `WorkspaceUiController` or save commands. Business layout and product recipes stay application-owned; this is not a second input, state or popup framework.

Implementation entry points are [application design roles](../../crates/harbor-app/src/appearance/design_system.rs), [Quick Save composition](../../crates/harbor-app/src/quick_save.rs) and [project review](../../crates/harbor-app/src/quick_save_project.rs). Quick Save now presents a usable location summary or explicit correction fields from the same draft, local name editing, a compact source-details disclosure, and fixed title/footer around a shrink-wrapped scrolling body. Long write errors live at the body end beside the footer and are revealed on failure, rather than consuming the fixed-control budget. Glass remains `GlassAppearance::command_palette()`; generic controls do not own recipe semantics.

Quick Save injects the application recipe through widget `ThemeProvider` before building its content. Accent colors and the optional `ControlStyle.selected_backdrop` token come from the inherited `Theme.button`. Selected environment segments reuse `GlassAppearance::command_palette_row`'s native backdrop sampling, blur/frost, edge refraction and highlight/shade rim, with only a light blue overlay (alpha 0.10) before sharp text/checkmark. The selection role supplies the same optics through opt-in `hovered_backdrop` for unselected hover, with a neutral white overlay (alpha 0.06) and no outline. It disables `pressed_feedback`, retaining the applicable hover/selection/idle appearance without changing interaction state or activation. The application supplies a 6 logical px `SegmentedControl` gap. Generic defaults stay gap-free, flat on hover and press-responsive; other roles do not acquire these backdrop overrides. Selection switches immediately with no animation, labels stay centered and final Save keeps its solid primary emphasis.

The body uses the existing ScrollView and a loose Flexible child inside an opt-in shrink-wrapped Column. Flex/Column's new shrink-wrap option changes only the final content extent, not share allocation or existing defaults; short bodies remain compact and long bodies use the space between header/footer. This is generic layout behavior, not a dialog manager or save-specific widget.

### First-consumer tuning baseline

These are current implementation mappings, not new product acceptance thresholds or runtime visual acceptance. Tuning must retain D1-D5's hierarchy and transparency; spacing must not force empty sections.

| Role | Current baseline |
| --- | --- |
| Panel horizontal / vertical inset / inter-section spacing | 24 / 8 / 24 logical px; fixed header/body/footer gaps use 6 logical px |
| Field-group gap / label-to-field gap | 12 / 6 logical px |
| Naming geometry | 620 logical px maximum width, 12 logical px viewport margins and 8 logical px panel inset; row height is UI line height + 12 logical px, matching the palette input row |
| Environment segments | 6 logical px surface gap; neutral hover overlay alpha 0.06, selected blue overlay alpha 0.10; no separate pressed appearance |
| Input padding | Review fields: 8 logical px; naming: palette-matched 6 logical px |
| Action / checkbox minimum height | 36 / 28 logical px; long labels may increase height |
| Control radius / selection marker | 7 logical px / 16 logical px icon |
| Page / section / label / supporting type | 1.6 / 1.2 / 0.95 / 0.9 times the UI font |
| Primary | Solid primary surface, including hover/pressed |
| Secondary / Quiet | Light translucent surface / no idle fill or outline |
| Selection / Checkbox | Inset choice track and check marker / no primary-button fill across the selected row |

Naming reuses the search icon-button composition, with a checkmark instead of a Save caption. Name-validation notifications are a separate feedback source using existing three-second expiry and duplicate suppression; recovery/cancel does not clear unrelated notices. Final Save remains the only primary submission in review, and review errors stay near their fields/actions.

## 6. End-to-End Acceptance

| ID | Given / When | Then |
| --- | --- | --- |
| E1 | Compare naming against the command palette at the same window size/DPI; enter from both save entries, type valid/invalid names and submit by checkmark/keyboard | Naming matches palette width, horizontal placement/margins and content insets, with height matching only its input row, not the command list; dimensions remain stable while editing, no inline warning or Save caption; checkmark retains transparent idle/search-like hover/press and visible focus; invalid submissions notify without advancing or taking focus; no write before review; repeated Enter/IME confirmation cannot save |
| E2 | Open a usable local preset with no agent or configured overrides, then a draft with both available | Simple review shows one launch summary, local Edit/Edit name actions and compact exclusion statements, without correction fields or empty sections; applicable choices and acknowledgements remain in the main review |
| E3 | Compare all inputs, choices, buttons and feedback and trigger applicable states | Equivalent roles are consistent; selection, focus, hover and disabled meanings are distinguishable, with pressed feedback where applicable; Q3's environment segments intentionally add no separate pressed appearance; only final Save is primary submission |
| E4 | Open/operate review over quiet, busy, high-contrast, light and dark terminal content while output continues | Live softened background/broad color transmission and recognizable edges; sharp readable foreground, distinguishable inputs, no full-window scrim or opaque imitation |
| E5 | Open Edit on a usable summary; switch Local/WSL/SSH with mouse/keyboard; submit invalid then valid corrections; also start with unknown context | Unknown context opens correction directly; target labels/path meanings remain correct; unconfirmed changes visibly block Save; invalid confirmation retains errors; valid confirmation returns to one updated summary without writing or bypassing independent approvals |
| E6 | Select agent/overrides and acknowledge separately, including synthetic suspected credentials; toggle details | Independent approvals/confidentiality unchanged; necessary warnings remain visible, exclusions understandable, source arguments not represented as replayed |
| E7 | Inspect long paths, Unicode, non-secret arguments and wrapped help; resize/change DPI | Readable text and complete inspectable/copyable values without source mutation, clipped line starts, out-of-bounds controls or optical hit-region shifts |
| E8 | Expand details, trigger errors, scroll the body and resize the review to a constrained window | Title/name and footer remain visible while the body scrolls; fields, warnings and keyboard targets remain reachable, actions wrap without overlap, and short content has no forced blank sections |
| E9 | Block saving, enter pending, encounter external-change/write failure, then validly retry | Distinct reasons/pending/failure; no duplicate authorization, false success or overwrite of external changes; retain draft and existing valid retry path |
| E10 | Use Edit name, change the name and return to review; cancel, replace the popup and deliver stale editor/IME/action events | Existing return-to-naming retains the draft/corrections; only the name edit changes the name; no extra footer return button, implicit approval, PTY leakage or old-flow commit of a new draft |
| E11 | Change terminal colors and compare existing search/palette/notification consumers | Natural transmitted colors may change; no terminal-theme-to-application-role mapping; unmigrated geometry, interaction and material contracts remain |
| E12 | Exercise relevant generic controls/styles outside a terminal and inspect dependencies | Caller-driven values/feedback without save-business dependencies; application identity does not become a compulsory generic default |
| E13 | In correction mode, select Local/WSL/SSH, hover an adjacent unselected segment, press selected/unselected segments, then activate by mouse/keyboard; also inspect focus and disabled states over quiet/busy backgrounds | Selected and adjacent hovered surfaces retain a clear visible gap without touching or merging; hover is neutral live liquid glass without added emphasis tint or opaque grey fill; the lightly tinted selected block and marker remain identifiable; pressing adds no extra darkening, color or depression, and only successful activation updates selection; keyboard focus remains visible without hover and disabled segments cannot change selection |

Acceptance covers both system consistency and the complete page. Shader compilation, control unit tests or a static screenshot alone cannot prove readability, live optics or save safety.

### Verification evidence

- Native before/after observations record revision, window size, DPI, backdrop, disclosure state and expected/actual results. Use synthetic paths/arguments/variable names, not secrets or unrelated terminal content.
- Observe short/long content, busy backgrounds, keyboard focus, pointer feedback, full-value copy and state transitions, not only an ideal empty background.
- Record actual tuned values and comparison conditions. This does not require Astryx pixel replication or invent performance/contrast thresholds.
- Retain existing clipping, alpha, DPI, bounded-resource and effects-disabled compatibility obligations. New layering must not silently change other consumers.
- Explicitly mark unexecuted native, GPU, automated and business-regression checks NOT RUN; document status is not evidence of passing execution.

### Naming/selector implementation checks

- **Source scope:** `066741e` plus the preserved in-progress tree and this correction; no commit or CONTEXT/ADR status change. The new checks below supersede the specification handoff's NOT RUN status only for the stated automated/offscreen coverage, not native visual acceptance.
- **PASS:** Mounted naming comparison with the actual command palette at logical widths 1000/400/280 and DPI scales 1.0/1.5/2.0, covering panel width/placement, insets, editor-text padding, input/checkmark height and stable dimensions without command-list space. Mounted segment tests cover separated surfaces, real neutral hover material, unchanged selected tint, no extra press appearance, successful activation, visible keyboard focus and no save/PTY write.
- **PASS:** `pwsh -NoProfile -File scripts/dev/test_cargo.ps1 --locked -p harbor-app -p harbor-widget --tests -- --test-threads=1 --quiet`; app unit suite: 234 passed/6 ignored; widget unit suite: 609 passed/21 ignored, plus ordinary widget integration suites including 11 selection-control tests. Generic tests cover gap validation/resizing, all pressed-appearance flag combinations and hover-material locality/selection/disabled precedence; ignored cases are not counted as passed.
- **PASS:** `HARBOR_TEST_GPU=dx12-warp` with `palette_render::gpu --ignored --nocapture --test-threads=1`: all five scoped application GPU cases passed, including naming/review, segment selection/neutral hover, search, palette and notification regression checks. Synthetic backgrounds, 1.0/1.5/2.0 scales and linear/sRGB BGRA targets; hovering changes pixels only inside its segment, leaves selection/gaps unchanged, pressing has identical pixels and pointer exit restores the baseline. Removing selected optics still differs from tint-only paint.
- **Regression strength:** The new mounted tests failed against the pre-correction width and missing gap. Temporary reintroduction of pressed feedback failed at the no-extra-press assertion; temporary removal of hover optics failed at the real-material assertion. Both mutations were undone before the final passing suites.
- **PASS:** `cargo clippy --locked -p harbor-app -p harbor-widget --all-targets --all-features -- -D warnings` and `cargo fmt --all -- --check`.
- **PASS:** `python scripts/docs/check_docs.py` (216 Markdown files) and `git diff --check`; documentation links, fragments, language and whitespace checks, not native UI acceptance.
- **Local evidence:** Optional PPM captures in `target/quick-save-evidence/naming-selector/`, including `quick-save-segments-hover-{scale}-{format}.ppm`; synthetic offscreen artifacts, not native-window screenshots or committed acceptance evidence.
- **NOT RUN:** Interactive native-window visual/IME/live-terminal acceptance, full workspace suites and unrelated ignored GPU/PTY/native cases. Subjective coherence/readability on the actual desktop remains to be confirmed; no runtime-acceptance status is advanced.

### Summary-first implementation checks

**Historical evidence boundary:** The summary-first results below predate the naming/selector correction; they do not independently verify the revised geometry or segment states. Use the scoped naming/selector results above and retain their native-acceptance limitations.

- **PASS:** Mounted Quick Save tests (24 ordinary cases), including summary/correction transitions, name-return draft retention, independent approvals, retired clipboard/IME delivery, unknown/unusable context, fixed chrome/body scrolling and long write errors at 280 × 180 logical px.
- **PASS:** `pwsh -NoProfile -File scripts/dev/test_cargo.ps1 --locked -p harbor-app -p harbor-widget --tests -- --test-threads=1 --quiet`; ordinary app/widget unit and integration suites. Ignored resource cases are not counted as passed.
- **PASS:** Targeted `quick_save_review_pixels` GPU case with `HARBOR_TEST_GPU=dx12-warp`, scales 1.0/1.5/2.0, linear/sRGB BGRA targets and synthetic striped background. Checks foreground glyphs, shared palette material and undimmed area outside the panel; not full live-backdrop/native acceptance.
- **PASS:** Scoped all-target/all-feature Clippy with warnings denied and workspace formatting check.
- **PASS:** `python scripts/docs/check_docs.py` (216 Markdown files) and `git diff --check`. Historical `scripts/check_docs.py` has moved; `scripts/checklist_summary.py` is absent and was unavailable, not passed.
- **Regression strength:** The new small-window write-error case failed with zero body height when the full error was in the fixed footer, then passed after retaining complete error text in the scrolling body and revealing it on failure.
- **Local visual artifact:** `target/quick-save-evidence/quick-save-summary.png` plus PPM captures, generated by setting `HARBOR_QUICK_SAVE_CAPTURE_DIR` for the GPU test. These are synthetic local build artifacts, not retained native evidence.
- **NOT RUN:** Interactive native-window visual/IME/live-terminal acceptance, full workspace tests/Clippy, and unrelated ignored GPU/PTY/native suites. No specification or ADR acceptance status is changed.

## 7. Decisions, Traceability and Limitations

| Decision | Source |
| --- | --- |
| Astryx organization plus restrained liquid glass, not web embedding | Owner request/accepted recommendation; ADR 0076 native path |
| Reusable application foundations, Quick Save first, not a complete application restyle | First recommended scope accepted by the owner |
| Initial specification-only endpoint, no tickets/implementation | Second recommended scope accepted by the owner; subsequent explicit loop invocation separately authorizes implementation |
| Semantic roles, typography, spacing, corners, materials, states, motion and readability | Accepted specification contents, organized by D1-D5 |
| Two stages, summary-first review, on-demand correction, local name editing and fixed review chrome | Owner accepted the latest screenshot recommendation; 0053's affected presentation clauses are amended together; 0052 R5-R7/R9 safety remains |
| Naming aligned to palette input-row geometry; spaced environment segments, neutral glass hover, no separate press treatment and retained selection/focus | Owner approved the follow-up refinement proposal; Q1/Q3 and E1/E3/E13 express it without fixed pixel thresholds; 0053/0054 synchronized |
| Shared application recipes, generic widget mechanisms, independent terminal themes | ADR 0064, 0070, 0076 and 0058; none changed here |

**Limitations:** Refinement used the owner-supplied screenshot and source inspection. Implementation has mounted-flow, layout and offscreen GPU evidence, not a fresh interactive native-window acceptance run. The schematic is not a pixel-final design; exact wording, fonts, colors, spacing and corners remain reversible tuning. No new localization, application-mode policy, business semantics or material architecture is selected. Live terminal backdrops, native IME/focus and subjective appearance still require native observation.

**Knowledge recording Pending:** The new reusable design direction is persisted as this task contract, not as a new CONTEXT concept or ADR. No knowledge-write authorization was supplied and existing ADR statuses were not changed. If recording is later requested, resume from the confirmed sources in this section with deduplication/consistency checks. Pending knowledge recording does not block delivery of the specification.

**Evidence boundary:** Specification checks establish source, link and scope consistency, not runtime behavior. Implementation checks and their omissions must be reported separately; none automatically changes this document to accepted.

## 8. Exclusions

- Ticket slicing, implementation-plan artifacts, commits and remote actions. The refinement itself ran no implementation/QA; the later explicit implementation invocation authorizes scoped code and checks, not automatic acceptance.
- Whole-application redesign, all-component migration, a full preset editor, merging naming and final confirmation, or further changes to save flow beyond Q1-Q4.
- React/WebView/Astryx runtime adoption, catalog-wide component ports or a second state/input/popup framework.
- New application-mode selection policy, user appearance settings or coupling controls to terminal theme palettes.
- Mandatory new font assets, brand colors, optical algorithms, continuous decorative animation or desktop capture.
- Changes to capture, environment recognition, arguments, safety acknowledgements, configuration paths, persistence or relaunch contracts.
- Treating a schematic or earlier tests as runtime acceptance of this visual design.
