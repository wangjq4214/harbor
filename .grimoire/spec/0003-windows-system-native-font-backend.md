# Windows System-Native Font Backend

**Spec ID:** 0003
**Status:** In Progress
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

### Seams

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Windows font services | `harbor-text` → DirectWrite | A usable DirectWrite factory, system font collection, fallback service, and process-private custom-font references | Native face identities, primary metrics, fallback mappings, glyph IDs, and grayscale glyph images |
| Font configuration | Process environment → `harbor-text` | Optional `HARBOR_FONT` filesystem path | A process-private primary face or an explicit configuration error |
| Text rendering contract | `harbor-terminal` ↔ `harbor-text` | Stable primary metrics and rasterizable resolved glyphs | Atlas-compatible bitmap/placement data without changing terminal GPU ownership |

## End-to-End Tests

### E2E: Default Windows startup

- **Given:** Windows with no `HARBOR_FONT` value
- **When:** Harbor creates its first terminal window and renders Latin text
- **Then:** DirectWrite selects the primary face, terminal metrics are positive and stable, the text appears through the existing WGPU atlas, and no legacy font loader is invoked

### E2E: Configured primary with system fallback

- **Given:** `HARBOR_FONT` points to a valid process-private font that lacks a tested CJK character
- **When:** Harbor renders Latin and CJK text
- **Then:** Latin uses the configured primary face, CJK uses DirectWrite system fallback, and both use cell geometry derived only from the configured primary face

### E2E: Cached fallback resolution

- **Given:** A terminal session has already rendered a character missing from its primary face
- **When:** The same character is rendered repeatedly and after terminal redraws
- **Then:** The resolved face and glyph are reused without rescanning, reopening, or reparsing fonts, and atlas output remains stable

### E2E: Invalid configured font

- **Given:** `HARBOR_FONT` points to a missing, unreadable, or unsupported font
- **When:** Harbor initializes text rendering
- **Then:** Startup returns a clear font-configuration error and does not silently use the system default or a legacy loader

### E2E: Windows memory profile

- **Given:** The documented Latin-only DHAT scenario and matching Windows private-memory measurement
- **When:** Harbor reaches first presentation and the defined steady-state dwell point
- **Then:** No complete font file is allocated in the Rust heap, the global live-heap peak is below 40 MiB on the reference machine, and peak and steady private memory are lower than the recorded pre-change baseline

### E2E: Unsupported platform

- **Given:** A non-Windows compilation target
- **When:** The workspace attempts to compile `harbor-text`
- **Then:** Compilation fails with an explicit unsupported-platform diagnostic rather than selecting the old portable backend

## Decisions

### DirectWrite owns the full Windows font path

- **Choice:** Use DirectWrite for selection, fallback, metrics, glyph resolution, and rasterization instead of using it only to discover files for `fontdue`.
- **Reason:** The measured dominant allocation path is eager `fontdue` parsing of large CJK collections; retaining parser-owned font bytes would not satisfy the memory requirement. Locating the backend in `harbor-text` preserves the terminal dependency boundary.
- **ADR reference:** [0012-consolidate-render-ui-into-terminal](../adr/0012-consolidate-render-ui-into-terminal.md)

### Keep CPU text and GPU rendering separated

- **Choice:** Change glyph identity and CPU rasterization as needed while retaining the existing WGPU atlas and terminal rendering pipeline.
- **Reason:** This preserves the shared CPU text-core boundary and terminal-owned GPU rendering established by existing architecture decisions.
- **ADR reference:** [0004-widget-dependency-boundary](../adr/0004-widget-dependency-boundary.md), [0008-widget-runtime-for-confirmation-window](../adr/0008-widget-runtime-for-confirmation-window.md), [0011-terminal-custompaint-gpu-injection](../adr/0011-terminal-custompaint-gpu-injection.md), [0012-consolidate-render-ui-into-terminal](../adr/0012-consolidate-render-ui-into-terminal.md)

### Delegate default selection and missing-glyph fallback to Windows

- **Choice:** Preserve only the `HARBOR_FONT` override; otherwise use DirectWrite system selection and DirectWrite system fallback.
- **Reason:** This removes hard-coded primary/CJK candidate lists, handles scripts through the platform font system, and preserves `harbor-text` as the font-policy owner.
- **ADR reference:** [0012-consolidate-render-ui-into-terminal](../adr/0012-consolidate-render-ui-into-terminal.md)

### Preserve terminal geometry from the primary face

- **Choice:** Fallback glyphs never alter terminal cell metrics.
- **Reason:** Stable grid dimensions are required by `harbor-terminal`; fallback affects glyph selection only.
- **ADR reference:** [0012-consolidate-render-ui-into-terminal](../adr/0012-consolidate-render-ui-into-terminal.md)

### Deliver a Windows-only backend without a legacy fallback

- **Choice:** Reject non-Windows compilation and fail explicitly when no usable DirectWrite face exists.
- **Reason:** The product runtime is currently Windows-only, and retaining the old loader would violate the memory invariant. No existing ADR governs platform support, so ADRs 0001–0013 were cross-checked with no conflict.
- **ADR reference:** No directly applicable ADR.

### Permit coordinated workspace API changes

- **Choice:** `harbor-text` public font and atlas APIs may change while all workspace callers migrate together.
- **Reason:** Native face identity, fallback caching, and rasterization ownership cannot be represented reliably by the current immutable `FontBook` plus `char`-only atlas contract.
- **ADR reference:** [0004-widget-dependency-boundary](../adr/0004-widget-dependency-boundary.md), [0012-consolidate-render-ui-into-terminal](../adr/0012-consolidate-render-ui-into-terminal.md)

### ADR cross-check

- ADRs 0001 and 0006 are superseded and introduce no active constraint.
- ADRs 0002, 0003, 0005, 0007, 0009, 0010, and 0013 concern widget reactivity/rendering, windowing/input, parser API, or PTY I/O and do not conflict with this requirement.
- ADRs 0004 and 0008 are satisfied because `harbor-text` remains a shared CPU core without `wgpu`, `winit`, or `harbor-widget` dependencies.
- ADRs 0011 and 0012 are satisfied because `harbor-terminal` continues to own GPU text rendering and consumes `harbor-text` glyph-atlas output.

## Test Plan

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

## Future Evolution

- Add platform backends such as CoreText and Fontconfig/FreeType behind the same text-backend contract.
- Add color-glyph atlas support if system emoji fallback requires color presentation.
- Add shaping and grapheme-aware resolution if terminal requirements expand beyond one-scalar glyph lookup.
- Revisit dynamic atlas allocation and reusable upload buffers after DirectWrite memory measurements isolate the remaining costs.
- Consider sharing immutable DirectWrite factories or face caches across multiple terminal/widget runtime instances if profiling shows duplicated native state.
- Re-evaluate asynchronous fallback only if measured first-use latency is unacceptable and redraw/cancellation semantics are specified.
