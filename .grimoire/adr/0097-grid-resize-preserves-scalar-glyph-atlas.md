# Grid Resize Preserves the Scalar Glyph Atlas

**Status:** Proposed
**Date:** 2026-10-27
**Sources:** Refinement conversation on 2026-10-27: the owner approved reusing the terminal glyph atlas across grid resize, including active-set reclamation under capacity pressure, and selected a short-spec-only endpoint.

## Context

At refinement, `Text::prepare_with_dirty` in `crates/harbor-terminal/src/render/text.rs` unconditionally rebuilds the CPU scalar atlas and uploads the complete R8 texture when rows or columns change. Grid layout changes require new vertex geometry, but do not themselves invalidate glyph bitmaps at an unchanged font session and physical font size.

Simply replacing the resize rebuild with the existing incremental entry point is insufficient: `GlyphAtlas::rasterize_new_at_size` retains all stored keys when capacity forces repacking. The old resize rebuild also reclaimed offscreen glyphs; removing it without a pressure policy can let stale entries displace the current working set.

## Decision

Separate grid/projection invalidation from scalar glyph-cache invalidation in terminal rendering:

- Reuse the existing scalar atlas across ordinary grid resize when font resources and physical font size remain valid. Preserve existing placements/UVs absent a capacity-driven repack. A cached-only resize performs no scalar glyph rasterization or scalar atlas upload; geometry may still be fully rebuilt.
- Consider the entire new visible projection and current IME preedit when determining required scalar glyphs, including combining/suffix paint inputs. Add missing glyphs incrementally when capacity permits; do not rebuild the atlas solely because dimensions changed.
- Retain cached glyphs in the ordinary no-pressure path. When capacity requires reclamation/repacking, use the current visible projection plus preedit as the active scalar working set rather than retaining all historical atlas entries. Stale cached entries must not cause omission of an otherwise packable current working set.
- Permit full atlas synchronization when repacking actually changes placements; rebuild all dependent geometry before using changed UVs. This is not a prohibition on full rebuilding under genuine capacity pressure or font-resource replacement.
- Keep the policy terminal-owned. Do not change widget text caching, terminal text/width semantics, the separate emoji color-atlas policy, or coherent engine-update acknowledgement. Font replacement and physical-font-size invalidation retain their existing coherent resource-replacement contracts; DPI alone retains the established physical-size policy.

## Consequences

- Repeated cached-only resize avoids the identified rasterization and full-texture-upload costs, not all full-grid work or every source of resize latency.
- The active set must cover the coherent current projection, not only dirty ranges from an earlier geometry; preedit remains presentation-only.
- Reclamation must also be safe for ordinary incremental rendering sharing this terminal atlas: a repack cannot leave unchanged cells or overlays using obsolete UVs.
- This does not promise that an arbitrarily large active glyph set fits the existing bounded atlas, select a new capacity/packing algorithm, or redesign native font and resolution caches.
- This supplements [ADR 0045](./0045-gpu-independent-terminal-core-boundary.md), [ADR 0054](./0054-color-emoji-offscreen-presentation.md) and [ADR 0094](./0094-mutation-revisions-and-incremental-terminal-updates.md); their ownership, emoji and update-consumption contracts remain unchanged. It does not supersede the bounded resize scheduling choice in [ADR 0096](./0096-bounded-multi-tab-resize-scheduling.md).
- Approval records the direction only. No implementation, performance measurements or runtime acceptance is claimed.
