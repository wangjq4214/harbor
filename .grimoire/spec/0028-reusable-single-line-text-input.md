# Reusable Single-Line Text Input

**Spec ID:** 0028
**Status:** Draft
**Date:** 2026-10-16
**Sources:** The user's request to establish a general input component informed by Flutter, and approval of the proposed controller/editing-core/decorated-field boundary, complete single-line editing and migration of the existing search box. The selected route is spec only; no tickets or implementation are authorized by this artifact.
**Architecture:** [ADR 0065](../adr/0065-layered-widget-text-input.md), applying [ADR 0064](../adr/0064-widget-and-application-component-ownership.md).

## Requirements

### R1 — Reusable layered control

Provide a reusable single-line input in `harbor-widget`, usable in a non-terminal application without application, terminal, session or configuration models. Separate:

- A controller holding coherent text, caret/selection and IME composition state, with programmatic read/write.
- A decoration-free editing core handling input, focus, editing geometry and horizontal scrolling.
- A decorated field adding placeholder, border, padding and disabled/read-only states without implementing a second editor.

`TextEditingController`, `EditableText` and `TextField` are working names for these roles; final Rust signatures and module names are not fixed. Consumers must be able to use the editing core without the field's decoration. The field must not hard-code Harbor search labels, commands or appearance. Reuse existing layout, decoration, focus and theme mechanisms where applicable; Flutter is a separation-of-responsibility reference, not a dependency or a Material styling requirement.

Source: approved proposal and ADR 0065.

### R2 — Coherent editing state and change notifications

Text, selection/caret and composition must describe one valid editing state. A collapsed selection represents a caret. Programmatic changes must support updating related state coherently without exposing an invalid intermediate selection or retaining composition over unrelated text. Document the public index convention and how invalid/out-of-range positions are handled; bytes, Unicode scalars, graphemes and displayed positions must not be silently interchangeable.

Expose committed-text change and submit intent to consumers, and allow consumers to read or replace editing state. Navigation or selection alone must not masquerade as a committed-text change. IME preedit changes must not become committed-text notifications. A rebuild caused by unrelated application state must not reset the active field's text, selection, composition or edit history. Consumers must not need competing editable copies of the same state to integrate with the application command queue.

These are controller/editing guarantees, not a transfer of application reducers or search result ownership into the widget library. Exact callback spelling, state storage and history grouping are implementation choices.

Source: approved controller/programmatic-read-write/change-callback scope; ADR 0065; [state/action boundary](../adr/0029-widget-store-state-action-boundary.md).

### R3 — Keyboard editing, clipboard and history

Support insertion at the caret, replacement of selected text, Backspace, Delete, left/right movement, Home/End, Shift-based selection extension and select-all. Deletion and movement must honor the Unicode editing boundaries in R4. Text entry must no longer be restricted to appending at the end.

Provide the standard Windows desktop shortcuts for select-all, copy, cut, paste, undo and redo. Copy/cut operate on the field's selected text, not a terminal selection. Paste replaces the selection or inserts at the caret. Undo/redo must restore committed editing changes coherently, including selection replacement, paste and IME commits; transient preedit updates are not separately committed query edits. The implementation must document and test its edit-history grouping and programmatic-update interaction without claiming Flutter API compatibility.

Clipboard operations must work through the actual native host path, including readback for paste, rather than only through a test stub or the current unsupported `ClipboardEffect::Read` path. Clipboard errors must not corrupt editing state or report a successful paste; a failed cut must not silently lose text. A result must not edit another field or a closed/replaced search session if its target is no longer eligible. Do not log clipboard or input payloads as part of error handling.

The generic field is single-line: Enter is submit intent rather than insertion of a line break. Clipboard content must not implicitly submit or become terminal input. The application-specific control-character policy for search remains R8; this spec does not introduce a multiline editor.

Source: approved keyboard/clipboard/undo-redo scope; actual clipboard host gap; existing payload-safe clipboard error handling.

### R4 — Unicode-safe editing

Caret movement, selection endpoints, Backspace and Delete must not split a user-perceived character, including combining marks and emoji sequences. Cover ordinary Latin text, CJK, combining text, supplementary characters, regional-indicator flags and ZWJ emoji sequences. Editing must preserve valid source text and must not apply terminal-cell indexing or terminal text-unit capacity rules to a generic field.

