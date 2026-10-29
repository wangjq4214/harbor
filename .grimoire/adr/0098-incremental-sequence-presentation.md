# Incremental Sequence Presentation

**Status:** Proposed
**Date:** 2026-10-29
**Sources:** Owner refinement request on 2026-10-29: introduce row/cell-level Sequence dirty tracking, reuse unchanged emoji/presentation data, and invalidate according to font-cache generation, atlas repacking and cell changes rather than forcing full ordinary-text vertex uploads merely because emoji remain visible.

## Context

At refinement, `SequenceLayer::prepare` in `crates/harbor-terminal/src/render/sequence.rs` clears `emoji_cells` and `tile_cells`, scans the complete visible snapshot, reconstructs sequence geometry and writes its vertex batches. `Text::prepare_with_dirty` in `crates/harbor-terminal/src/render/text.rs` then sets full text-layer dirtiness when the previous or current projection contains emoji. Thus a bounded ordinary-text edit can rebuild and upload the whole ordinary-text batch while unrelated emoji are unchanged.

The bounded color atlas already caches native presentation results. That cache alone does not retain the per-cell projection or eliminate the ordinary-text upload amplification. Its repacking and complete/fallback transitions can affect cells outside the incoming dirty span, so simply removing the full-dirty guard is not sufficient.

## Decision

Introduce row/cell-level incremental dirty tracking for the terminal Sequence presentation projection. Reuse unchanged emoji and related presentation data, and base invalidation on actual cell changes, font/presentation-cache generations and atlas placement changes. The presence of unchanged visible emoji alone must not force a full ordinary-text vertex upload.

Keep this retained state in presentation/rendering, consistent with [ADR 0054](./0054-color-emoji-offscreen-presentation.md) and the GPU-independent engine boundary of [ADR 0045](./0045-gpu-independent-terminal-core-boundary.md). Preserve the existing complete color/monochrome/fallback behavior and bounded resources. Dependent geometry must remain coherent when placements or presentation outcomes change; incrementality does not authorize stale UVs or duplicated scalar/sequence paint.

Owner-confirmed clarification (2026-10-29): dirty-row granularity is sufficient; strict cell-only reconstruction is not required. Both retained CPU projection reuse and partial Sequence GPU vertex uploads are in scope. Ordinary bounded changes must not rebuild or upload unaffected rows merely because other rows contain persistent emoji. Resize and genuine global resource invalidation may perform necessary complete reconstruction. Exact retained storage, buffer layout and allocation policy remain implementation choices.

## Consequences

- Native tile caching and retained row/cell presentation are separate concerns; avoiding rerasterization alone does not remove repeated projection and upload work.
- Dirtiness of the Sequence projection and the ordinary text layer must reflect their actual dependencies rather than a blanket emoji-presence condition.
- Real resource/placement invalidation still requires refreshing affected references, including references outside the immediate content dirty span.
- Existing text/width/copy semantics and coherent update acknowledgement remain unchanged ([spec 0020](../spec/0020-color-emoji-sequence-presentation.md), [ADR 0094](./0094-mutation-revisions-and-incremental-terminal-updates.md)). This supplements the scalar-atlas reuse decision in [ADR 0097](./0097-grid-resize-preserves-scalar-glyph-atlas.md), not a redesign of widget text or terminal model storage.
- This records the selected direction only. No implementation, tests, performance measurements or acceptance is claimed.
