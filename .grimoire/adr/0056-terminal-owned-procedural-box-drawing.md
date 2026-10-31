# Terminal-Owned Procedural Box Drawing

**Status:** Proposed
**Date:** 2026-10-11
**Superseded by:** [ADR 0101](./0101-demand-bounded-box-and-decoration-storage.md), only for fixed-slot allocation and underline slot/offset preservation constraints; remaining decisions continue to apply.

**2026-10-31 refinement:** The owner approved demand-bounded box/decoration storage and GPU/CPU capacity reclamation. This replaces the allocation constraints below, not their presentation or local-update guarantees. Implementation and detailed reclamation policy are not established by this note.

## Context

[Issue #193](https://github.com/wangjq4214/harbor/issues/193) requires seamless box-drawing joins and decoration correctness under font and DPI changes. Source inspection at `fbdd1bb` found that `harbor-text` resolves and rasterizes these characters as ordinary font glyphs, while `Text::build_range_vertices` places and clips their bitmaps using font bearings. This path has no terminal-specific guarantee that compatible strokes meet at adjacent cell edges.

The user approved terminal-owned procedural box drawing and the associated lifecycle direction in the #193 refinement conversation on 2026-10-11. This status is Proposed because the decision is approved but not implemented or runtime-verified. Stretching font bitmap quads to fill seams was rejected: it cannot reliably align both axes and junctions and can distort stroke thickness and double lines. UI-style smoothing is forbidden by the issue's sharpness and contrast constraint.

## Decision

- Render supported box-drawing characters as terminal-cell geometry in `harbor-terminal`, rather than changing shared `FontBook` resolution or `GlyphAtlas` rasterization for all consumers. Ordinary text and widget text retain their existing font paths.
- Derive connecting arms from consistent cell boundaries and pixel-alignment rules. A cell's shape depends on its source character and viewport, not on inspecting neighboring content. Compatible arms can therefore meet without cross-cell shaping, a retained run cache, or neighbor-driven damage expansion. Preserve intentional gaps in dashed characters.
- Keep this geometry in a separate, bounded renderer-owned resource layout with fixed per-cell slots. Do not change the existing underline slot size or mix box geometry into its upload offsets.
- Preserve authoritative terminal source text, assigned width, selection/copy, foreground/inverse/conceal semantics, clipping, and the GPU-independent engine/update contract of [ADR 0045](./0045-gpu-independent-terminal-core-boundary.md). This is a presentation extension, not a new width or text engine.
- Preserve the current physical-font-size policy for DPI-only transitions. Font replacement updates metrics and related renderer resources coherently; a display scale change is not authorization to multiply ordinary terminal glyphs and decoration by DPI again. Decoration projection follows the same metric/viewport contract across underline, strikethrough, and overline.

## Consequences

- Joined rendering no longer depends on the selected font's box-drawing bearings or edge coverage. Geometry must still be verified with GPU pixels, not just matching CPU endpoints.
- Character-local geometry preserves bounded dirty updates. Erase, conceal, character replacement, viewport changes, resize, and font replacement must clear or invalidate obsolete geometry without losing pending engine damage.
- Box-drawing buffers and underline buffers retain independent slot sizing and offset calculations. Invalid metrics and extreme scales must not yield non-finite vertices, out-of-bounds uploads, or unbounded geometry work.
- Font-resource replacement can reuse the existing `TerminalRenderPipeline::replace_fonts` seam; this decision does not introduce user-configuration hot reload. Native-resource smoke and actual desktop cross-monitor evidence remain distinguishable acceptance obligations, not claims established by this ADR.
- This supplements [terminal render ownership](./0012-consolidate-render-ui-into-terminal.md) and preserves [modern SGR policy](./0051-modern-sgr-decoration-and-blank-retention-policy.md); it does not supersede either policy.

## Sources

- Issue #193 and the user's approval of the refinement recommendation on 2026-10-11.
- `crates/harbor-text/src/font.rs` and `crates/harbor-text/src/atlas.rs`: native font resolution and ordinary scalar atlas path.
- `crates/harbor-terminal/src/render/text.rs`: `Text::build_range_vertices`.
- `crates/harbor-terminal/src/render/decoration.rs`: fixed underline slots and decoration projection.
- `crates/harbor-terminal/src/render/pipeline.rs`: `replace_fonts`, `sync_raster_scale`, and `sync_viewport`.
- `crates/harbor-terminal/src/render/text_tests.rs`: `dpi_transitions_preserve_physical_metrics_and_scalar_pixels` specifies existing physical-font-size behavior; its presence is not a fresh test-execution claim.
