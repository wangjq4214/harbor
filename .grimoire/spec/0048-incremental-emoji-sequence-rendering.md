# Incremental Emoji / Sequence Rendering

**Spec ID:** 0048
**Status:** Draft
**Date:** 2026-10-29
**Sources:** Owner request for row/cell-level Sequence dirty tracking and subsequent approval of the proposed scope: dirty-row granularity is sufficient; both CPU projection reuse and Sequence GPU partial uploads are required; genuine global invalidation may rebuild fully; deliver a short Spec without tickets or implementation. [ADR 0098](../adr/0098-incremental-sequence-presentation.md) records the architectural direction.

## Requirements

### R1 — Retain the Sequence projection

With stable geometry and resource generations, reuse unchanged row/cell Sequence classification, request associations and presentation geometry rather than clearing all retained state and scanning the entire visible `TerminalSnapshot` on every content update. A bounded edit must inspect/rebuild only its affected rows/cells and any genuinely invalidated dependencies, not all visible cells merely to rediscover unchanged emoji.

Dirty-row granularity is allowed: rebuilding a touched row does not require strict per-cell change detection. Unaffected rows must remain reusable. Repeated preparation without relevant changes must not rebuild or upload the retained Sequence geometry. Existing native presentation-cache hits must continue to avoid repeated shaping/rasterization under the cache contract of [spec 0020](./0020-color-emoji-sequence-presentation.md).

### R2 — Keep uploads incremental across both layers

Persistent visible emoji alone must not set full ordinary-text dirtiness. For a bounded ordinary-text edit with stable Sequence presentation and placements, update ordinary-text vertex ranges at the affected granularity and retain Sequence data outside that granularity. Emoji on untouched rows must cause no Sequence geometry rebuilding or vertex uploading.

Sequence additions, replacements and removals must support partial Sequence vertex updates without rebuilding/uploading all unaffected rows. Cover both complete-presentation tiles and the missing-cue path; caching native raster tiles while rewriting the entire Sequence vertex batch does not satisfy this requirement. Rebuilding/uploading a touched row is permitted. Clear removed geometry and restore or suppress ordinary scalar paint where the actual presentation requires it.

This requires retained CPU projection reuse and GPU partial uploads, not only removal of the current `had_emoji` full-dirty guard. Exact retained storage, GPU buffer layout and allocation strategy remain implementation choices.

### R3 — Invalidate actual dependencies coherently

Track content and resource dependencies sufficiently to avoid stale classification, raster appearance, placement, fallback or geometry. Relevant inputs include complete retained text and suffixes, style/conceal, foreground-dependent appearance, assigned geometry, font/presentation generation, effective DPI/raster scale and atlas placements, under spec 0020's existing identity rules.

Atlas repacking, eviction/retry and complete-tile / scalar-fallback / missing-cue transitions may invalidate references outside the incoming content dirty range. Refresh every affected reference before drawing. Placement-only changes must not, by themselves, force unrelated ordinary-text rows to upload; changes to Sequence suppression/fallback must refresh the ordinary-text ranges that actually depend on them. Preserve scalar-atlas repack correctness under [spec 0047](./0047-grid-resize-glyph-atlas-reuse.md).

Initial construction, resize and genuine global resource invalidation may perform necessary complete reconstruction. These exceptions must not turn ordinary bounded edits with stable resources into full reconstruction. Geometry/viewport, font, palette and DPI transitions must remain correct even without unrelated later terminal output.

### R4 — Preserve presentation, bounds and ownership

Keep tracking and presentation resources in terminal rendering, not engine cells or the renderer-update domain contract ([ADR 0045](../adr/0045-gpu-independent-terminal-core-boundary.md), ADR 0098). Preserve the bounded, lazy color atlas and bounded native/negative-result caches; retained projection data must not grow without bound with historical edits or sequences.

Preserve complete color/monochrome presentation, leading-pictograph fallback and visible missing cues; assigned-cell fitting/clipping; exact source/copy and width semantics; fragmented suffix/ZWJ updates; continuation cells; conceal; decorations; combining/isolated marks; IME; and existing neighboring-cell behavior from spec 0020. No stale pixels, duplicated scalar/Sequence paint or obsolete UVs may remain. Incremental output must converge to a fresh complete preparation of the same current projection.

Preserve coherent update acknowledgement and pending damage after skipped/failed preparation ([spec 0045](./0045-mutation-revisions-and-incremental-terminal-updates.md)). Scroll/review, primary/alternate transitions and resumed drawing must not reuse stale row coordinates or wrong-buffer presentation.

## Solution and integration contracts

Retain row/cell Sequence projection state and derive each layer's damage from actual changed content and resource dependencies. Keep native raster reuse, atlas synchronization and retained vertex updates distinct; a cache hit at one stage does not establish that later stages performed no work.

