# Quick Save Review UI and Reusable Controls

**Spec ID:** 0053
**Status:** Draft; settled requirements, not implementation or runtime acceptance
**Date:** 2026-11-07
**Sources:** Owner-supplied screenshot of the Quick Save confirmation overlay; the refinement conversation approving a restrained, clearer tool-style presentation and asking which components belong in widget; subsequent approval of one UI-and-widget-boundary spec without ticket slicing or implementation. Existing behavior is governed by [spec 0052](./0052-named-session-profiles-and-quick-save.md).
**Source snapshot:** `3aa8e98`, clean working tree before this document. Inspection covered Quick Save composition, project review, application glass appearance, widget exports, text fields, labels and shared control surfaces. This is a scoped source assessment, not a runtime diagnosis of the screenshot's clipping.

## Outcome

The in-window Quick Save review reads as a focused task form rather than a flat diagnostic dump. Users can identify the preset, review its execution environment and directory, resolve blocking issues and explicitly save without losing existing safety, editing or persistence guarantees. Generic selection controls and explanatory text layout become reusable widget capabilities; Harbor's recipe semantics, workflow and visual recipes remain application-owned.

This spec refines presentation and its component boundary. It does not replace spec 0052's project capture, recognized-agent, confirmation, configuration-write or relaunch requirements, or claim those requirements have already passed runtime acceptance.

## Requirements

### R1 — Compact, bounded in-window review

- Keep confirmation inside the existing application window. Size the review to its content rather than retaining the screenshot's large unused lower area; constrain it to the available viewport and make overflowing body content scrollable.
- The title, body and actions must have a clear hierarchy. On a constrained window, all fields, warnings and actions remain reachable rather than being clipped outside the usable surface.
- Retain Return to edit, Cancel and explicit final Save. Return to edit preserves the draft; Cancel writes nothing. Do not convert the review into a separate native window or a full profile editor.
- Keep the existing input-only naming step and its adjacent advance button from spec 0052 R5; the redesign is not permission to merge naming and final confirmation into one submit action.

### R2 — Task-oriented information hierarchy

Organize review around:

1. **What will be saved:** preset name, execution environment and project directory, with agent inclusion and confirmed arguments available for meaningful review when applicable.
2. **What needs attention:** unknown or edited context, invalid fields, required acknowledgements and save failures.
3. **Supporting details:** source program/startup-argument provenance, configured environment overrides and restoration limitations.

Supporting detail may use collapsible sections to reduce visual noise. Required warnings, suspected-credential handling, blocking feedback and necessary confirmation controls must not disappear into a collapsed section. Where details are collapsed, inclusion/exclusion state remains understandable without opening every section. Full required review content remains accessible; do not remove it merely to shorten the form.

Use concise user-facing labels and explanations. Distinguish source startup arguments that are not replayed from any agent invocation actually selected for relaunch. Do not imply that initial cwd is evidence of the current project directory, that corrections were recognized automatically, or that omitted/changed agent arguments are an exact copy.

### R3 — Environment-specific form and actionable feedback

- Replace the character-marked Local pwsh / WSL bash / Direct SSH bash buttons with a real single-choice segmented control.
- Show no WSL/SSH destination field for local PowerShell. For WSL, label the target as a distribution; for direct SSH, label it as the SSH destination. The application owns these choices and their validation semantics.
- Give inputs persistent labels; placeholders provide examples rather than being the only labels. Keep project-directory interpretation tied to the selected environment.
- Present Apply context correction as a secondary action associated with the relevant fields, not a visually dominant full-width primary action. Preserve its existing explicit validation/approval boundary; typing or selecting an environment alone does not authorize saving.
- Explain blocked saving near the relevant field or confirmation control instead of relying only on a greyed-out Save button. Write failures remain visible and retain the draft for correction/retry.
- Replace character-marked boolean buttons for agent inclusion, argument acknowledgement and configured-override choices with actual checkbox presentation where those choices exist. Do not change default inclusion, opt-in, acknowledgement or secret-handling policy to match a new control's appearance.

