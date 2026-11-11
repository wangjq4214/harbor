# Quick Save Review UI and Reusable Controls

**Spec ID:** 0053
**Status:** Draft; settled requirements, not implementation or runtime acceptance
**Date:** 2026-11-07
**Sources:** Owner-supplied screenshot of the Quick Save confirmation overlay; the refinement conversation approving a restrained, clearer tool-style presentation and asking which components belong in widget; subsequent approval of one UI-and-widget-boundary spec without ticket slicing or implementation. Existing behavior is governed by [spec 0052](./0052-named-session-profiles-and-quick-save.md).
**Presentation refinement source:** Earlier owner feedback approved larger headings and reading blocks. The latest screenshot feedback identified excessive visual weight and operation complexity; the owner accepted summary-first review with on-demand correction, retained naming/confirmation stages and revision of the existing specifications only. This supersedes the always-visible correction form and three-action footer, not save safety.

**Naming/selector refinement source:** The owner subsequently approved palette-input-row naming geometry; visible gaps between environment segments; neutral liquid-glass hover without an added emphasis color; no independent pressed appearance; and retained light selection tint/marker and keyboard focus. That specification turn authorized only updates to the existing design-system body and 0053/0054 entries, without implementation, tickets or CONTEXT/ADR writes. Subsequent explicit loop invocation authorizes implementation separately; evidence is linked below.
**Source snapshot:** `3aa8e98`, clean working tree before this document. Inspection covered Quick Save composition, project review, application glass appearance, widget exports, text fields, labels and shared control surfaces. This is a scoped source assessment, not a runtime diagnosis of the screenshot's clipping.
**Related design-system extension:** [Design system (Spec 0054)](../../docs/app/design-system.md) is the canonical visual foundation and detailed Quick Save presentation contract. This revision aligns its Q1-Q4 with the affected requirements here; the two-stage workflow, widget/business boundaries and spec 0052's safety remain.

## Outcome

The in-window Quick Save review reads as a focused task form rather than a flat diagnostic dump. Users can identify the preset, review its execution environment and directory, resolve blocking issues and explicitly save without losing existing safety, editing or persistence guarantees. Generic selection controls and explanatory text layout become reusable widget capabilities; Harbor's recipe semantics, workflow and visual recipes remain application-owned.

This spec refines presentation and its component boundary. It does not replace spec 0052's project capture, recognized-agent, confirmation, configuration-write or relaunch requirements, or claim those requirements have already passed runtime acceptance.

## Requirements

### R1 — Compact, bounded in-window review

- Keep confirmation inside the existing application window. Size the review to its content rather than retaining the screenshot's large unused lower area; constrain it to the available viewport and make overflowing body content scrollable.
- Keep the title/name and final action area visible while the review body scrolls. On constrained windows, wrapping and sizing must retain reachable fields, warnings and keyboard targets without fixed regions covering the body.
- Retain return-to-naming as a quiet Edit name action beside the displayed name, preserving the draft; keep Cancel and explicit final Save in the footer. Cancel writes nothing. This repositions and clarifies spec 0052's Return to edit capability, not an inline rename or a full profile editor.
- Keep the existing input-only naming step and its adjacent advance button from spec 0052 R5; the redesign is not permission to merge naming and final confirmation into one submit action.
- Match naming width, horizontal placement/margins and content insets to the command palette under the same window/DPI conditions. Naming height matches only the command palette input row, not its list-bearing panel, while retaining the adjacent advance control. Change naming geometry, not the command palette; retain stable dimensions while editing and the canonical Q1's notification-only invalid-name feedback.

### R2 — Task-oriented information hierarchy

Organize review around:

1. **What will be saved:** preset name, execution environment and project directory, with agent inclusion and confirmed arguments available for meaningful review when applicable.
2. **What needs attention:** unknown or edited context, invalid fields, required acknowledgements and save failures.
3. **Supporting details:** source program/startup-argument provenance and restoration limitations.

