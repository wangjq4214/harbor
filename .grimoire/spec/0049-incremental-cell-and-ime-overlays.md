# Incremental Cell and IME Overlays

**Spec ID:** 0049
**Status:** Implemented
**Date:** 2026-10-30
**Sources:** Owner refinement conversation on 2026-10-30: confirm the residual full-grid overlay preparation cost, then approve separate row-retained combining overlays and independently cached IME, with a short-spec-only endpoint. [ADR 0100](../adr/0100-separate-retained-cell-and-ime-overlays.md) records the selected solution. This is newly selected future work, not an implementation or acceptance claim.

## Requirements

### R1 — Reuse unchanged overlays

After initial preparation, with unchanged content and rendering dependencies, repeating a valid prepare must perform zero overlay cell inspections, zero overlay geometry rebuilding and zero overlay vertex uploads. Cover both an empty overlay (no IME or combining marks) and populated overlays. Retain CPU vertices/storage and GPU geometry instead of constructing a new transient vector each time. This requirement concerns the overlay path, not every operation in `Pipeline::prepare`.

### R2 — Keep cell overlays row-local

Retain variable-length suffix/isolated-mark geometry by row. For local content changes with stable resources, inspect/rebuild only affected rows, including rows affected by Sequence suppression changes. Keep all unaffected rows' CPU and GPU geometry. Dirty-row granularity is sufficient; do not require strict cell-only reconstruction.

If a touched row's resulting geometry is unchanged, do not upload it. If geometry changes, update only affected row geometry, not a concatenated full-screen batch. Removal of all marks from a row must stop drawing its old geometry. Preserve multiple combining marks per cell; do not introduce a one-quad-per-cell limitation.

### R3 — Cache IME independently

Retain a separate IME projection and GPU buffer. Rebuild it only when actual paint inputs change: composition text, cursor position, viewport/grid geometry, font metrics/resources, palette or referenced glyph availability/placement. An IME-only change must not scan/rebuild cell-overlay rows unless shared rendering dependencies genuinely changed. A cell-only edit that changes no IME dependency must retain IME geometry.

When composition becomes empty/absent, clear its drawn range once. Subsequent absent preparations perform no IME geometry construction/upload and no cell scan on behalf of IME. Preserve existing preedit layout, clipping, cursor behavior and cancellation semantics from [ADR 0039](../adr/0039-route-ime-preedit-as-transient-terminal-overlay.md).

### R4 — Invalidate actual dependencies coherently

Empty content damage alone does not establish cache validity. Both overlay projections must observe relevant viewport, geometry, font/metrics and color changes, scalar atlas placement changes/repacking, and missing-to-available glyph transitions. Refresh affected references before drawing; do not permanently cache a missing glyph as empty geometry. Placement-stable additions unrelated to retained overlay dependencies must not force unrelated rows to rebuild/upload.

Sequence tile/scalar/fallback suppression changes must refresh affected cell-overlay rows even beyond incoming terminal content damage. Preserve invalidation information until overlay preparation observes it; the ordinary text layer clearing its own dirty flag must not hide an overlay dependency change.

Initial construction, resize and genuine global invalidation may reconstruct the necessary projection. Continue to satisfy scalar-atlas reuse and Sequence incrementality under [spec 0047](./0047-grid-resize-glyph-atlas-reuse.md) and [spec 0048](./0048-incremental-emoji-sequence-rendering.md).

### R5 — Preserve presentation and ownership

Incremental output must match a fresh complete preparation of the same current projection, including existing paint order, suffix filtering, isolated-mark dotted circles, continuation/conceal filtering, tile suppression, assigned-cell clipping and IME placement. Clearing/replacing overlays must leave no stale or duplicate paint or obsolete UVs.

Keep the new caches in terminal rendering; do not move transient preedit or GPU cache state into terminal cells/PTY content. Preserve coherent read-only updates and pending damage after skipped/failed preparation ([ADR 0045](../adr/0045-gpu-independent-terminal-core-boundary.md), [ADR 0094](../adr/0094-mutation-revisions-and-incremental-terminal-updates.md)).

## Solution and integration contracts

Use independent retained cell and IME overlay projections as selected in ADR 0100. Cell overlays retain row-local CPU vertices and independently updatable GPU geometry; IME retains its own CPU geometry and GPU buffer. Exact private APIs, dependency tracking and row-buffer allocation remain implementation choices, not additional requirements.

| Seam | Exchange | Guarantee |
| --- | --- | --- |
| Coherent update → cell overlays | Current cells/dimensions and accumulated content damage | Inspect affected rows; retain unaffected rows without consuming engine damage |
| Sequence → cell overlays | Current tile suppression and extra suppression damage | Restore/suppress dependent mark paint even outside incoming content damage |
| Preedit/cursor → IME overlay | Current composition and live cursor geometry | Independent reuse, correct replacement and one-time clearing |
| Scalar resources/layout/appearance → both overlays | Current glyph availability/placements, font metrics, viewport and palette | Refresh actual dependencies, including repacked UVs and previously missing glyphs |
| Retained projections → GPU/draw | Changed row geometry or changed IME geometry/count | No full-overlay upload amplification; preserve paint order and remove obsolete drawing |

