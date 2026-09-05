# Widget Decoration and Terminal Chrome

**Source:** [Spec: 0009-widget-decoration-and-terminal-chrome.md](../../spec/0009-widget-decoration-and-terminal-chrome.md), refined by conversation to preserve the implemented 2dp terminal inset
**Ticket folder:** `.grimoire/ticket/0009-widget-decoration-and-terminal-chrome/`

## Overview

These tickets add reusable Flutter-style box decoration to `harbor-widget`, carry decoration and rounded clipping through the retained scene and GPU frame encoder, and apply the result to the main Terminal without changing the Terminal rendering boundary. The product result is a 12dp anti-aliased terminal radius and layered outer shadow while preserving Acrylic, input, scheduling, layout allocation, and the currently implemented 2dp root inset. The 2dp inset is an explicit conversation-level correction to the source spec's stale 16dp description.

## Layers

The project's architectural layers confirmed during decomposition are:

1. **Widget API and Layout** — public widget/value contracts, logical-pixel geometry, and layout-neutral decoration.
2. **Fiber and Retained Scene** — paint phases, stable SceneItem identity, clip propagation, and hit testing.
3. **Widget Renderer and Frame Encoder** — GPU preparation, paint-ordered encoding, shadows, rounded shapes, and external-draw clipping.
4. **Runtime Host and Terminal Bridge** — application root composition and the existing `CustomPaint` Terminal integration boundary.
5. **Verification** — unit, integration, GPU-contract, interaction, performance, and manual visual evidence.

Every ticket lists all five layers; pre-refactoring and consuming slices explicitly justify layers with no direct code changes.

## Dependency Graph

### Blocking relationships

| Ticket | Blocks | Reason |
| --- | --- | --- |
| T0001 | T0002, T0003, T0004, T0005, T0006 | Every slice consumes the shared decoration values, validation, paint-phase, or clip contracts. |
| T0002 | T0003 | Shadow rendering extends the concrete `DecoratedBox` scene and renderer path established by fill and border. |
| T0003 | T0004, T0006 | Rounded clipping follows stabilization of shared decoration/renderer files; the Terminal preset requires working shadows. |
| T0004 | T0005 | External-draw clipping consumes the rounded clip stack and hit/paint semantics established for normal widget content. |
| T0005 | T0006 | The product preset cannot safely wrap Terminal until `Primitive::External` obeys rounded clipping. |
| T0006 | — | Final product composition slice. |

### Parallel groups

No tickets are declared parallel. T0002 through T0005 modify shared decoration, Scene, renderer, or frame-encoding contracts, and T0006 consumes all resulting behavior; sequential work avoids contract and file conflicts.

## Recommended Order

1. T0001 — Decoration Foundation (pre-refactoring)
2. T0002 — DecoratedBox Fill and Border
3. T0003 — Layered Outer Shadows
4. T0004 — Rounded Widget Clipping and Hit Testing
5. T0005 — Rounded CustomPaint Clipping
6. T0006 — Terminal Decoration Preset

## Ticket Index

| Ticket ID | File | Title | Summary |
| --- | --- | --- | --- |
| T0001 | [T0001-decoration-foundation.md](./T0001-decoration-foundation.md) | Decoration Foundation | Defines shared decoration values, validation, paint phases, and clip contracts. |
| T0002 | [T0002-decorated-box-fill-and-border.md](./T0002-decorated-box-fill-and-border.md) | DecoratedBox Fill and Border | Renders layout-neutral color, uniform border, and per-corner radii around a normal child. |
| T0003 | [T0003-layered-outer-shadows.md](./T0003-layered-outer-shadows.md) | Layered Outer Shadows | Renders ordered outer shadows with offset, blur, spread, and alpha. |
| T0004 | [T0004-rounded-widget-clipping-and-hit-testing.md](./T0004-rounded-widget-clipping-and-hit-testing.md) | Rounded Widget Clipping and Hit Testing | Applies hard-edge and anti-aliased rounded clips consistently to normal child paint and pointer targeting. |
| T0005 | [T0005-rounded-custom-paint-clipping.md](./T0005-rounded-custom-paint-clipping.md) | Rounded CustomPaint Clipping | Makes external Terminal-style draws obey the same rounded child clip without changing provider contracts. |
| T0006 | [T0006-terminal-decoration-preset.md](./T0006-terminal-decoration-preset.md) | Terminal Decoration Preset | Applies the confirmed terminal radius and shadow while preserving the actual 2dp inset and Acrylic behavior. |
