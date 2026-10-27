# Grid Resize Glyph Atlas Reuse

**Spec ID:** 0047
**Status:** Draft
**Date:** 2026-10-27
**Sources:** Owner-approved refinement on 2026-10-27: ordinary resize reuses the terminal scalar atlas; missing glyphs are added incrementally; capacity pressure permits active-set reclamation/repacking. The owner selected a short Spec without tickets or implementation. [ADR 0097](../adr/0097-grid-resize-preserves-scalar-glyph-atlas.md) records the settled architectural choice.

## Requirements

### R1 — Separate grid geometry from glyph-cache validity

For an existing terminal renderer with unchanged valid font resources and physical font size, a row/column change alone must not clear/rebuild the scalar glyph atlas, rerasterize cached glyphs, or upload the full scalar atlas texture. If all required scalar glyphs are cached, resize must perform zero scalar glyph rasterizations and zero scalar atlas uploads, preserving their placements/UVs.

Grid-dependent vertices may still require complete rebuilding and sufficient buffer capacity. A changed row stride must be handled even when total cell count is unchanged or decreases. This requirement removes atlas work, not all full-grid or resize work.

### R2 — Cover newly visible content and preedit

Determine the required scalar glyphs from the entire coherent new visible projection and current IME preedit, including the existing combining/suffix paint inputs. Old-geometry dirty ranges alone are insufficient. When required glyphs are missing and packing has room, rasterize only missing glyphs and upload only their new tiles; cached glyph placements/UVs remain unchanged.

### R3 — Reclaim on pressure, not on every resize

Keep cached glyphs in the ordinary no-pressure path. When capacity requires reclamation/repacking, use the current visible projection plus preedit as the active scalar working set rather than retaining every historical atlas entry. Stale entries must not crowd out a current working set that fits after repacking.

A genuine repack may rebuild the atlas and upload the complete scalar texture. All geometry dependent on changed placements, including otherwise unchanged cells and preedit, must be refreshed before drawing. This must remain correct when the shared terminal atlas encounters pressure during ordinary incremental output as well as resize; recovery must not rely on another resize or unrelated child output.

This does not guarantee that an arbitrary active working set fits the bounded atlas or select new behavior for intrinsically un-packable working sets. It must not require unbounded atlas growth to avoid repacking.

### R4 — Preserve existing ownership and presentation contracts

Keep atlas/GPU decisions in terminal presentation, not engine cells or renderer updates. Preserve coherent preparation and explicit acknowledgement: skipped/failed preparation cannot consume pending changes, and resumed rendering must converge to the current projection ([ADR 0045](../adr/0045-gpu-independent-terminal-core-boundary.md), [spec 0045](./0045-mutation-revisions-and-incremental-terminal-updates.md)).

Preserve wide/combining text, IME, selection/copy, reflow and primary/alternate-screen behavior. Ordinary text and Nerd-icon presentation remain unchanged, as do the separate emoji color-atlas/fallback policy and widget text caching ([ADR 0054](../adr/0054-color-emoji-offscreen-presentation.md), [ADR 0093](../adr/0093-natural-overflow-nerd-font-icons.md)).

Genuine font-resource or physical-font-size changes retain coherent resource replacement and failure preservation. DPI-only transitions retain the established ordinary physical-font-size policy; this feature must not add a second scaling factor or invalidate scalar bitmaps merely because grid dimensions changed ([spec 0022](./0022-box-drawing-and-decoration-lifecycle.md), [ADR 0060](../adr/0060-font-reload-uses-normal-bounded-resize.md)). Existing PTY ordering and bounded multi-tab scheduling are unchanged ([ADR 0096](../adr/0096-bounded-multi-tab-resize-scheduling.md)).

## Solution and necessary seams

Reuse the terminal's existing scalar cache independently of geometry invalidation. Ordinary additions remain incremental; capacity pressure admits active-set reclamation and a coherent full GPU/geometry refresh. No new public API, packing algorithm or threading model is selected.

| Seam | Exchange | Required guarantee |
| --- | --- | --- |
| Coherent update → terminal text preparation | Current dimensions, visible paint inputs, damage and preedit | Complete new-geometry working set; engine remains read-only and GPU-independent |
| Terminal text preparation → scalar atlas | Required glyphs and current active set for pressure recovery | Cache hits retain placement; missing glyphs append when possible; stale entries are reclaimable under pressure |
| CPU atlas → GPU texture and geometry | Added tiles or a repacked layout | No upload on cache hit; tile uploads on ordinary additions; full synchronization and no stale UVs after repack |
| Font/viewport lifecycle → retained renderer | Valid font resources, physical metrics and geometry transitions | Genuine resource invalidation stays distinct from grid-only change; failed replacement preserves valid presentation |