Verified source surfaces: [`render/text.rs`](../../crates/harbor-terminal/src/render/text.rs) (`prepare_preedit`, `prepare_with_dirty`, atlas synchronization), [`render/pipeline.rs`](../../crates/harbor-terminal/src/render/pipeline.rs) (unconditional valid-prepare overlay call), [`render/sequence.rs`](../../crates/harbor-terminal/src/render/sequence.rs) (suppression damage) and [`render/gpu.rs`](../../crates/harbor-terminal/src/render/gpu.rs) (retained capacity and nonempty writes). At refinement these establish the residual cost, not a requirement to preserve current method names.

## Input-to-outcome verification

| Scenario | Given / when | Required result |
| --- | --- | --- |
| Warm empty/populated overlays | Prepare ordinary-only cells, combining/isolated marks, or active IME; repeat without relevant changes | Same image; zero overlay cell inspections, geometry rebuilds and vertex uploads (R1) |
| Local ordinary edit | Keep combining marks on untouched rows; change an ordinary cell on one row with stable resources/cursor | Inspect at most affected rows; no upload if overlay geometry is unchanged; untouched rows and IME retained (R2/R3) |
| Local mark churn | Add, replace and remove suffixes/isolated marks on one row; keep other rows unchanged | Only affected row geometry may change/upload; removed paint disappears; output matches full reference (R2/R5) |
| IME lifecycle | Start/edit/move composition, then cancel/commit/disable or lose focus using existing lifecycle behavior | Correct existing composition/cursor outcome; no unrelated cell-row work; clear obsolete IME once and reuse absence (R3/R5) |
| Suppression transition | Change Sequence tile/scalar/fallback outcome, including changes outside terminal damage | Correct mark suppression/restoration on affected rows, without unrelated uploads (R4/R5) |
| Resource changes | Change palette/viewport/font metrics, repack scalar atlas, or make a previously missing referenced glyph available with no cell damage | Both projections reflect actual dependencies; valid UVs and visible glyphs; unchanged unrelated placements do not force row work (R4) |
| Resize/deferred updates | Resize or accumulate changes while preparation is skipped/failed, then prepare successfully | Current full-reference-equivalent output; no lost damage or obsolete row geometry (R4/R5) |

Verify cell inspections, rebuilt rows/IME batches and vertex-write ranges/bytes with deterministic work observations, alongside complete-reference render comparisons. Use warm repeated prepares and local edits at multiple grid sizes to distinguish removed full-grid work from cached rasterization alone. No numerical speedup or latency threshold is selected; timing cannot substitute for work and correctness assertions.

## Decisions, limits and handoff

- **Row-retained cell overlays plus independent IME:** owner-approved recommendation; ADR 0100. Ordinary content damage and Sequence extra damage already meet in text preparation; neither alone covers every resource dependency.
- **Zero-work claim is scoped to unchanged overlays:** not zero total prepare cost, zero draw calls, or a redesign of ordinary/Sequence text rendering.
- **No additional product assumptions or blocking decisions:** private names, buffer allocation and dependency bookkeeping are left to implementation within this contract.
- **Selected endpoint: short Spec only.** No tickets, implementation plan, code changes, test execution or runtime/performance acceptance are included. No implementation progress is asserted by this Draft.

## Implementation verification — 2026-10-30

The owner subsequently invoked `grimoire-loop`, releasing implementation and verification beyond the original spec-only refinement endpoint. The retained row-local cell overlays, independent IME cache, dependency invalidation and shared-source-row reuse are implemented. Focused work assertions and complete-reference comparisons cover R1–R5; the verification summary is retained here rather than in a separate developer report.

Workspace/core tests, strict workspace Clippy and formatting pass. The final renderer suite passes all 130 tests on both native Vulkan and DX12 WARP, including seven new focused tests. These establish this scoped implementation contract, not whole-prepare zero cost, a measured speedup or fresh interactive desktop/ConPTY acceptance.

Focused coverage is in [`render/text_tests/overlay.rs`](../../crates/harbor-terminal/src/render/text_tests/overlay.rs): warm empty/populated fixtures at 8x80, 80x80 and 240x80 assert zero overlay inspections, rebuilds and writes; local ordinary/mark edits stay row-local; IME text/cursor/cancellation changes remain independent. Palette/geometry/metrics, glyph admission/repacking, Sequence suppression outside content damage and deferred/structural updates are checked against fresh complete-reference pixels. The 130-test results above are the historical 2026-10-30 record, not the size of the current suite. No latency, speedup, process-memory or display-present measurement was made.