### R4 — Readable material, controls and text

- Use a more solid, distinguishable review surface with a darker, more neutral background scrim and less distracting terminal bleed-through. Retain restrained glass styling rather than adding more visual effects.
- Distinguish title, body and auxiliary text typographically and by foreground treatment. Give inputs a discernible surface, and make primary Save visually distinct from secondary and cancel actions. Hover, pressed, selected, disabled and keyboard-focus feedback must be discernible where applicable.
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

Compose the task-oriented review at the application layer using reusable selection controls, text layout and existing layout/scroll/focus mechanisms. Use existing `Button`, `TextField` and read-only `EditableText` capabilities where appropriate rather than creating save-specific widget types. Whether wrapping extends an existing text component or uses a new generic component remains open; the display contract is required, not a second editor or a text-shaping redesign.

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Controlled selection | Application draft/review state → widget controls → application update | Generic choices and current selected/checked values, enabled state and callbacks | User intent and generic feedback without owning or interpreting the recipe |
| Form feedback | Application validation/save result → field/review presentation | Supplied labels, errors, eligibility and warnings | Nearby, readable feedback; no widget-owned validation or save admission |
| Text display | Application messages and review values → widget text/editor capabilities | Available width, original display-safe text and read-only/selectable value state | Wrapped explanations and complete inspectable/copyable non-secret values without app-side line breaking |
| Popup lifecycle | Existing exclusive popup/application commands → review composition | Current flow lifetime and established input/authorization policy | One in-window review, no underlying input leakage or stale-flow commit |
| Appearance | Application appearance → generic decoration/filter/control styling | Harbor-owned material and foreground choices | Clear surface/control hierarchy without moving product recipes into widget defaults |
| Final save | Existing application review → Host/config writer | Explicit final approval of the validated draft | Existing conflict-safe write/publication or visible failure retaining input; controls do not write files |

Inspected surfaces are `crates/harbor-app/src/quick_save.rs`, `quick_save_project.rs` and `appearance.rs`, plus `crates/harbor-widget/src/widgets/` exports, `text_label.rs`, `text_field.rs` and `control_surface.rs`. These locate current responsibilities, not mandated new file/module names.

## End-to-End Acceptance

| ID | Given / when | Required observation |
| --- | --- | --- |
| E1 | Enter a valid name from either existing save entry and advance by button or keyboard | Input-only naming remains; one readable in-window review opens, no file is written, no native window opens and repeated Enter does not save |
| E2 | Review a short captured local preset, then constrain window size or review a longer draft | Short content has no large forced lower blank area; overflowing content is scrollable; messages, fields and actions remain reachable without clipped line starts |
| E3 | Switch local PowerShell → WSL → direct SSH using mouse and separately keyboard | Exactly the current environment is visibly selected; local has no target input, WSL/SSH show correctly labeled targets; focus is visible; application correction semantics remain in force |
| E4 | Start with unknown context; edit directory/target; attempt saving before and after applying corrections | The blocking issue is readable near the relevant controls; edits alone do not authorize Save; applying invalid correction shows feedback, while a valid correction can satisfy that gate without bypassing independent acknowledgements |
| E5 | Review a draft with a recognized agent and available configured overrides; operate its boolean choices | Real checkbox states track application values; agent and override acknowledgements remain independent; defaults and redaction are unchanged; disabled controls do not mutate the draft |
| E6 | Open/collapse supporting details with both ordinary and suspected-credential data | Supporting provenance and limitations are accessible; inclusion state remains understandable; mandatory warning/handling/confirmation controls are not hidden; no credentials are exposed |
| E7 | Review long synthetic paths/arguments, Unicode text and multi-line explanations; resize and copy values | Explanations wrap using UI geometry; required messages remain readable; full display-safe values can still be inspected, selected and copied without source truncation or app-inserted line breaks |
| E8 | Return to edit, cancel, replace the popup, confirm IME candidates and deliver late/repeated input | Draft retention, no-write cancellation, exclusive lifetime and terminal-focus restoration follow spec 0052; no stale approval, extra commit or leaked PTY input |
| E9 | Explicitly Save a valid reviewed draft, then repeat with a write conflict/failure | Successful persistence/publication uses the existing selected-file contract; failure is visible and retains the draft; visual redesign causes no silent overwrite, premature success claim or permission bypass |
| E10 | Render and exercise generic selection/text capabilities without Harbor profile or terminal state | Public contracts remain usable independently; caller-owned values drive visuals and callbacks return intent; no Harbor business dependencies or competing selected/checked state are required |
| E11 | Compare review over both quiet and visually busy terminal backgrounds | Review surface, inputs, text hierarchy, selection/focus feedback and primary/secondary actions remain distinguishable; reduced bleed-through does not blur foreground text or change hit regions |

