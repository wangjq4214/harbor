# Phase 3 Widget Runtime Integration

**Source:** [Spec: 0001-phase-3-widget-runtime-integration.md](../../spec/0001-phase-3-widget-runtime-integration.md)
**Ticket folder:** `.grimoire/ticket/0001-phase-3-widget-runtime-integration/`

## Overview

These tickets deliver the revised Phase 3: the terminal becomes App-provided `CustomPaint` in the main Widget Runtime, while paste confirmation becomes a separate native winit window rendered by a second Widget Runtime. The App retains terminal, PTY, surface, and event-stream ownership; `harbor-text` supplies shared CPU text behavior. Cross-window input isolation follows ADR 0009.

## Layers

1. **`harbor-text`** — shared CPU font, glyph-raster, atlas-data, metric, and text-run concerns.
2. **`harbor-widget`** — declarative View/Fiber, Scene, renderer, input, and `CustomPaint` behavior.
3. **`harbor-render`** — existing terminal renderer and its adapter to shared text data.
4. **App host** — `src/app.rs` winit lifecycle, surfaces, PTY, and event orchestration.
5. **Verification** — crate, integration, and manual GPU/DPI validation.

Every ticket covers all confirmed layers; pre-refactoring may state that a layer has no user-visible work.

## Dependency Graph

### Blocking relationships

| Ticket | Blocks                     | Reason                                                                                                              |
| ------ | -------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| T0001  | T0002, T0003, T0004, T0005 | All Phase 3 consumers require the shared CPU text contract and migrated terminal adapter.                           |
| T0002  | T0003                      | The confirmation host extends the App/Runtime lifecycle and provider contracts established by terminal CustomPaint. |
| T0003  | T0004                      | Confirmation actions require the native Widget window and App-level cross-window gate.                              |
| T0004  | T0005                      | Preview completion extends the functional confirmation UI and its event path.                                       |
| T0005  | —                          | Final slice.                                                                                                        |

### Parallel groups

No safe parallel groups exist. Every user-visible slice changes `src/app.rs` and/or the same Widget Runtime contracts; parallel work would create file and contract conflicts.

## Recommended Order

1. T0001 — Shared Text Foundation
2. T0002 — Terminal CustomPaint
3. T0003 — Native Cancellation Window
4. T0004 — Safe Paste Confirmation
5. T0005 — Preview and DPI Completion

## Ticket Index

| Ticket ID | File                                                                         | Title                      | Summary                                                                             |
| --------- | ---------------------------------------------------------------------------- | -------------------------- | ----------------------------------------------------------------------------------- |
| T0001     | [T0001-shared-text-foundation.md](./T0001-shared-text-foundation.md)         | Shared Text Foundation     | Establish shared CPU text types and migrate the terminal adapter.                   |
| T0002     | [T0002-terminal-custom-paint.md](./T0002-terminal-custom-paint.md)           | Terminal CustomPaint       | Render and route the terminal through the main Widget Runtime.                      |
| T0003     | [T0003-native-cancellation-window.md](./T0003-native-cancellation-window.md) | Native Cancellation Window | Replace the legacy dialog with a Widget-rendered native window that safely cancels. |
| T0004     | [T0004-safe-paste-confirmation.md](./T0004-safe-paste-confirmation.md)       | Safe Paste Confirmation    | Send raw content on explicit confirmation using current input modes.                |
| T0005     | [T0005-preview-and-dpi-completion.md](./T0005-preview-and-dpi-completion.md) | Preview and DPI Completion | Finish preview scrolling, keyboard behavior, and scale-factor correctness.          |