Express that hierarchy in this reading order, using the summary-first presentation in [0054 Q1-Q4](../../docs/app/design-system.md#4-quick-save-page-rules):

- **Review header:** page title, inspectable preset name and local Edit name action returning to naming.
- **Launch location:** one usable environment/destination/directory summary with a local Edit action. Unknown context goes directly to correction with an explanation; a usable summary does not show correction fields by default.
- **Launch contents:** applicable agent inclusion, invocation and required confirmations/warnings, followed by configured-override choices and independent acknowledgement. No agent means a concise shell-only explanation; no available configured overrides means a concise exclusion statement, not empty chapters. Available but unselected overrides still expose their choices.
- **Supporting details:** one compact disclosure row for source provenance and restoration limitations, not a heading plus detached large button. Required warnings remain outside disclosure.
- **Final actions:** Cancel and Save, visually distinct from body content. Name/location editing stays beside the affected content.

Keep field errors, safety warnings and acknowledgement instructions next to their relevant content or controls rather than collecting them into a detached generic warning block. Save failures remain visible near the final actions. These block labels express grouping, not mandatory UI wording or localization.

Supporting detail may use collapsible sections to reduce visual noise. Required warnings, suspected-credential handling, blocking feedback and necessary confirmation controls must not disappear into a collapsed section. Where details are collapsed, inclusion/exclusion state remains understandable without opening every section. Full required review content remains accessible; do not remove it merely to shorten the form.

Use concise user-facing labels and explanations. Distinguish source startup arguments that are not replayed from any agent invocation actually selected for relaunch. Do not imply that initial cwd is evidence of the current project directory, that corrections were recognized automatically, or that omitted/changed agent arguments are an exact copy.

### R3 — Environment-specific form and actionable feedback

- In correction mode, use a real single-choice segmented control for Local pwsh / WSL bash / Direct SSH bash, not character-marked buttons.
- Keep clear gaps between adjacent environment-segment surfaces even when one is selected and another hovered. Hover is lightweight neutral native liquid glass with live background transmission, no added emphasis-color tint or opaque grey fill. Selection retains its light emphasis tint and marker. Pressing retains the applicable hover/selected appearance without added darkening, color change or depression; only successful activation updates caller-owned selection. Preserve independent visible keyboard focus, disabled behavior and existing activation semantics; this local treatment does not restyle all controls.
- Show no WSL/SSH destination field for local PowerShell. For WSL, label the target as a distribution; for direct SSH, label it as the SSH destination. The application owns these choices and their validation semantics.
- Give inputs persistent labels; placeholders provide examples rather than being the only labels. Keep project-directory interpretation tied to the selected environment.
- Reveal correction fields on the location's Edit action, or immediately for unknown/unusable context. Present a local secondary action such as Confirm launch location; typing/selecting does not approve changes. While editing, show pending/invalid status rather than an old approved summary competing with the edited values. Invalid confirmation retains the fields and errors; successful explicit confirmation returns to one updated summary without saving or bypassing independent acknowledgements.
- Explain blocked saving near the relevant field or confirmation control instead of relying only on a greyed-out Save button. Write failures remain visible and retain the draft for correction/retry.
- Replace character-marked boolean buttons for agent inclusion, argument acknowledgement and configured-override choices with actual checkbox presentation where those choices exist. Do not change default inclusion, opt-in, acknowledgement or secret-handling policy to match a new control's appearance.

### R4 — Readable material, controls and text

- Preserve the translucent glass character used by the command palette and search: share their application-owned backdrop material, subtle panel tint and distinguishable translucent input surfaces. Keep softened background colors visible rather than replacing the review or inputs with near-opaque fills or adding a dark full-window scrim. Readability must come from the shared glass treatment and foreground/control hierarchy, not loss of transparency.
- Distinguish title, body and auxiliary text typographically and by foreground treatment. Give inputs a discernible surface, and make primary Save visually distinct from secondary and cancel actions. Hover, pressed, selected, disabled and keyboard-focus feedback must be discernible where applicable. The environment segments deliberately have no separate pressed appearance as specified in R3; this does not remove selection or keyboard-focus feedback.
- In the final action area, arrange Cancel then Save as a right-aligned group when space permits. Save is the sole solid primary action. The quiet Edit name action sits beside the name rather than duplicating Return to edit in the footer. Narrow layouts may wrap or stack without losing complete labels, usable targets or reachability; local actions remain beside their content.
- Use a clear type scale: the page title is the most prominent heading; major block headings are visibly larger than persistent field labels and body text; helper text stays subordinate but readable. A case change or foreground-color change alone does not satisfy the heading-size distinction.
- Group related fields with typography and spacing, retaining larger gaps between meaningful groups than within them. Use restrained divider lines only where needed, not a mandatory line at every section boundary. Remove empty chapters before adding whitespace; avoid nested bordered/glass cards and competing outlines. Preserve discernible input surfaces, compact sizing and grouping when content wraps or details expand.
- Explanatory and error text wraps within available content width. Fix the screenshot's clipped message presentation; do not solve it by silently removing the leading text or hiding warnings behind ellipses.
- Keep long captured paths and non-secret argument values fully inspectable, selectable and copyable. Wrapping explanatory text does not authorize replacing existing read-only value editors with non-selectable labels, shortening copied values or revealing redacted secrets.
- Preserve source values across resizing and layout changes. Use UI-renderer text measurements rather than app-maintained character/cell-count line breaking.
- Exact dimensions, font sizes, colors, alpha values, radii and wording are reversible presentation tuning, not new fixed acceptance thresholds or user-facing settings. The schematic discussed in chat is an information-hierarchy illustration, not a pixel specification or a localization requirement.

### R5 — Reusable widget capabilities and state contract

Deliver the following generic capabilities in `harbor-widget` and use them in Quick Save:

| Capability | Required contract | Application responsibility |
| --- | --- | --- |
| Single-choice segmented control | Accept choices and the caller's selected value; expose selection intent; render selection and support mouse/keyboard use, disabled state and visible focus | Supply environment choices, own selected environment and perform recipe/correction transitions |
| Checkbox | Accept the caller's checked state; expose change intent; render checked/unchecked state and support mouse/keyboard use, disabled state and visible focus | Own inclusion, acknowledgement, configured-override selection and save eligibility |
| Wrapping explanatory text | Measure/layout text under available width using widget/UI text geometry, retaining source text through re-layout | Supply message content and decide whether it is supporting information, warning or error |

These are capability contracts, not final Rust signatures. `SegmentedControl` and `Checkbox` are working names. Application state remains authoritative for selected/checked business values; controls may retain local interaction state but must not maintain a competing business-state copy. Reuse existing generic interaction/focus mechanisms rather than introduce a second input system.

A lightweight form-field composition may group a label, supplied input, helper text and supplied error. Its extraction is optional, not a requirement for a form framework. It must not infer validation rules, hold a duplicate field value or become a save controller.

### R6 — Keep business and product policy above widget

Apply the existing ownership rule from [ADR 0064](../adr/0064-widget-and-application-component-ownership.md): a widget contract must make sense outside a terminal application without interpreting Harbor business types.

Keep these concerns in `harbor-app`:

- Environment meanings, target-field visibility and labels, directory rules, draft capture/correction and recipe interpretation.
- Agent inclusion consequences, suspected-secret handling, acknowledgements, save eligibility, final authorization and persistence/retry.
- Popup ownership/generation, stale-request admission and application command dispatch.
- Information grouping, default detail disclosure, review layout and Harbor's material/appearance recipe.

Reusable widget contracts must not depend on `ProjectRecipe`, `WorkspaceUiController`, Quick Save generations or profile/session save commands. Existing `Portal`, constraint/layout, scrolling and focus capabilities should serve application-owned review composition; no generic dialog manager or popup stack is selected. Existing application `GlassAppearance` remains the owner of product presets and local overrides, in accordance with [ADR 0070](../adr/0070-shared-application-glass-presets-with-local-overrides.md).

### R7 — Preserve interaction and save safety

Retain spec 0052 R5–R7/R9 while changing presentation:

- Enter or the naming advance button only advances to review. Held/repeated Enter and IME candidate confirmation must not also trigger final Save.
- Review interactions do not leak into underlying popups or PTY input. Retired editors, stale events/requests and replaced flows cannot change or commit a new flow.
- Unknown/edited context remains explicitly correctable and cannot be saved as approved merely because fields are visible or a selector changed.
- Acknowledgements remain independent where required; selecting an environment or including an agent does not implicitly acknowledge argument or environment-variable safety. Suspected credentials remain hidden/handled without persistence or disclosure.
- A pending or failed operation must not be presented as successful. Preserve conflict-safe selected-file writes, configured-overrides opt-in, stable draft semantics and truthful retry behavior.

These are regression constraints, not a request to redesign capture, configuration persistence or environment-aware relaunch.

## Solution and Integration Seams

Compose the summary-first review and on-demand correction at the application layer using the existing stable draft, validation/save gates and reusable layout/scroll/focus/control mechanisms. Presentation mode is not a second recipe owner and does not reset or confer independent approvals. Use existing `Button`, `TextField` and read-only `EditableText` capabilities rather than save-specific widget types. Wrapping's concrete component/API remains open; no second editor or text-shaping redesign is selected.

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Controlled selection | Application draft/review state → widget controls → application update | Generic choices and current selected/checked values, enabled state and callbacks | User intent and generic feedback without owning or interpreting the recipe |
| Form feedback | Application draft/correction validation/save result → review presentation | Usable/unknown context, edited values, approvals, errors and eligibility | Summary or correction of the same stable draft, visible unconfirmed changes and nearby feedback; no widget-owned validation or save admission |
| Text display | Application messages and review values → widget text/editor capabilities | Available width, original display-safe text and read-only/selectable value state | Wrapped explanations and complete inspectable/copyable non-secret values without app-side line breaking |
| Popup lifecycle | Existing exclusive popup/application commands → review composition | Current flow lifetime and established input/authorization policy | One in-window review, no underlying input leakage or stale-flow commit |
| Appearance | Application appearance → generic decoration/filter/control styling | Harbor-owned material and foreground choices | Clear surface/control hierarchy without moving product recipes into widget defaults |
| Final save | Existing application review → Host/config writer | Explicit final approval of the validated draft | Existing conflict-safe write/publication or visible failure retaining input; controls do not write files |

Inspected surfaces are `crates/harbor-app/src/quick_save.rs`, `quick_save_project.rs` and `appearance.rs`, plus `crates/harbor-widget/src/widgets/` exports, `text_label.rs`, `text_field.rs` and `control_surface.rs`. These locate current responsibilities, not mandated new file/module names.

## End-to-End Acceptance

| ID | Given / when | Required observation |
| --- | --- | --- |
| E1 | Compare naming with the command palette at the same window size/DPI, then enter a valid name from either save entry and advance by button/keyboard | Width, horizontal placement/margins and content insets match the palette; height matches only its input row, not the command list; input-only naming and stable editing dimensions remain; one readable in-window review opens, no file is written, no native window opens and repeated Enter does not save |
| E2 | Review a short usable local preset with no agent/configured overrides, then constrain the window or open a longer draft | One launch summary and compact shell-only/exclusion statements, no initial correction fields or empty chapters; body scrolls while title/name/footer remain visible and required content remains reachable |
| E3 | Open the location's Edit action, then switch local PowerShell → WSL → direct SSH with mouse and keyboard | Correction fields replace the summary; exactly one environment is selected, Local has no target and WSL/SSH have correctly labeled targets; existing path and agent consequences remain |
| E4 | Start with unknown context or edit a known location; submit invalid then valid corrections | Unknown context opens correction directly; pending changes visibly block saving; invalid confirmation retains adjacent errors; valid explicit confirmation returns to one updated summary without writing or satisfying independent safety gates |
| E5 | Review a draft with a recognized agent and available configured overrides; operate its boolean choices | Real checkbox states track application values; agent and override acknowledgements remain independent; defaults and redaction are unchanged; disabled controls do not mutate the draft |
| E6 | Open/collapse supporting details with both ordinary and suspected-credential data | Supporting provenance and limitations are accessible; inclusion state remains understandable; mandatory warning/handling/confirmation controls are not hidden; no credentials are exposed |
| E7 | Review long synthetic paths/arguments, Unicode text and multi-line explanations; resize and copy values | Explanations wrap using UI geometry; required messages remain readable; full display-safe values can still be inspected, selected and copied without source truncation or app-inserted line breaks |
| E8 | Use Edit name, change the name and return to review; cancel, replace the popup, confirm IME candidates and deliver late/repeated input | Existing draft/corrections and return-to-naming behavior are retained with the local action; no implicit approval, extra commit, stale-flow mutation or leaked PTY input |
| E9 | Explicitly Save a valid reviewed draft, then repeat with a write conflict/failure | Successful persistence/publication uses the existing selected-file contract; failure is visible and retains the draft; visual redesign causes no silent overwrite, premature success claim or permission bypass |
| E10 | Render and exercise generic selection/text capabilities without Harbor profile or terminal state | Public contracts remain usable independently; caller-owned values drive visuals and callbacks return intent; no Harbor business dependencies or competing selected/checked state are required |
| E11 | Compare review with the command palette over both quiet and visually busy terminal backgrounds | The review retains comparable glass transparency, softened background color and optical treatment without a dark full-window scrim; review surface, inputs, text hierarchy, selection/focus feedback and primary/secondary actions remain distinguishable, foreground stays sharp and hit regions are unchanged |
| E12 | Review with an available agent and configured overrides, then without them; expand supporting details | Applicable choices and mandatory confirmations stay in the main reading order; absent items become compact truthful explanations, not empty sections; details starts as one row; hierarchy is clear without mandatory per-section dividers or nested cards |
| E13 | In a constrained window, trigger a context-validation error, a required safety acknowledgement and a save failure; expand/collapse supporting details | Field errors and required safety information stay beside their relevant content/controls, save failure remains visible near final actions, and mandatory warnings are never collapsed away; wrapped content, dividers and larger headings do not make required content or actions unreachable |
| E14 | Review at fitting and narrow widths; scroll long body content and trigger a save failure | Cancel then Save are in the visible footer, right-aligned when fitting and wrapped when necessary; Edit name remains beside the name, no duplicate footer return action; failures stay near the footer and no fixed region covers required content |
| E15 | In correction mode, select an environment, hover its neighbor, press selected/unselected segments and activate with mouse/keyboard; inspect focus and disabled states on quiet/busy backdrops | Adjacent selected/hovered surfaces stay visibly separated; hover is neutral live liquid glass without emphasis tint or opaque grey fill; selection keeps its light tint and marker; pressing adds no extra darkening/color/depression; successful activation alone changes selection, keyboard focus remains visible without hover and disabled controls do not change values |

## Decisions and Traceability

- **One spec, no slice or implementation:** the owner's final approval selected the proposed UI-and-widget-boundary scope. Existing spec 0052 remains the behavioral contract; a focused companion avoids rewriting unrelated launch/capture requirements.
- **Task-form presentation and restrained glass:** approved screenshot discussion supplies R1–R4, including compact sizing, supporting-detail disclosure, persistent labels, secondary correction and visible blocked-save feedback. It does not supply exact visual tokens or a broader workspace redesign.
- **Naming geometry and segment-state refinement:** The owner's follow-up approval supplies R1's palette-input-row geometry and R3/R4's spaced segments, neutral glass hover, no separate pressed appearance and retained selection/focus, covered by E1/E15. This is a local visual refinement, not a change to activation, environment meaning, widget-wide defaults or save safety; exact pixel gaps and optical values remain tuning.
- **Preserve palette-like transparency:** the owner's later rejection of the opaque review explicitly supersedes R4's earlier more-solid-surface/darker-scrim direction. Reuse the command palette's glass and translucent editor appearance while retaining the approved grouping, headings, action layout, readability and save-safety constraints.
- **Reading hierarchy refinement:** Earlier approval established recognizable groups and heading scale. The latest accepted screenshot recommendation refines R2/R4/E12-E13 to summary-first content, compact empty states, a one-row details disclosure and fewer dividers. Required review, warnings and long-value inspection remain; this is not new capture or persistence policy.
- **Final actions and scrolling:** The latest accepted recommendation supersedes the earlier Cancel / Return to edit / Save footer: Cancel / Save remain there, while Edit name beside the name preserves return-to-naming. Title/name and footer stay visible around body scrolling. This is a Quick Save rule, not a whole-application button or dialog framework.
- **Prioritize selection controls and wrapping:** the approved boundary recommendation supplies R5. Form-field extraction is optional; a reusable disclosure component is not required. These are task scope choices rather than a new general design-system architecture.
- **Mechanism versus product policy:** [ADR 0064](../adr/0064-widget-and-application-component-ownership.md), [ADR 0065](../adr/0065-layered-widget-text-input.md), [ADR 0070](../adr/0070-shared-application-glass-presets-with-local-overrides.md) and [ADR 0083](../adr/0083-composable-buttons-and-shared-control-surfaces.md) support R5–R6. Reuse their state, editor, interaction and appearance boundaries; no new ADR supersession is selected.
- **Existing confirmation/safety contract:** [ADR 0107](../adr/0107-shared-palette-profile-flow-and-in-window-confirmation.md) and spec 0052 support R1/R7 and the safety cases. Detail disclosure cannot weaken required review or credential handling.

## Verification Guidance

Verify generic controls' caller-driven state, disabled behavior, pointer/keyboard interaction and focus feedback, plus wrapping under constrained width and re-layout. Verify application composition separately for environment-specific fields, nearby error placement, disclosure and retained selectable values. Generic tests alone do not prove the actual Quick Save migration or its save gates.

Use synthetic directories, arguments and environment entries for native visual/interaction evidence. Include a short and overflowing review, quiet and busy terminal backgrounds, window resizing and the existing Windows DPI/IME/focus paths. Record the tested revision, display/window conditions, steps and expected versus observed results. For conflicts, pending saves and suspected credentials, verify truthful UI and unchanged data/authorization behavior rather than only screenshots. No new performance threshold or screenshot-exact golden values are prescribed.

The specification-only refinement performed no implementation, automated test suite or runtime QA; naming-geometry and segment-state checks were NOT RUN at that handoff. Subsequent explicitly authorized implementation and scoped automated/offscreen results are recorded in the [canonical design-system evidence](../../docs/app/design-system.md#namingselector-implementation-checks), with native visual acceptance still outstanding. This does not advance this spec's acceptance status. Exact component API spellings and reversible visual tuning remain implementation details; there are no pending blocking requirements decisions in this scope.

## Out of Scope

- Ticket slicing, implementation planning, production code changes or execution/QA as part of this refinement.
- A whole-application/sidebar redesign, full graphical profile editor, new native save window or generic dialog/popup-management framework.
- A complete design system, mandatory `FormField`/`Disclosure` widget, save-specific widget types or a second interaction/editor system.
- New user-facing style settings, unrelated widget-default restyling, text-shaping redesign or mandatory localization changes.
- Changes to capture/recognition support, environment/agent semantics, persistence schema, write authorization, configuration path selection or existing relaunch coverage.
- Commits, staging, remote actions or changes to existing tickets and their acceptance status.