## Decisions and Traceability

- **One spec, no slice or implementation:** the owner's final approval selected the proposed UI-and-widget-boundary scope. Existing spec 0052 remains the behavioral contract; a focused companion avoids rewriting unrelated launch/capture requirements.
- **Task-form presentation and restrained glass:** approved screenshot discussion supplies R1–R4, including compact sizing, supporting-detail disclosure, persistent labels, secondary correction and visible blocked-save feedback. It does not supply exact visual tokens or a broader workspace redesign.
- **Prioritize selection controls and wrapping:** the approved boundary recommendation supplies R5. Form-field extraction is optional; a reusable disclosure component is not required. These are task scope choices rather than a new general design-system architecture.
- **Mechanism versus product policy:** [ADR 0064](../adr/0064-widget-and-application-component-ownership.md), [ADR 0065](../adr/0065-layered-widget-text-input.md), [ADR 0070](../adr/0070-shared-application-glass-presets-with-local-overrides.md) and [ADR 0083](../adr/0083-composable-buttons-and-shared-control-surfaces.md) support R5–R6. Reuse their state, editor, interaction and appearance boundaries; no new ADR supersession is selected.
- **Existing confirmation/safety contract:** [ADR 0107](../adr/0107-shared-palette-profile-flow-and-in-window-confirmation.md) and spec 0052 support R1/R7 and the safety cases. Detail disclosure cannot weaken required review or credential handling.

## Verification Guidance

Verify generic controls' caller-driven state, disabled behavior, pointer/keyboard interaction and focus feedback, plus wrapping under constrained width and re-layout. Verify application composition separately for environment-specific fields, nearby error placement, disclosure and retained selectable values. Generic tests alone do not prove the actual Quick Save migration or its save gates.

Use synthetic directories, arguments and environment entries for native visual/interaction evidence. Include a short and overflowing review, quiet and busy terminal backgrounds, window resizing and the existing Windows DPI/IME/focus paths. Record the tested revision, display/window conditions, steps and expected versus observed results. For conflicts, pending saves and suspected credentials, verify truthful UI and unchanged data/authorization behavior rather than only screenshots. No new performance threshold or screenshot-exact golden values are prescribed.

This spec-only refinement performs no implementation, automated test suite or runtime QA and establishes no new acceptance evidence. Exact component API spellings and reversible visual tuning remain implementation details; there are no pending blocking requirements decisions in this scope.

## Out of Scope

- Ticket slicing, implementation planning, production code changes or execution/QA as part of this refinement.
- A whole-application/sidebar redesign, full graphical profile editor, new native save window or generic dialog/popup-management framework.
- A complete design system, mandatory `FormField`/`Disclosure` widget, save-specific widget types or a second interaction/editor system.
- New user-facing style settings, unrelated widget-default restyling, text-shaping redesign or mandatory localization changes.
- Changes to capture/recognition support, environment/agent semantics, persistence schema, write authorization, configuration path selection or existing relaunch coverage.
- Commits, staging, remote actions or changes to existing tickets and their acceptance status.