| Seam | Exchange | Required guarantee |
| --- | --- | --- |
| Coherent terminal update → Sequence projection | Current cells, dimensions and accumulated damage | Reuse unchanged rows; identify changed complete units without consuming engine damage |
| Font/scalar/color resources → retained projection | Presentation generation, current outcomes and valid placements | Refresh all affected references, including references beyond content damage; preserve bounded fallback/retry behavior |
| Sequence projection → ordinary text preparation | Current tile/missing-cue suppression and scalar-fallback dependencies | Refresh affected scalar ranges without emoji-presence full dirtiness |
| Retained projection → GPU buffers/textures | Changed vertex regions, added tiles or changed atlas layout | Partial steady-state vertex writes; coherent texture/UV synchronization and removal of obsolete geometry |

Verified source surfaces are [`render/sequence.rs`](../../crates/harbor-terminal/src/render/sequence.rs), [`render/text.rs`](../../crates/harbor-terminal/src/render/text.rs) and [`render/color_atlas.rs`](../../crates/harbor-terminal/src/render/color_atlas.rs). At refinement, Sequence preparation clears its cell sets and reconstructs complete batches; text preparation promotes emoji-containing updates to full dirty. These observations identify the costs to remove, not mandatory private APIs or implementation steps.

## Input-to-outcome verification

| Scenario | Given / when | Required result |
| --- | --- | --- |
| Warm unchanged projection | Prepare cached emoji and repeat with no relevant changes | Identical presentation; no Sequence geometry rebuilding or vertex upload (R1/R2) |
| Ordinary edits beside persistent emoji | Keep warm emoji on untouched rows; modify a bounded ordinary-text region repeatedly | No full Snapshot classification scan; Sequence rows retained; ordinary-text uploads remain local rather than full because emoji exist (R1/R2) |
| Local Sequence churn | Add, replace and erase emoji on one row while other Sequence rows remain unchanged; keep resources stable and capacity sufficient | Only affected row/cell geometry updated/uploaded; old paint removed; untouched rows retained; output matches complete reference (R2/R4) |
| Fragmented units and fallback | Extend a drawn unit with VS16/ZWJ suffixes; exercise complete tile, usable scalar and missing-cue outcomes | Correct replacement and suppression, no stale/duplicate paint; width/source/copy unchanged (R2–R4) |
| Atlas pressure and outcome changes | Repack or release bounded capacity so placements or fallback outcomes change beyond the incoming dirty span | Every dependent reference refreshed in the same preparation; no stale UVs; unrelated scalar rows not made dirty solely by color-atlas movement (R3) |
| Resource/geometry changes | Change font generation, DPI, palette or viewport; resize with different row strides | Correct current presentation without requiring later output; complete rebuilding only where actual invalidation requires it; scalar-atlas reuse contract preserved (R3/R4) |
| Structural and deferred rendering | Scroll/review, switch primary/alternate buffers, or accumulate edits while preparation is skipped/failed, then resume | Current complete-reference-equivalent presentation; no wrong-coordinate/buffer reuse; damage acknowledged only after safe current preparation (R4) |

## Evidence and limits

Use deterministic work observations for inspected/classified cells or rows, rebuilt geometry, native presentation/raster calls, atlas repacks/texture uploads, and ordinary/Sequence vertex-write ranges and bytes. Test both tile and missing-cue geometry and include cache churn/overflow so reuse does not hide unbounded retention. Observations may be test-only; this contract does not require a new public metrics API.

Compare fixed bounded edits across different viewport sizes, including persistent emoji outside the dirty rows. Separate unchanged preparation, ordinary edits, local Sequence changes and genuine reconstruction. Compare output with a fresh complete reference, including unchanged rows affected indirectly by a repack or fallback transition. Record reproducible before/after work counts and measurements with workload, grid size, build/features, font/DPI/platform and revision/dirty scope. Preserve applicable Windows/native-GPU regression evidence from spec 0020; label actual checks PASS / FAIL / NOT RUN / BLOCKED.

No numerical speedup or latency threshold is selected. Reduced Sequence work does not promise constant-time whole frames or eliminate unrelated full-layer work.

## Decisions and endpoint

- **Row granularity allowed, CPU and GPU both covered:** explicitly owner-approved refinement; ADR 0098.
- **Necessary complete rebuilding allowed:** owner-approved for resize/global invalidation; actual placement/outcome dependencies still govern local invalidation.
- **Existing presentation and engine contracts preserved:** specs 0020, 0045 and 0047; no new shaping backend, text-width policy, widget-text redesign or engine storage rewrite is selected.
- **Spec only:** one coordinated outcome with a continuing performance/correctness contract; no ticket decomposition or implementation plan is selected.

No additional product assumptions or blocking requirements decisions remain. This document defines future acceptance, not implementation evidence. No production edits, tests, performance runs or runtime acceptance were performed by this refinement.

## Implementation verification

Retained row projection, actual resource-dependency invalidation, row-local Sequence GPU buffers and scalar-suppression damage merging are implemented. [Verification record](../../docs/developer/incremental-sequence-rendering.md) maps R1–R4 to work assertions, native/GPU reference comparisons, before/after measurements and current quality-gate results.

Automated renderer checks pass on native Vulkan and DX12 WARP; workspace/core tests, strict Clippy and formatting pass. A separately enabled ignored calibration experiment fails identically on the untouched baseline. Fresh interactive screenshot/clipboard/ConPTY acceptance was not run; this appendix does not promote Draft status or claim full desktop acceptance.
