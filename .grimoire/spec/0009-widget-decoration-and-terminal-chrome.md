# Widget Decoration and Terminal Chrome

**Spec ID:** 0009
**Status:** In Progress
**Date:** 2026-08-26

## Requirement

Harbor must provide Flutter-style, reusable widget box decoration and use it to render the main terminal with anti-aliased rounded corners and an outer shadow while preserving layout, input, external-draw, and Acrylic behavior.

## Solution

`harbor-widget` exposes a general-purpose `DecoratedBox` wrapper with immutable value types and fluent builders modeled after Flutter: `BoxDecoration`, `BorderRadius`, `Border`, `BoxShadow`, and `ClipBehavior`. `BoxDecoration` supports an optional color, one uniform border, uniform or per-corner radii, and an ordered list of outer shadows. Values use logical pixels and do not alter measurement or layout bounds.

Decoration paints in the stable order shadow → background → child → border. Shadow lists paint in list order from back to front, with the first item at the bottom. A decoration with no color emits no background fill. Radius, border width, and blur radius are finite and non-negative; spread radius may be negative; non-finite values are invalid.

`ClipBehavior` supports `None`, `HardEdge`, and `AntiAlias`, defaulting to `None`. When clipping is enabled, the rounded shape constrains child painting and hit testing but does not clip the box's own shadows. Ancestor clips remain authoritative. Rounded clipping applies to normal widget primitives and `Primitive::External`, so the Terminal continues to render through `CustomPaint` rather than gaining terminal-specific styling or rendering APIs.

The main Runtime Host composes `TerminalWidgetBridge` inside `DecoratedBox` within the existing 16dp Terminal Window Inset. The product preset uses a 12dp radius, 25%-opaque black outer shadow, `(0dp, 4dp)` offset, 12dp blur, zero spread, no fill, and `ClipBehavior::AntiAlias`. The absent fill preserves Default Background Cell transparency and the existing Windows Acrylic backdrop.

### Seams

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Terminal decoration composition | Runtime Host → `harbor-widget` | Root `TerminalWidgetBridge`, existing 16dp inset, and the product decoration preset | A reusable `DecoratedBox` composition without Terminal-specific visual API |
| Rounded external-draw clipping | `harbor-widget` frame encoding ↔ `CustomPaint` / Terminal external draw | Paint-ordered `Primitive::External`, allocation geometry, ancestor clips, and selected `ClipBehavior` | Rounded child clip for terminal pixels while preserving external draw identity, scheduling, and render-pass ownership |

## End-to-End Tests

### E2E: Main terminal displays the product decoration

- **Given:** Harbor opens the main terminal window at a drawable non-zero size.
- **When:** The first terminal frame is presented.
- **Then:** The terminal has a 12dp anti-aliased rounded outline and a 25%-opaque black shadow offset 4dp downward with 12dp blur, while its measured allocation and 16dp window inset remain unchanged.

### E2E: Rounded clipping applies to Terminal CustomPaint

- **Given:** Terminal content or a scrollbar reaches a corner of the terminal allocation.
- **When:** The terminal is rendered through `TerminalWidgetBridge` and `CustomPaint`.
- **Then:** Pixels outside the 12dp rounded child shape are not presented, the shadow remains visible outside that shape, and ancestor clipping can still truncate overflow.

### E2E: Rounded clipping governs pointer targeting

- **Given:** `ClipBehavior::AntiAlias` is active on the decorated terminal.
- **When:** A pointer event occurs inside the terminal's rectangular allocation but outside its rounded shape.
- **Then:** The terminal is not selected as the hit target; pointer behavior inside the rounded shape is unchanged.

### E2E: Acrylic remains visible

- **Given:** The Windows main window uses Acrylic and the terminal contains Default Background Cells.
- **When:** The decorated terminal frame is presented.
- **Then:** Empty terminal pixels remain unfilled and reveal Acrylic, while explicit cell backgrounds, inverse cells, the border if configured, and the outer shadow remain readable.

### E2E: Decoration remains reusable and optional

- **Given:** A non-terminal child is wrapped in `DecoratedBox`, and another child is not decorated.
- **When:** Both are laid out, painted, and hit-tested.
- **Then:** The decorated child follows its configured color, border, radius, shadows, and clip behavior without changing size; the undecorated child preserves existing appearance and behavior.

### E2E: Degenerate and invalid decoration values are bounded

- **Given:** A decoration uses zero radius, zero blur, negative spread that collapses the shadow bounds, or invalid non-finite input.
- **When:** The scene is prepared and rendered.
- **Then:** Zero or collapsed effects produce no spurious pixels, invalid input is rejected by the public value contract, and rendering does not panic or submit invalid GPU geometry.

## Decisions

### Use a generic DecoratedBox rather than Terminal-specific properties

- **Choice:** Decoration belongs to `harbor-widget`; the application wraps `TerminalWidgetBridge` instead of extending Terminal or `CustomPaint` with terminal-only style fields.
- **Reason:** The Widget Runtime owns its independent visual pipeline, while Terminal remains an opaque engine injected through the explicit CustomPaint escape hatch.
- **ADR reference:** [0003-widget-independent-pipeline](../adr/0003-widget-independent-pipeline.md), [0011-terminal-custompaint-gpu-injection](../adr/0011-terminal-custompaint-gpu-injection.md)

### Preserve the CustomPaint external-draw boundary