Painting, hit testing and selected-text extraction must agree on positions. A narrow field or unsupported glyph presentation does not authorize corruption of the source text. This requirement concerns editing integrity and consistent geometry, not a promise that every font supplies every glyph or that terminal glyph rendering must be redesigned.

Source: approved Unicode-safe editing scope and the generic/non-terminal boundary.

### R5 — Pointer selection and visible editing geometry

A pointer click places the caret at the corresponding valid editing position. Dragging selects text, including in a horizontally scrolled field, using the existing capture lifecycle so selection does not remain stuck after release or cancellation. Shift-based selection must remain coherent with pointer-created selections.

Paint a real caret and selection highlight rather than append a cursor character to the string. Neither caret nor placeholder is source text, selectable text or clipboard content. Long content scrolls horizontally to keep the active editing position usable; clipping prevents text/caret/selection from painting over adjacent controls. Scrolling, padding, resize and DPI changes must keep drawing, hit testing and IME positioning aligned. Derive editing positions consistently with rendered advances, not by assuming one Unicode scalar equals one fixed-width cell.

Source: approved pointer/caret/selection/horizontal-scroll scope; current `SearchEntry` and character-layout limitations.

### R6 — IME, focus and routing

Support Chinese IME preedit and commit through the existing platform-independent input path. Display preedit separately from committed text and maintain its reported caret/selection meaning. A commit inserts or replaces at the active editing position exactly once. Cancellation and focus loss must not leave stale preedit or commit it as a search query. Confirming an IME candidate must not also submit the field or navigate search results from the same interaction.

Enable/disable native IME according to the eligible focused editor and position its candidate window at current editing geometry, including after horizontal scrolling, resize or DPI changes. The generic editor must not depend on the external terminal-draw IME callback to supply its own position.

Use existing focus handles/scopes and Tab/Shift+Tab traversal. Field editing shortcuts must reach the focused field rather than be stolen by terminal copy/paste handlers; consumed editing events must not enqueue terminal commands or emit PTY bytes. Retain unrelated application shortcut behavior and restore established terminal command behavior when the terminal has focus. Do not globally remove application shortcut precedence as a shortcut to solving focused-field editing.

Source: approved IME/focus scope; [command boundary](../adr/0040-application-command-dispatch-and-keybinding-precedence.md); [native host ownership](../adr/0031-widget-winit-adapter-owns-native-host-infrastructure.md).

### R7 — Ordinary field presentation and modes

Provide placeholder, border and padding configuration in the decorated field. Placeholder appears for empty content without becoming editing state. Disabled/read-only behavior must be functional, not merely a color change: read-only prevents user text mutation while permitting viewing/selection/copy; disabled does not accept user editing. Expose these states without embedding application permission or search availability decisions in the generic editor.

Appearance choices remain composable with existing widget styling; do not mandate Material styling or Harbor-specific colors, dimensions or labels.

Source: approved decorated-field proposal.

### R8 — Migrate the application search box

Replace the manual `SearchEntry` editor in `crates/harbor-app/src/workspace.rs` with the reusable input. The application continues to own search visibility/source session, status, query acceptance, `SearchChanged`, previous/next/close commands and search execution. Remove the superseded append/pop, preedit-string and cursor-character editing path; do not maintain two independent editors or two competing editing-state writers.

Preserve the current literal-search integration from [spec 0027](./0027-revisioned-terminal-read-model-and-literal-search.md):

- Opening search focuses its input; closing it cancels search and restores terminal focus. Changing source session clears/closes the old search rather than applying its input/results to another session.
- Search observes accepted committed text, not IME preedit. Accepted text edits, including undo/redo, update search through the existing typed command path without duplicate delivery or feedback loops.
- Keep Enter/Shift+Enter for next/previous, Escape for close outside active IME handling, and the existing Previous/Next/Close buttons and status display. Candidate confirmation follows R6.
- Keep the application-owned `MAX_QUERY_BYTES` limit of 128 UTF-8 bytes and rejection of control-character input. Enforce the limit against the resulting text after selection replacement, not just old length plus inserted length. Rejected edits leave the previously accepted query valid and must not split UTF-8 or a grapheme to fit the limit.
- Preserve empty-query cancellation, validated highlights/navigation, result limits, bounded search work and stale-result/session rejection. Field painting, caret movement, selection changes or decoration changes must not restart terminal-history searches.

