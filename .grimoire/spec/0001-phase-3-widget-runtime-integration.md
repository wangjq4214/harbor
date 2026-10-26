# Phase 3 Widget Runtime Integration

**Spec ID:** 0001
**Status:** Accepted (owner-confirmed)
**Date:** 2026-07-24

**Architecture evolution:** The paste-safety and confirmation behavior below remains the product contract. Rendering/provider and native-host ownership subsequently evolved through Specs 0002, 0016, and 0012 and ADRs 0011 and 0031; Spec 0016 preserves the retired Spec 0005 bridge-migration boundary.

## Requirement

Harbor must render the terminal through the main Widget Runtime and present paste confirmation in a separate native window rendered by a second Widget Runtime while preserving existing paste-safety semantics and sharing CPU-side text resources.

## Solution

Create `harbor-text` as the shared CPU text core for font discovery, fallback, glyph rasterization, atlas data, metrics, and text-run caching. `harbor-render` and `harbor-widget` consume this core but retain their own wgpu adapters and GPU resources; `harbor-widget` must not depend on `harbor-render`.

Replace the main-window demo root with a focusable terminal `CustomPaint` node. The node stores an `ExternalDrawId`; during encoding the Runtime flushes its compatible widget batch, applies the node's rect and rectangular clip, and invokes an App-supplied provider. The App provider encodes the existing terminal renderer and receives terminal input only after Widget event propagation has completed.

Keep paste-disposition, raw content, current `InputModes`, and PTY writes in the App. A confirmation request creates or updates a secondary owned winit window. The App owns that window's surface, redraw, encoding, submission, and presentation; the window owns a separate Widget Runtime whose root is the paste-confirmation UI. Both windows share the App's Device and Queue and the `harbor-text` CPU core, but acquire, encode, submit, and present their own surface frames.

The confirmation UI displays escaped preview text without changing the raw text, wraps to its available width, and maintains only a dialog-specific read-only vertical scroll offset. It retains Cancel as the default focus and supports click, `y`, `n`, Escape, Tab / Shift+Tab, Enter, arrows, PageUp / PageDown, and wheel scrolling. While this window exists, the App blocks terminal keyboard input and new paste requests but continues PTY output, terminal rendering, scrollback browsing, and copying.

Every Runtime viewport must receive the window's logical size, physical size, and current scale factor so text placement, layout, and hit testing agree after DPI changes.

## Decisions

Paste safety and the separate-window gate follow [ADR 0007](../adr/0007-retain-separate-paste-confirmation-window.md) and [ADR 0009](../adr/0009-app-cross-window-input-gate.md). Current native-host ownership follows [Spec 0012](./0012-widget-winit-native-host-consolidation.md); the earlier App-owned surface/presentation description above is historical.

## Test Plan

A valid newline with bracketed paste disabled must open/focus the owned confirmation window. Confirm uses unchanged raw bytes and modes current at confirmation time; cancel, native close, n/Escape or Enter on default Cancel write no PTY bytes.

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
