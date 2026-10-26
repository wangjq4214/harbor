# Widget Decoration and Terminal Chrome

**Spec ID:** 0009
**Status:** Accepted (owner-confirmed)
**Date:** 2026-08-26

**Superseded in part:** the earlier 16dp inset and 12dp/25%/4dp/12dp preset in the acceptance text below are superseded by the current Terminal Window Inset (4dp) and Terminal Decoration Preset (8dp radius, 10%-opaque shadow, 1dp offset and blur) recorded in CONTEXT.md. The earlier values are retained as historical design intent.
## Requirement

Harbor must provide Flutter-style, reusable widget box decoration and use it to render the main terminal with anti-aliased rounded corners and an outer shadow while preserving layout, input, external-draw, and Acrylic behavior.

## Solution

`harbor-widget` exposes a general-purpose `DecoratedBox` wrapper with immutable value types implementing `Default` plus fluent builders modeled after Flutter: `BoxDecoration`, `BorderRadius`, `Border`, `BoxShadow`, and `ClipBehavior`. `BoxDecoration` supports an optional color, one uniform border, uniform or per-corner radii, and an ordered list of outer shadows. Values use logical pixels and do not alter measurement or layout bounds.

Decoration paints in the stable order shadow → background → child → border. Shadow lists paint in list order from back to front, with the first item at the bottom. A decoration with no color emits no background fill. Radius, border width, and blur radius are finite and non-negative; spread radius may be negative; non-finite values are invalid.

`ClipBehavior` supports `None`, `HardEdge`, and `AntiAlias`, defaulting to `None`. When clipping is enabled, the rounded shape constrains child painting and hit testing but does not clip the box's own shadows. Ancestor clips remain authoritative. Rounded clipping applies to normal widget primitives and `Primitive::External`, so the Terminal continues to render through `CustomPaint` rather than gaining terminal-specific styling or rendering APIs.

The main Runtime Host composes `TerminalWidgetBridge` inside `DecoratedBox` within the existing 16dp Terminal Window Inset. The product preset uses a 12dp radius, 25%-opaque black outer shadow, `(0dp, 4dp)` offset, 12dp blur, zero spread, no fill, and `ClipBehavior::AntiAlias`. The absent fill preserves Default Background Cell transparency and the existing Windows Acrylic backdrop.

## Decisions

Decoration is generic widget composition, not terminal-owned styling. Preserve CustomPaint ownership and paint order ([ADR 0011](../adr/0011-terminal-custompaint-gpu-injection.md)); native-host ownership follows [Spec 0012](./0012-widget-winit-native-host-consolidation.md).

## Test Plan

Pointer positions inside the rectangle but outside the rounded clip must not target the child. Non-terminal children must remain reusable and undecorated siblings unchanged. Invalid non-finite decoration must be rejected without panic or invalid GPU geometry; zero/collapsed effects produce no spurious pixels.

- **Integration tests:** Verify public builder defaults and validation; uniform and per-corner radius geometry; multi-shadow ordering; stable shadow `SceneItem` identity across retained updates, including when zero-alpha or collapsed shadows are filtered as ineffective entries; shadow/background/child/border scene order; unchanged decorated-child layout; rounded hit testing; ancestor-clip intersection; `Primitive::External` clipping for `None`, `HardEdge`, and `AntiAlias`; no-fill decoration preserving transparent pixels; retained-scene updates when decoration values change.
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
