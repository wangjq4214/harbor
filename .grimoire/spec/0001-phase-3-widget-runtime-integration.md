# Phase 3 Widget Runtime Integration

**Spec ID:** 0001
**Status:** Draft
**Date:** 2026-07-24

## Requirement

Harbor must render the terminal through the main Widget Runtime and present paste confirmation in a separate native window rendered by a second Widget Runtime while preserving existing paste-safety semantics and sharing CPU-side text resources.

## Solution

Create `harbor-text` as the shared CPU text core for font discovery, fallback, glyph rasterization, atlas data, metrics, and text-run caching. `harbor-render` and `harbor-widget` consume this core but retain their own wgpu adapters and GPU resources; `harbor-widget` must not depend on `harbor-render`.

Replace the main-window demo root with a focusable terminal `CustomPaint` node. The node stores an `ExternalDrawId`; during encoding the Runtime flushes its compatible widget batch, applies the node's rect and rectangular clip, and invokes an App-supplied provider. The App provider encodes the existing terminal renderer and receives terminal input only after Widget event propagation has completed.

Keep paste-disposition, raw content, current `InputModes`, and PTY writes in the App. A confirmation request creates or updates a secondary owned winit window. The App owns that window's surface, redraw, encoding, submission, and presentation; the window owns a separate Widget Runtime whose root is the paste-confirmation UI. Both windows share the App's Device and Queue and the `harbor-text` CPU core, but acquire, encode, submit, and present their own surface frames.

The confirmation UI displays escaped preview text without changing the raw text, wraps to its available width, and maintains only a dialog-specific read-only vertical scroll offset. It retains Cancel as the default focus and supports click, `y`, `n`, Escape, Tab / Shift+Tab, Enter, arrows, PageUp / PageDown, and wheel scrolling. While this window exists, the App blocks terminal keyboard input and new paste requests but continues PTY output, terminal rendering, scrollback browsing, and copying.

Every Runtime viewport must receive the window's logical size, physical size, and current scale factor so text placement, layout, and hit testing agree after DPI changes.

### Seams

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Shared text core | `harbor-render` ↔ `harbor-widget` via `harbor-text` | Shared font, raster, atlas-data, metric, and text-run contracts | One CPU text implementation for terminal and Widget consumers |
| External terminal provider | `harbor-widget` Runtime ↔ App / terminal renderer | `ExternalDrawId`, target rect, clip, render-pass context, and deferred input events | Terminal drawing and input inside the main Widget tree without a terminal-crate dependency |
| Confirmation-window host | App ↔ winit / confirmation Widget Runtime | Window lifecycle, secondary surface, shared Device/Queue, scale-factor and input events | Native secondary confirmation window rendered by Widget Runtime |
| Paste confirmation handoff | Selection / paste disposition ↔ App / confirmation UI | Raw paste candidate and current terminal state | Confirm-or-cancel intent; App sends the unchanged raw content using `InputModes` at confirmation time |

## End-to-End Tests

### E2E: Terminal CustomPaint renders and receives input

- **Given:** Harbor has initialized its main window, terminal, and main Widget Runtime.
- **When:** The main window redraws and the terminal CustomPaint node receives focus and a terminal input event.
- **Then:** The existing terminal renderer draws in the CustomPaint rect and clip, the App receives the deferred event, and the existing terminal input path handles it without `harbor-widget` depending on terminal renderer types.

### E2E: Confirmable paste opens a native Widget window

- **Given:** Bracketed paste is disabled and a paste contains a valid newline.
- **When:** The user initiates that paste.
- **Then:** The App opens or focuses a separate owned winit confirmation window rendered by its own Widget Runtime; the main window remains a terminal window rather than drawing an in-window confirmation overlay.

### E2E: Confirm and cancel preserve paste safety

- **Given:** A confirmation window is open for raw paste content.
- **When:** The user confirms after terminal `InputModes` have changed.
- **Then:** The App sends the unchanged raw content with the modes read at confirmation time.

- **Given:** A confirmation window is open.
- **When:** The user cancels by clicking Cancel, pressing `n`, Escape, or Enter while Cancel is focused.
- **Then:** No paste bytes are sent to the PTY and the confirmation window closes.

### E2E: Cross-window modality preserves terminal safety

- **Given:** A confirmation window is open.
- **When:** The user types in the main terminal window or starts another paste.
- **Then:** The App does not forward terminal keyboard input or create a second confirmation request; PTY output, terminal redraw, scrollback browsing, and copying remain available.

### E2E: Preview, focus, and DPI behavior

- **Given:** A confirmation window with control characters and content exceeding its preview height.
- **When:** The user resizes or changes DPI, tabs from Cancel, scrolls the preview, and activates a button with Enter.
- **Then:** Control characters are visibly escaped, wrapped lines and hit regions use the updated scale factor, scrolling remains within bounds, focus traversal is correct, and activation follows the focused button.

## Decisions

### Keep Widget and terminal GPU pipelines independent

