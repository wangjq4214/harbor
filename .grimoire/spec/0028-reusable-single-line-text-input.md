# Reusable Single-Line Text Input

**Spec ID:** 0028
**Status:** Implemented with scoped automated/native smoke evidence; final native acceptance pending.
**Sources:** Approved complete single-line editor and search migration; [ADR 0065](../adr/0065-layered-widget-text-input.md), [0064](../adr/0064-widget-and-application-component-ownership.md), [state/action boundary](../adr/0029-widget-store-state-action-boundary.md). Flutter informs separation, not dependency/API/Material compatibility.

## Requirements
### R1 — Reusable layered control
In harbor-widget provide controller-owned coherent text/caret/selection/composition with programmatic read/write; a decoration-free editing core for input/focus/geometry/horizontal scrolling; a decorated field adding placeholder/border/padding/enabled/read-only without a second editor. Plain core remains usable independently. No Harbor search/session/terminal/config types or product labels/appearance; reuse generic layout/decoration/focus/theme. Public names/signatures/storage remain implementation choices.

### R2 — Coherent editing state and change notifications
Collapsed selection is caret. Programmatic related-state changes are atomic and remove unrelated composition. Document index convention and invalid/out-of-range normalization; bytes/scalars/graphemes/display positions differ.
Expose committed-text changes and submit intent plus state read/replacement. Selection/navigation/preedit are not committed-text notifications. Unrelated rebuilds preserve text/selection/composition/history. No competing editable state to integrate command queues; controller is local editor state, not app reducer/search-result owner.

### R3 — Keyboard editing, clipboard and history
Insert/replace at caret/selection, Backspace/Delete, left/right, Home/End, Shift extension, select-all with R4 boundaries. Standard Windows select-all/copy/cut/paste/undo/redo operate on field text, not terminal selection. Paste inserts/replaces; undo/redo coherently restore replacement/paste/IME committed edits and selection, not transient preedit. Document/test history grouping and programmatic updates.
Clipboard must use actual native readback/result path, not stub/unsupported Read effect. Failure cannot corrupt state, falsely succeed or delete failed-cut text. Late results cannot edit another field or closed/replaced search target. Never log clipboard/input payloads.
Single-line Enter is submit, not newline; pasted content cannot implicitly submit or become PTY input.

### R4 — Unicode-safe editing
Movement/deletion/selection never split user-perceived characters: Latin/CJK, combining sequences, supplementary characters, flags and ZWJ emoji. Preserve valid source independent of terminal cell/text-unit capacity rules. Paint/hit testing/extracted selection agree; narrow fields/unsupported glyphs cannot corrupt text. No promise every font supports every glyph or terminal renderer redesign.

### R5 — Pointer selection and visible editing geometry
Click maps to valid caret; drag selects through horizontal scrolling with existing capture release/cancellation, no stuck selection; Shift extension agrees with pointer-created ranges.
Real caret/selection primitives, not cursor characters; placeholder/caret are not source/copy text. Scroll long content to active editing position and clip text/caret/selection from neighboring controls. Padding/scroll/resize/DPI must align paint/hit/IME geometry using rendered advances, not scalar=fixed-cell assumptions.

### R6 — IME, focus and routing
Chinese preedit is transient with reported caret/selection meaning; commit inserts/replaces exactly once, cancel/focus loss clears without query commit. Candidate confirmation cannot also submit/navigate.
Enable native IME only for eligible focused editor and position candidates at current scrolled/resized/DPI caret, without terminal external-draw position dependency.
Use existing focus handles/scopes and Tab/Shift+Tab. Local field editing chords reach field, not terminal handlers; consumed events queue no terminal command/PTY bytes. Preserve unrelated application precedence and restore terminal commands on terminal focus, under [ADR 0040](../adr/0040-application-command-dispatch-and-keybinding-precedence.md)/[native host 0031](../adr/0031-widget-winit-adapter-owns-native-host-infrastructure.md).

### R7 — Ordinary field presentation and modes
Composable placeholder/border/padding; placeholder only for empty content, never state. Read-only prevents user mutation but permits viewing/selection/copy. Disabled accepts no editing. These are functional modes, not just colors; no application permission/search policy or fixed styling/dimensions.

