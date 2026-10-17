# Windows System-Native Font Backend

**Spec ID:** 0003
**Status:** Implemented
**Superseded in part:** the `HARBOR_FONT` path override is superseded by TOML `[font]` configuration in `harbor-config` (CONTEXT.md, “System Default Font Selection”); override requirements below are historical.
**Date:** 2026-07-30

## Requirement

On Windows, Harbor must select, resolve, measure, and rasterize terminal fonts through DirectWrite without retaining complete font-file copies in the Rust heap, while preserving primary-font terminal metrics and the existing WGPU glyph-atlas renderer.

## Solution

Replace the Windows `fontdb`/`fontdue` font path in `harbor-text` with a DirectWrite-backed font abstraction. DirectWrite owns primary-face selection, process-private loading of the optional `HARBOR_FONT` file, system fallback resolution, metrics, glyph identity, and grayscale glyph rasterization.

When `HARBOR_FONT` is absent, the backend delegates primary-face selection to Windows rather than consulting Harbor’s hard-coded font candidates. When it is present, the referenced font becomes the primary face; an invalid configured path is a startup error. Missing characters from either primary source are resolved through DirectWrite system fallback.

Primary-face metrics remain the sole source of terminal cell width, line height, ascent, underline, and strikethrough placement. Fallback faces cannot change grid geometry. Glyph resolution is cached, including unavailable results, and atlas identity distinguishes at least face, glyph, size, and style so multiple fallback faces cannot collide.

DirectWrite resources use explicit ownership and thread-lifetime rules compatible with application startup and terminal rendering. Discovery collections and rejected candidates are released after selection. Long-lived state contains only required native handles, resolution caches, and glyph-atlas data; it contains no complete font-file `Vec<u8>`.

`harbor-text` remains CPU-only and exposes atlas-compatible rasterized glyph data. `harbor-terminal` retains the existing WGPU texture, upload, vertex, and draw pipeline. Public `harbor-text` APIs may change, and all workspace consumers are migrated together.

The Windows implementation has no runtime fallback to `fs::read`, `fontdb`, or `fontdue`. Non-Windows builds are rejected at compile time for this delivery.

## Decisions

DirectWrite replaces eager whole-font parsing, not merely file discovery. CPU text and terminal GPU rendering stay separate; see [ADR 0011](../adr/0011-terminal-custompaint-gpu-injection.md).

Non-Windows compilation is rejected explicitly; no legacy loader fallback is permitted.

## Test Plan

Default and configured primary faces must render Latin and fallback CJK at stable positive primary metrics. Cached missing-primary resolution must avoid rescans/reopenings/reparsing.

- **Integration tests:** Exercise DirectWrite factory creation, system primary selection, process-private `HARBOR_FONT` loading, primary metrics, system fallback across multiple faces, unavailable-result caching, stable glyph identity, grayscale rasterization, and the `harbor-terminal` atlas contract. Windows-dependent tests must distinguish platform integration tests from deterministic backend-policy tests.
- **Manual tests:** Run cold Latin startup, sustained Latin output, first and repeated CJK rendering, symbols/emoji with available grayscale outlines, a primary font with built-in CJK coverage, an invalid `HARBOR_FONT`, resize/DPI changes, and repeated terminal redraws. Confirm that default appearance may follow Windows rather than the former hard-coded candidate order.
- **Performance thresholds:** Under the documented Latin DHAT workload, the reference-machine global live-heap peak must remain below 40 MiB; Windows font initialization must make zero calls to the legacy `fs::read`/`fontdb`/`fontdue` path and allocate no buffer equal to a complete font file; peak and steady Windows private memory must be lower than a pre-change run using the identical executable scenario. First-present and first-fallback latency are recorded but have no hard gate in this spec.
- **Edge cases:** Missing or malformed configured files; TTC/collection inputs; a primary face missing Latin or CJK characters; unavailable fallback faces; repeated unresolved characters; spaces and zero-area glyphs; multiple characters resolving to different faces; DPI/font-size changes; atlas rebuild/eviction; DirectWrite initialization failure; and transfer or recreation of native resources across the startup/render thread boundary.

## Out of Scope

- CoreText, Fontconfig, FreeType, or any other non-Windows backend.
- Retaining the portable `fontdb`/`fontdue` implementation as a runtime or compile-time fallback.
- The reference document’s “lazy `fontdue` fallback first” delivery path; this spec intentionally proceeds directly to DirectWrite.
- Complex-script shaping, ligatures, grapheme-cluster layout, bidi layout, and changes to terminal cell-width policy.
- Full-color emoji rendering or changes from the existing grayscale atlas format.
- Dynamic atlas sizing, vertex scratch-buffer reuse, atlas eviction redesign, or replacement of the WGPU text pipeline.
- Installing fonts or mutating the user/system font collection.
- Asynchronous missing-glyph resolution and placeholder/redraw coordination.
- Preserving the exact default font family or appearance produced by the former hard-coded candidate order.