Verified integration surfaces at refinement: [`render/text.rs`](../../crates/harbor-terminal/src/render/text.rs) unconditionally rebuilds/uploads in its resize branch, but already has incremental raster-result/GPU synchronization for ordinary updates; [`harbor-text/src/atlas.rs`](../../crates/harbor-text/src/atlas.rs) offers incremental and retained-key repacking, with its ordinary entry retaining all stored keys; [`render/pipeline.rs`](../../crates/harbor-terminal/src/render/pipeline.rs) coordinates coherent projection, preedit and detached font replacement. These are source observations, not mandatory edit steps.

## Input-to-outcome verification

| Scenario | Given / when | Required result |
| --- | --- | --- |
| E1 — Cached resize burst | Warm all required glyphs, then repeatedly grow/shrink rows and columns without font changes or new glyphs | Correct new layout; zero scalar rasterizations and scalar atlas uploads throughout the burst; cached UVs stable (R1) |
| E2 — Newly visible text | Resize exposes uncached history/content, including scalar suffixes, with sufficient free packing space | Only missing glyphs rasterized and new tiles uploaded; existing UVs stable; complete correct visible projection (R2) |
| E3 — Pressure and stale cache | Accumulate now-invisible glyphs until an insertion requires repacking; resize to a current working set that fits after reclamation | Stale entries reclaimed; required visible/preedit glyphs retained/admitted; any full upload and geometry refresh coherent (R3) |
| E4 — Incremental pressure | Trigger pressure with a local edit/preedit change while other visible cells remain unchanged | Unchanged visible cells remain correct after repack, as does preedit; no stale UVs or dependency on later output/resize (R3) |
| E5 — Row-stride changes | Resize between different row/column shapes with equal or decreasing cell count | Correct indexing, geometry and buffer writes; no atlas rebuild caused only by changed shape (R1/R2) |
| E6 — Unicode/overlay and buffer regression | Resize with wide text, combining suffixes, Nerd icons, emoji and IME; switch primary/alternate buffers and revisit prior content | Current presentation matches a fresh complete reference under existing policies; no source/copy/width changes or duplicate/stale overlay paint (R2/R4) |
| E7 — Resource transitions | Exercise valid changed-font/size replacement, rejected replacement, and DPI/viewport-only transitions | Coherent new resources on success, old valid state on rejection; existing physical-size and emoji policies hold (R4) |
| E8 — Skipped/failed preparation | Accumulate resize/content changes while hidden/skipped, or reject preparation, then resume | Pending updates survive; current projection converges and is acknowledged only when safely prepared (R4) |

## Evidence and limits

Verify scalar rasterization counts, atlas repack counts, scalar texture upload kind/bytes and cached placements in focused integration coverage. Do not count necessary vertex-buffer writes or independent emoji color-atlas work as scalar atlas uploads. Use controlled pressure fixtures and a current working set demonstrably packable after reclamation, rather than relying on platform-dependent timing to trigger overflow.

Compare rendered output with a fresh complete reference, including ordinary cells outside the immediate dirty range after repack. Record before/after cached-resize measurements and work counts with grid sizes, content, font settings, build/features, environment and revision/dirty scope. No numerical speedup or latency threshold is selected; avoiding the identified atlas work does not establish a whole-frame responsiveness guarantee. Include focused Windows DirectWrite/GPU resize and preedit observations for the affected path; label evidence PASS / FAIL / NOT RUN / BLOCKED honestly.

## Decisions, assumptions and endpoint

- **Reuse across grid-only change:** owner-approved refinement and ADR 0097; geometry changes do not invalidate unchanged scalar bitmaps.
- **Pressure-driven current-visible-plus-preedit reclamation:** explicitly approved with reuse; avoids losing the old resize path's stale-entry reclamation benefit.
- **Allow full repack and geometry reconstruction:** approved capacity exception; not an attempt to eliminate every full rebuild.
- **Spec-only:** one bounded outcome with a continuing performance/correctness contract; no ticket decomposition adds value at this endpoint.

No additional product assumptions or blocking requirements decisions remain. Concrete APIs, pressure fixtures and packing details remain implementation choices within this contract. Out of scope are a general font-cache redesign, emoji/widget atlas redesign, changed terminal semantics, worker-thread resize redesign, and a promise that arbitrary active glyph sets fit existing capacity.

This artifact defines future acceptance only. No implementation, tests, performance measurements or runtime acceptance were performed by this refinement; the existing-spec acceptance baseline does not establish acceptance of this new contract.

## Implementation verification

Retained focused tests live in [`render/text_tests.rs`](../../crates/harbor-terminal/src/render/text_tests.rs) and [`harbor-text/src/atlas.rs`](../../crates/harbor-text/src/atlas.rs). The evidence requirements and limits above still apply. At the owner's request, no custom test feature, cross-crate pressure hook or work counters are retained. This does not promote the spec status or claim complete desktop acceptance.
