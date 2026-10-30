# Separate Retained Cell and IME Overlays

**Status:** Completed
**Date:** 2026-10-30
**Sources:** Refinement conversation on 2026-10-30: after confirming the unconditional overlay scan, the owner accepted the recommendation to retain combining-mark overlays by row, cache IME independently, and produce a short spec without tickets or implementation.

## Context

At refinement, `Text::prepare_preedit` in `crates/harbor-terminal/src/render/text.rs` handles both cell suffix/isolated-mark paint and transient IME composition. Every valid `Pipeline::prepare` calls it even when ordinary text and Sequence preparation reuse their projections. It scans every visible cell, builds a fresh local vertex vector, and writes every nonempty overlay without checking whether its geometry changed. Empty overlays avoid GPU writes but not the grid scan; GPU buffer capacity is already retained.

The two overlay inputs have different lifecycles. Cell overlays depend on content damage and Sequence suppression, while IME depends on current composition, cursor and rendering resources. Merely returning when preedit is absent would omit cell overlays. Rebuilding a few rows but concatenating and uploading all rows would retain upload amplification.

## Decision

Separate the retained cell-overlay and IME-overlay projections, with independent CPU vertex retention and GPU buffers, while preserving existing compositing order and presentation semantics.

- Retain variable-length cell-overlay geometry by row. Use terminal content damage together with Sequence suppression damage to inspect/rebuild affected rows; reuse unaffected rows and their GPU geometry. Do not impose a new one-quad-per-cell limit on combining paint. Do not upload unchanged geometry or concatenate the entire overlay for every local change.
- Cache IME geometry independently against its actual paint dependencies, including composition text, cursor position, viewport, font metrics, palette and referenced scalar glyph placements/availability. Composition changes must not rescan terminal cell overlays unless they genuinely invalidate shared resources. Clear obsolete IME drawing once when composition ends; persistent absence does not scan terminal cells.
- Invalidate according to actual dependencies, not just empty content damage or the ordinary text layer's final dirty flag. Preserve invalidation information until both consumers observe it. Refresh dependent overlays on geometry/font/color changes, scalar atlas repacks and relevant missing-to-available glyph transitions. Sequence tile/scalar/fallback suppression changes must refresh affected cell-overlay rows even outside incoming content damage.
- Retain cache ownership in terminal rendering. Preserve transient, presentation-only IME and the coherent read-only update/acknowledgement boundary.

This supplements [ADR 0039](./0039-route-ime-preedit-as-transient-terminal-overlay.md), [ADR 0097](./0097-grid-resize-preserves-scalar-glyph-atlas.md) and [ADR 0098](./0098-incremental-sequence-presentation.md); it does not supersede their behavior or resource contracts. Renderer ownership and acknowledgement remain governed by [ADR 0045](./0045-gpu-independent-terminal-core-boundary.md) and [ADR 0094](./0094-mutation-revisions-and-incremental-terminal-updates.md).

## Consequences

- With stable inputs/resources, repeated preparation performs no overlay cell scanning, geometry rebuilding or vertex uploading, whether overlays are empty or populated.
- Local content changes may inspect/rebuild a touched row, but not unrelated rows; unchanged resulting geometry causes no upload. Removed geometry must stop drawing without stale pixels.
- Initial construction and genuine global invalidation may reconstruct the required projection. Exact private types, row-buffer allocation and dependency tracking mechanisms remain implementation choices.
- This removes the identified overlay-path work, not all `Pipeline::prepare` cost, and makes no numerical latency or speedup promise.
- This record selects future work only. No code, tests, performance measurements or runtime acceptance are claimed.

## Implementation evidence — 2026-10-30

After the spec-only refinement, the owner explicitly invoked `grimoire-loop`. The selected cache separation is implemented in terminal rendering, with row-local buffers, retained CPU staging, shared source-row reuse and independent suppression/glyph dependency checks. All selected automated checks pass; [verification](../../docs/developer/incremental-terminal-overlays.md) records requirement coverage, both GPU backends and the remaining unmeasured/interactive limitations. This realizes the decision without claiming zero whole-pipeline cost or desktop acceptance.
