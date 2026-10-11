# Color Emoji Offscreen Presentation

**Status:** Proposed
**Date:** 2026-10-10

## Context

[Issue #181](https://github.com/wangjq4214/harbor/issues/181) adds complete color emoji and emoji ZWJ presentation to the retained Unicode contract of [ADR 0044](./0044-unicode-text-unit-and-presentation-width-policy.md). The current DirectWrite backend resolves individual scalars, and the terminal renderer uses a foreground-tinted R8 coverage atlas. Neither changing the font alone nor changing the texture format alone supplies sequence shaping and color compositing.

The refinement conversation confirmed the architecture below and the fit policy. This record is Proposed because the choice is approved but has not been implemented; approval is not a runtime support claim. Manual color-run compositing was considered, but implementing the richer COLR v1 paint operations would substantially broaden the renderer work. Color-font formats and Windows API availability still require capability detection and scoped evidence.

## Decision

- Keep the retained `Cell` source text and assigned grid width authoritative. Shape complete emoji text units with DirectWrite, including sequence-aware font fallback, without introducing another terminal width engine or changing selection/copy semantics.
- Use Direct2D to rasterize/composite the shaped emoji presentation into an offscreen color tile. Reuse native offscreen resources. Initially transfer cached raster results to the concrete wgpu terminal renderer through pixel data, not Direct2D/wgpu shared-texture interoperation.
- Preserve the existing R8 ordinary-monospace path and add a separate, bounded, lazily allocated color atlas. Do not require whole-layout shaping or four-channel atlas storage for ordinary text.
- Fit oversized emoji artwork by uniform downscaling and positioning within the assigned cell rectangle, with final clipping to that rectangle. Do not enlarge its cell allocation or advance neighboring content according to font metrics.
- Keep font/shaping/raster resources in the presentation layer and wgpu atlas/geometry resources in the renderer. Do not add native font handles, shaping output, atlas UVs, or GPU state to the terminal model/update contract. Preserve [ADR 0045](./0045-gpu-independent-terminal-core-boundary.md)'s GPU-independent core and coherent damage consumption.
- Distinguish complete color presentation, complete monochrome presentation, and bounded unsupported-sequence fallback. Unsupported font, image format, or native capability must not be treated as successful joined presentation. Preserve the documented leading-pictograph fallback where available and a visible missing-glyph cue when no usable glyph is available, without rewriting source text.

## Consequences

- The native backend must detect relevant color-format/API capabilities rather than claim universal emoji, font, or Windows support. A COLR table version or nonempty/color bitmap is not proof that a particular sequence is correctly presented.
- The presentation request/cache must distinguish original sequence, font session and resolved fallback faces, effective raster size/DPI, style, presentation, and raster-dependent appearance. Font session changes invalidate session-local face identities.
- Bound shaping, negative-result and CPU raster caches as well as atlas memory. Atlas repacks invalidate issued UVs and require the affected render projection to be rebuilt.
- Fixed-color artwork is not tinted with terminal foreground color. Font layers that request the current foreground must honor it; pixel format, color space, alpha representation and blending must agree across native rasterization and GPU composition.
- Fragmented updates and font/DPI/appearance changes must replace stale presentation without text loss. Existing combining/orphan-mark and preedit behavior must not regress or paint a sequence twice.
- Capture comparable simple-monospace performance evidence before and after changes. Cold-cache offscreen raster/readback cost is separate from the retained ordinary-text fast path.
- This decision supplements ADR 0044's presentation policy; it does not supersede its text/width semantics, resolve its documented final-column no-wrap discrepancy, or make #181 a prerequisite for #153.