The widget may expose generic acceptance/notification mechanisms, but must not import `MAX_QUERY_BYTES`, `AppCommand`, `TabId` or search engine types. This is an input migration, not a search algorithm, query-language or cross-session-search redesign.

Source: approved search migration, current workspace/search contracts and spec 0027.

## Solution and Integration Contracts

Implement the three approved roles within the existing widget/application/host boundaries. The following seams are required for correctness; concrete request IDs, controller storage, history representation, layout data structures and helper modules are not prescribed.

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Editing state | Application/controller ↔ editing core | Coherent text, valid selection and transient composition; programmatic replacements | Observable current state and committed-text/submit intent without duplicate editable state |
| Field presentation | Decorated field → editing core | Placeholder/style, padding, enabled/read-only configuration and available bounds | The same editor with independently composable appearance, not another input engine |
| Input routing | Runtime/focus/actions ↔ focused editor | Keyboard, pointer, capture, focus and IME events with consumed/pass-through semantics | Local editing and focus traversal without terminal-command interception or PTY leakage |
| Editing geometry | Text rendering/layout ↔ editor | Render-consistent text advances and valid Unicode editing boundaries in current bounds/scale | Caret/selection positions, pointer mapping, horizontal scrolling and clipping that agree |
| Clipboard | Editor ↔ runtime/native host | Copy/cut payload or paste request associated with an eligible editor; success/failure/readback | Native clipboard behavior, safe delivery and failure handling without terminal-only dependencies |
| Native IME | Focused editor ↔ runtime/winit host | Allowed state, current caret geometry, preedit/commit/cancel | Positioned native composition and exactly-once committed editing |
| Search consumer | Reusable editor → application workspace/command/search | Accepted committed query and submit/navigation intent for the live search source | Existing bounded search, highlights/status/navigation and application-owned query policy |

The clipboard read/result path and general editor IME positioning need actual support. Existing event/effect type names alone are not proof these contracts are implemented. Preserve the current host ownership, separate terminal paste/OSC clipboard authorization flows and HMR teardown/business-state retention rules.

## End-to-End Verification

| ID | Given / when | Required observation |
| --- | --- | --- |
| E1 — Non-terminal reuse | Build a widget-only root with a plain editing core and a separately decorated field; enter and programmatically replace text/selection; rebuild unrelated UI | Both use the shared editing behavior without Harbor models. Programmatic state stays coherent; unrelated rebuilds do not reset editing. Placeholder/style changes do not mutate text. |
| E2 — Keyboard and history | In `abcd`, move between `b` and `c`, insert, extend a selection, replace it, Backspace/Delete, Home/End, select all, undo and redo | Text changes at the intended position, selection and caret remain valid, and history restores edits coherently. Selection/navigation alone does not report a query edit. |
| E3 — Unicode | Use mixed Latin/CJK, `e` plus combining acute, a supplementary emoji, a flag and a family ZWJ sequence; move, select, replace, delete and undo | No operation leaves an interior grapheme boundary or malformed text. Copied selected text and displayed selection agree. |
| E4 — Pointer/viewport | Click and drag in long text within a narrow padded field; scroll, resize, change scale and cancel/release a drag outside the field | Caret/highlight follow valid positions, active editing stays usable, clipping contains paint, and capture terminates correctly. Coordinate mapping remains aligned. |
| E5 — Native clipboard | With a selected range, copy/cut/paste using the real Windows clipboard, then repeat with a backend failure and a paste result whose target has been closed/replaced | Only the eligible field is edited; copy/paste use its text; errors do not corrupt it or lose a failed cut. No terminal selection/PTY operation occurs. |
| E6 — Native IME | Compose Chinese in the middle of text or over a selection, confirm a candidate, cancel another composition, then change focus; repeat after scroll/resize/DPI changes | Preedit is transient, candidate window follows the field caret, commit edits once, cancellation/focus loss leave no stale preedit, and candidate confirmation does not also navigate search. |
| E7 — Field modes | Show an empty placeholder field, a populated read-only field and a disabled field; try input, paste, cut and selection/copy | Placeholder is not copied; read-only prevents mutations but permits selection/copy; disabled accepts no editing. Decorations remain independent of source text. |
| E8 — Search migration | Open current-session search, edit the middle of its query, replace a selection, paste and undo; invoke next/previous/close by keys and buttons | One reusable editor drives accepted committed queries through the existing command path; status/highlights/navigation remain functional; closing cancels and restores terminal focus. |
| E9 — Search policy | At the 128-byte boundary, replace a selected range with multibyte text; attempt an over-limit or control-character insertion; then empty the query | Limits use final UTF-8 byte length; valid replacements work, rejected edits preserve accepted text without partial graphemes, and empty query cancels results. Preedit is not searched. |
| E10 — Focus and lifecycle regression | Keep a terminal selection, focus the search input and copy/cut/paste there; traverse focus, return to terminal, switch/close sessions and deliver late clipboard/IME events; exercise supported root reload | Field shortcuts affect the field only. No consumed search/edit input reaches PTY. Terminal shortcuts work again with terminal focus. Old-session events do not revive/edit a new search; existing HMR teardown and retained business state guarantees hold. |