- **Choice:** Rounded clipping and decoration compose around `Primitive::External`; Terminal continues to receive frame-scoped render access and does not acquire Widget, Window, Surface, Device, or Queue ownership.
- **Reason:** This retains provider-by-ID paint ordering and the Runtime-owned frame policy rather than introducing a second Terminal rendering path.
- **ADR reference:** [0005-custom-paint-provider-by-id](../adr/0005-custom-paint-provider-by-id.md), [0011-terminal-custompaint-gpu-injection](../adr/0011-terminal-custompaint-gpu-injection.md), [0015-runtime-owned-frame-presentation](../adr/0015-runtime-owned-frame-presentation.md)

### Separate decoration, clipping, and layout semantics

- **Choice:** Decoration never changes measured size; `ClipBehavior` independently controls rounded child paint and hit testing; shadows paint outside the child clip but remain subject to ancestors.
- **Reason:** This keeps the retained Widget Runtime's layout, paint, and hit-test responsibilities explicit and avoids moving allocation policy into the renderer or terminal.
- **ADR reference:** [0002-signal-pull-model](../adr/0002-signal-pull-model.md), [0003-widget-independent-pipeline](../adr/0003-widget-independent-pipeline.md)

### Use Flutter-style value types with a bounded first-version surface

- **Choice:** Provide `Default` plus fluent builders; support optional color, uniform border, per-corner radius, ordered outer shadows, and `None`/`HardEdge`/`AntiAlias` clipping; omit inner shadows, per-edge borders, and save-layer clipping.
- **Reason:** The API is extensible without requiring speculative rendering features, and the Widget Runtime's independent pipeline can evolve without coupling to terminal rendering.
- **ADR reference:** [0003-widget-independent-pipeline](../adr/0003-widget-independent-pipeline.md)

### Preserve transparent terminal composition

- **Choice:** `BoxDecoration` defaults to no fill, and the terminal preset does not set a color; decoration paints shadow → background → child → border.
- **Reason:** An implicit opaque quad would violate the Acrylic contract, while the selected order keeps the external terminal below an optional border and above the background.
- **ADR reference:** [0005-custom-paint-provider-by-id](../adr/0005-custom-paint-provider-by-id.md), [0023-windows-acrylic-backdrop-not-mica](../adr/0023-windows-acrylic-backdrop-not-mica.md)

### Apply a concrete product preset at the Runtime Host root

- **Choice:** Wrap the main `TerminalWidgetBridge` with 12dp radius, 25%-opaque black shadow, `(0dp, 4dp)` offset, 12dp blur, zero spread, no fill, and anti-aliased clipping.
- **Reason:** The Host already composes the terminal root while Terminal remains styling-independent; applying the preset there gives the feature an observable product result without changing standalone Terminal rendering.
- **ADR reference:** [0011-terminal-custompaint-gpu-injection](../adr/0011-terminal-custompaint-gpu-injection.md), [0015-runtime-owned-frame-presentation](../adr/0015-runtime-owned-frame-presentation.md), [0023-windows-acrylic-backdrop-not-mica](../adr/0023-windows-acrylic-backdrop-not-mica.md)

## Test Plan

- **Integration tests:** Verify public builder defaults and validation; uniform and per-corner radius geometry; multi-shadow ordering; shadow/background/child/border scene order; unchanged decorated-child layout; rounded hit testing; ancestor-clip intersection; `Primitive::External` clipping for `None`, `HardEdge`, and `AntiAlias`; no-fill decoration preserving transparent pixels; retained-scene updates when decoration values change.
- **Manual tests:** Launch Harbor on Windows 11 and Windows 10; inspect corners and shadow at 100%, 150%, and 200% DPI; resize, maximize, restore, and move between monitors; exercise text selection, scrollbar dragging, wheel input, focus, typing, cursor blink, synchronized output, and paste confirmation; verify Acrylic remains visible through default cells and the confirmation window remains unchanged.
- **Performance thresholds:** Preserve the existing sub-2ms 80×24 terminal frame target at p95 on the reference development GPU; decoration must not introduce a continuous redraw deadline; steady-state frames with unchanged decoration must reuse retained scene data and perform no per-frame heap allocation proportional to terminal cell count.
- **Edge cases:** Zero-sized and fully clipped allocations; radius larger than half the box extent; independently large corner radii; zero-alpha and empty shadow lists; zero blur; negative spread collapsing shadow bounds; multiple overlapping shadows; fractional logical coordinates and DPI scales; ancestor rectangular and rounded clips; surface loss/recovery; opaque explicit terminal backgrounds; pointer positions exactly on rounded boundaries.

## Out of Scope

- Inner/inset shadows.
- Per-edge border colors or widths; the first version supports a uniform `Border::all` equivalent only.
- Gradients, background images, shape decorations other than rounded rectangles, or decoration-driven layout/padding.
- Flutter `AntiAliasWithSaveLayer`, arbitrary path clipping, filters, blend modes, or a general offscreen compositing API.
- Terminal-owned decoration fields or changes to `harbor-terminal`'s internal text, background, cursor, selection, scrollbar, or scheduling pipelines.
- Appearance changes to the standalone terminal adapter or paste confirmation window.
- User-configurable terminal radius, shadow, border, or Acrylic settings.
- Custom caption chrome, Mica, or changes to Windows compositor policy.
- Allowing a widget's own shadow to escape an ancestor clip.

## Future Evolution

- Add per-edge borders when a concrete widget requires asymmetric outlines.
- Add inner shadows or arbitrary shape decoration only after a compositing model and performance budget are specified.
- Add gradients and images as new decoration inputs without changing the Terminal boundary.
- Introduce user-configurable terminal decoration after the configuration schema and live-update policy are defined.
- Re-evaluate save-layer anti-aliasing if visual evidence shows direct anti-aliased rounded clipping is insufficient.
- Generalize rounded clip representation for nested transformed clips when non-axis-aligned transforms enter the Widget Runtime.