- **Choice:** `harbor-widget` owns its instanced quad and text adapter pipelines and consumes shared CPU text data rather than terminal renderer GPU abstractions.
- **Reason:** This preserves the crate boundary while eliminating duplicated CPU text behavior.
- **ADR reference:** [0003-widget-independent-pipeline](../adr/0003-widget-independent-pipeline.md), [0004-widget-dependency-boundary](../adr/0004-widget-dependency-boundary.md)

### Use provider-by-ID for terminal drawing and input

- **Choice:** `CustomPaint` refers to an external identifier, and the App supplies drawing and deferred-input providers at Runtime boundaries.
- **Reason:** The App retains terminal ownership while the Widget Runtime retains layout, clipping, focus, and paint-order control.
- **ADR reference:** [0005-custom-paint-provider-by-id](../adr/0005-custom-paint-provider-by-id.md), [0006-custom-paint-input-provider](../adr/0006-custom-paint-input-provider.md)

### Retain a native confirmation window rendered by Widget Runtime

- **Choice:** The confirmation UI has its own winit window and Widget Runtime instead of a main-window overlay or the legacy `harbor-ui` renderer.
- **Reason:** It provides the requested independent native-window behavior while moving the confirmation UI to the new Runtime.
- **ADR reference:** [0007-retain-separate-paste-confirmation-window](../adr/0007-retain-separate-paste-confirmation-window.md), [0008-widget-runtime-for-confirmation-window](../adr/0008-widget-runtime-for-confirmation-window.md)

### Resolve the window-scoped modality tension before implementation

- **Choice:** Preserve the behavior that an open confirmation blocks terminal keyboard input and new paste requests through an App-level cross-window gate; use `FocusScope` only within the confirmation Runtime for its own controls.
- **Reason:** ADR 0006 describes a modal `FocusScope` intercepting terminal input, which is applicable when both controls share a Runtime. ADRs 0007 and 0008 move confirmation to a separate window and Runtime, so the Runtime-local modal barrier cannot enforce cross-window terminal isolation. A follow-up ADR must supersede or narrow ADR 0006 before planning; this spec treats the newer separate-window decisions as the required feature behavior.
- **ADR reference:** [0006-custom-paint-input-provider](../adr/0006-custom-paint-input-provider.md), [0007-retain-separate-paste-confirmation-window](../adr/0007-retain-separate-paste-confirmation-window.md), [0008-widget-runtime-for-confirmation-window](../adr/0008-widget-runtime-for-confirmation-window.md)

### Superseded Phase 3 overlay requirement

- **Choice:** Do not require terminal and paste-confirmation UI to share one main-window frame.
- **Reason:** ADR 0001's Phase 3 removal of the separate window is superseded by ADR 0007; the main terminal still uses CustomPaint and single-frame composition within its own surface.
- **ADR reference:** [0001-widget-crate-separation](../adr/0001-widget-crate-separation.md), [0007-retain-separate-paste-confirmation-window](../adr/0007-retain-separate-paste-confirmation-window.md)

## Test Plan

- **Integration tests:** Exercise `harbor-text` from both renderers; test SceneItem `Text` and `External` encoding order, batch flush and state restoration; test provider dispatch after event propagation; and test App routing across main and confirmation window IDs.
- **Manual tests:** Open a confirmable multiline paste; verify the secondary window is owned and centered over the main window; confirm and cancel with each supported keyboard and pointer route; change display DPI and resize both windows; verify terminal output and scrollback remain usable while confirmation is open.
- **Performance thresholds:** With no dirty Fiber, animation, or external redraw request, each Runtime requests zero redraws for an idle event-loop cycle. Repeated rendering of an already-cached `(font, glyph)` must perform zero additional CPU rasterizations. Preview rendering must emit glyph instances only for visible wrapped lines.
- **Edge cases:** Bracketed paste enabled; a single-line paste; empty paste; a second paste while confirmation is open; confirmation-window close request; stale external IDs; terminal focus loss; control characters including `\r`; empty trailing lines; content narrower than one character; surface loss, timeout, resize, and DPI change on either window.

## Out of Scope

- A main-window paste-confirmation overlay; it is rejected in favor of an independent native window.
- Reusing the legacy `harbor-ui` renderer for the confirmation window; it would leave Phase 3 Widget text and interaction unexercised.
- A platform-provided native alert dialog; the confirmation content remains Harbor-rendered Widget UI.
- A general-purpose `ScrollView`, images, paths, non-rectangular clipping, complex animations, virtual lists, R-tree hit testing, and time-sliced reconciliation.
- Complex text shaping, bidirectional text, ligatures, selection/caret editing, and a shared GPU glyph atlas across renderer adapters.
- Moving PTY ownership, terminal model ownership, or terminal renderer ownership into `harbor-widget`.

## Future Evolution

- Re-evaluate a shared GPU glyph atlas only if profiling shows duplicate GPU uploads across adapters are material.
- Add a general scroll container only after a second independently justified scrollable Widget use case.
- Add shaping and richer text behavior when a Widget UI requires it and shared text-core APIs can support terminal and UI needs without coupling.
- Generalize external providers for additional App-owned content only after a second concrete `CustomPaint` consumer.
- Revisit cross-window modality and accessibility when Harbor adds platform-native activation, multiple main windows, or accessibility requirements.