## Decisions and Traceability

- The user approved complete single-line editing plus search migration, not a display-only wrapper or a wholesale Flutter implementation. R1–R8 express that agreed scope and its integration guarantees.
- Controller/core/field ownership follows ADR 0065 and the existing reusable-widget criterion, rather than moving search business logic downward. Local editor state is not an application reducer under ADR 0029.
- The source baseline is `e3d8238a6c010b192944885a5e5d36913f79ca3c`. Inspected surfaces include [current search entry/composition](../../crates/harbor-app/src/workspace.rs), [search execution](../../crates/harbor-app/src/search.rs), [input events](../../crates/harbor-widget/src/input/event.rs), [event context](../../crates/harbor-widget/src/input/event_ctx.rs), [effects](../../crates/harbor-widget/src/effects.rs), [native clipboard adapter](../../crates/harbor-widget/src/winit/effects.rs) and [widget text layout](../../crates/harbor-widget/src/text.rs). Source observations are not runtime acceptance.
- External reference: Flutter's [EditableText](https://api.flutter.dev/flutter/widgets/EditableText-class.html), [TextField](https://api.flutter.dev/flutter/material/TextField-class.html), [TextEditingController selection](https://api.flutter.dev/flutter/widgets/TextEditingController/selection.html) and [text-field change cookbook](https://docs.flutter.dev/cookbook/forms/text-field-changes). These inform layering and coherent editing state, not an exhaustive compatibility claim.
- No final Rust signatures, new crate, global event bus, replacement of the terminal renderer, fixed performance thresholds or native-platform support expansion is assumed. Internal algorithms may vary while satisfying the observable contracts. New material scope/architecture decisions must return for clarification rather than be invented downstream.

## Test Plan and Evidence

Use deterministic editing/controller tests for Unicode boundaries, selection replacement, history and accepted/rejected updates; runtime integration tests for focus/capture, shortcut routing, clipboard completion eligibility and IME event sequencing; and geometry/rendering tests for caret/highlight/hit-test agreement and clipping. Exercise the real application search command path in addition to an isolated editor harness.

Under [validation policy](../../docs/validation.md), implementation acceptance needs Windows runtime evidence for clipboard, IME/candidate placement, focus, pointer selection, resize/DPI and field-versus-terminal routing. Include supported HMR regression checks and distinguish unavailable cases. Record revision/dirty paths, environment, exact steps, expected/observed result, artifacts and PASS / FAIL / NOT RUN / BLOCKED honestly. Do not capture unrelated terminal or clipboard secrets.

At implementation boundaries run the standard formatting, clippy, workspace-test and documentation gates from that policy. For this documentation-only refinement run the documentation scripts and inspect changed links/claims. No product build, tests, native UI or QA acceptance is established by writing this spec.

## Out of Scope

Multiline editing, password fields, rich text and form validation are deferred as explicitly approved. Search matching semantics, cross-session search and search-engine redesign remain outside this input migration. No implementation plan or ticket decomposition is produced at the selected spec-only endpoint.