### R8 — Migrate the application search box
Replace manual SearchEntry with the reusable field; remove append/pop/preedit-string/cursor-character editor, no parallel editor/writer. Application owns visibility/source/status/query acceptance/SearchChanged/previous-next-close/execution under [0027](./0027-revisioned-terminal-read-model-and-literal-search.md):
- Open focuses input; close cancels/restores terminal focus; source-session change clears/closes old search.
- Accepted committed edits including undo/redo travel the typed command path exactly once, no feedback loops; preedit never searched.
- Enter/Shift+Enter next/previous, Escape close outside IME; existing buttons/status retained. IME confirmation obeys R6.
- Application limit remains 128 UTF-8 bytes and rejects controls. Validate final text after selection replacement; reject whole edit preserving prior query, never truncate UTF-8/graphemes.
- Preserve empty cancellation, validated highlights/navigation, result/batch limits and stale session/results rejection. Painting/caret/selection/decoration does not restart history search.
Generic acceptance/callbacks may support policy, but widgets import no MAX_QUERY_BYTES/AppCommand/TabId/search types. No matching/language/global-search redesign.

## End-to-End Verification
| ID | Scenario → required observation |
| --- | --- |
| E1 | Non-terminal plain core/decorated field, programmatic text/selection and unrelated rebuild → shared generic editor/coherent state, no reset or decoration-induced text change. |
| E2 | Edit middle of abcd, Shift-select/replace, delete/move/Home/End/select-all/undo/redo → correct positions/coherent history, no navigation-only query change. |
| E3 | Mixed Latin/CJK/combining/supplementary/flag/family ZWJ move/select/replace/delete/undo → intact graphemes/source and copied/displayed selection agreement. |
| E4 | Narrow padded long field, click/drag/scroll/resize/DPI, cancel/release outside → usable aligned caret/highlight/clipping and capture termination. |
| E5 | Real Windows selected copy/cut/paste, backend failure and stale closed/replaced target → eligible field only; no lost failed cut, false success, terminal selection/PTY effect. |
| E6 | Chinese composition in middle/over selection, confirm/cancel/focus loss after scroll/resize/DPI → transient preedit, positioned candidate, once-only commit, no accidental submit/navigation. |
| E7 | Empty/read-only/disabled fields and editing/clipboard attempts → placeholder not copied, read-only selection/copy but no mutation, disabled no editing. |
| E8 | Search middle edit/replacement/paste/undo, key/button navigation/close → one editor/typed query path, working status/highlights, close cancels/restores focus. |
| E9 | Multibyte selection replacement at 128-byte boundary, overflow/control edit, empty query → final-length enforcement/atomic rejection/whole graphemes, empty cancels, preedit not searched. |
| E10 | Terminal selection then field copy/cut/paste/focus traversal/return, switch/close and late clipboard/IME, supported root reload → local routing/no PTY leakage, restored terminal commands, no revived old session, HMR safety/state retention. |

## Evidence and Remaining Acceptance
[Input guide](../../docs/widget-input.md) owns delivered API/index/history policies. [Verification](../../docs/widget-input-verification.md) records deterministic controller/runtime/workspace/native-adapter coverage and a limited Latin/selection/native-clipboard smoke preceding final refinements, not final integrated acceptance. Actual controller/editor/field and search migration exist; historical original append-only limitations are not current requirements.
Use deterministic Unicode/history/atomic-rejection tests, focus/capture/clipboard eligibility/IME sequence integration, render-advance geometry/clipping tests and the real application command path. Under [validation policy](../../docs/validation.md), run standard fmt/clippy/workspace/doc gates and record revision/dirty scope/environment/steps/expected-observed/artifacts/exclusions/PASS/FAIL/NOT RUN/BLOCKED without payload secrets.
Still NOT RUN on final integrated binary: Chinese native candidate/cancel/commit ordering, mixed-Unicode visual/caret, real resize/DPI/drag, supported live DLL/HMR replacement and extended field/terminal coexistence/lifecycle. Headless geometry or HMR tests do not substitute. No new performance threshold/improvement claimed; ADR Testing is not Completed. Preserve separate terminal paste/OSC authorization and old-root teardown/Host state retention.

## Out of Scope
Multiline/password/rich text/form validation, Flutter compatibility, new crate/bus, terminal renderer replacement, native-platform expansion or search-engine/cross-session redesign. Initial spec-only refinement was later followed by implementation authorization; no ticket decomposition required.
