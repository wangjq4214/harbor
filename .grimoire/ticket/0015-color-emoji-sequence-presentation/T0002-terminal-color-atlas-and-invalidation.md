# Terminal Color Atlas and Invalidation

**Ticket ID:** T0002
**Source:** [Spec 0020](../../spec/0020-color-emoji-sequence-presentation.md), R1-R7 renderer obligations; [ADR 0054](../../adr/0054-color-emoji-offscreen-presentation.md); [ADR 0045](../../adr/0045-gpu-independent-terminal-core-boundary.md)
**Status:** Done

## Goal

The terminal renderer draws complete supported emoji tiles and bounded visible fallback, fits artwork inside authoritative cells, and replaces stale presentation on source/resource changes. Atlas packing, cache invalidation, geometry and dirty updates form one coherent deliverable.

## Affected Surfaces

- `crates/harbor-text/src/atlas.rs` or adjacent presentation storage used by the terminal; preserve scalar coverage API compatibility.
- `crates/harbor-terminal/src/render/text.rs`, text vertex/upload helpers and `render/pipeline.rs`.
- Actual font/DPI/appearance transition integration in `crates/harbor-app/src/terminal_view.rs` and renderer-host paths as needed.
- Focused CPU/GPU geometry, atlas/repack, damage and existing overlay regression tests; retained `Cell`/snapshot/update contracts remain authoritative.

## Approach

Consume T0001's whole-unit presentation data for emoji candidates and retain ordinary text's R8 path. Add a separate bounded lazy color atlas and appropriate upload/draw composition. Fit oversized ink by uniform downscaling and position/final-clip within the actual assigned cell rectangle; never advance neighbors using font metrics.

Handle complete color, complete monochrome and explicit unsupported results. Preserve leading-pictograph fallback where usable and a visible missing-glyph cue otherwise. Do not tint fixed artwork with terminal foreground RGB; honor foreground-dependent font layers using the agreed raster/blend contract.

Integrate source/font/DPI/appearance invalidation with current coherent updates and real host transitions. Repack revisions invalidate old UVs and rebuild references. Clear obsolete pixels on streamed extension/overwrite/fallback changes. Keep combining/orphan/preedit overlays compatible and suppress duplicate scalar paint for complete sequence tiles. Do not add a new settings hot-reload feature to meet the resource transition contract.

Capture the simple monospace baseline before editing its fast path; T0003 owns the final comparable evidence. Do not split atlas insertion from its dirty/repack/resource correctness into separately accepted incomplete states.

## Dependencies and Coordination

- **Blocked by:** T0001 for final integrated acceptance: the renderer needs the actual native request/result, fallback and raster-generation contract. Synthetic atlas/geometry test preparation may proceed earlier.
- **Blocks:** T0003's final application and after-change evidence, which must exercise this integrated path.
- **Coordination risks:** Pixel/alpha, result identity and shared `harbor-text` files with T0001. Preserve widget scalar consumers. Coordinate baseline/revisions with T0003 before altering ordinary-text work.

## Acceptance

- [ ] Emoji units use complete source requests; continuation cells are not separate inputs. Ordinary/CJK text is not routed to sequence shaping merely because it is wide.
- [ ] Complete color artwork uses the separate color path with correct partial-alpha composition and no terminal foreground tint; foreground-dependent layers update correctly.
- [ ] Oversized artwork is uniformly downscaled and finally clipped; geometry/encode tests cover native bearings, cell edges and unchanged neighboring cells.
- [ ] Complete monochrome and visible unsupported fallback stay inside assigned cells, without changing `Cell` source/width, cursor geometry or selection/copy data.
- [ ] Ordinary-only output retains the R8 path without allocating the color atlas or performing unnecessary whole-layout shaping; the pre-change monospace baseline is preserved for T0003.
- [ ] Atlas and presentation memory stay within delivered testable bounds under churn/overflow; eviction/repack changes revisions and rebuilds all affected old placements.
- [ ] Fragmented `♥` + VS16 and `👩` + ZWJ + `💻` replace previously drawn units without stale pixels; lead/assigned-area damage, erase/overwrite and full/incremental convergence have focused tests.
- [ ] Actual font-session/fallback/size/DPI and raster-dependent appearance transitions invalidate cached tiles/failures and repaint without text loss; skipped draws retain changes until consumption.
- [ ] Combining marks, isolated-mark cues, conceal, selection, decorations and preedit pass applicable regressions; complete sequence tiles do not duplicate scalar/mark overlays.
- [ ] Native font/shaping/atlas/GPU state remains outside the engine/model contract; existing GPU-independent core and coherent update guarantees are preserved.
- [ ] Focused atlas/dirty/geometry and practical GPU encode results are recorded; skipped adapter/native cases are not reported as passed Windows visual acceptance.

## Out of Scope

New width/copy semantics, general renderer abstraction, broad screen/parser/PTY rewrites, widget emoji support, shared-texture interoperation, settings hot reload, and resolving Spec 0015's historical final-column no-wrap discrepancy.
