# Procedural Box Drawing

**Ticket ID:** T0001
**Source:** [Spec 0022 R1, R2 and box-specific R4](../../spec/0022-box-drawing-and-decoration-lifecycle.md), [ADR 0056](../../adr/0056-terminal-owned-procedural-box-drawing.md), [Issue #193](https://github.com/wangjq4214/harbor/issues/193).
**Status:** Done

## Goal

All U+2500–U+257F characters receive terminal-local procedural presentation. Compatible border strokes join without unintended seams, preserve intended forms/gaps, and remain correct under local edits and retained projection changes.

## Affected Surfaces

- `crates/harbor-terminal/src/render/`: terminal-local character/geometry/resource code and integration in `text.rs`, `pipeline.rs`, `mod.rs`, and existing geometry/upload helpers as needed.
- Focused CPU/GPU render coverage for the complete repertoire, joins and incremental projection.
- Existing `harbor-text` scalar font/atlas and ordinary terminal text contracts are preserved, not globally replaced.
- Engine snapshot/damage, source text, width and copy are inputs/regression contracts, not new GPU-bearing model fields.

## Approach

Use the approved character-local geometry and consistent grid-edge/pixel-alignment direction. Keep box resources separately bounded with fixed cell slots and suppress duplicate scalar paint for replaced box glyphs. Reuse effective palette/inverse/conceal/clipping behavior and coherent render invalidation. Private tessellation, buffer slot capacity, and helper organization are not prescribed here.

## Dependencies and Coordination

- **Blocked by:** None.
- **Blocks:** T0003 final application/combined acceptance needs the procedural path.
- **Coordination:** T0002 may touch `pipeline.rs`, viewport helpers and retained tests; sequence edits or coordinate ownership. Keep box and underline buffer layouts independent.

## Acceptance

- [ ] Focused repertoire assertions cover all 128 characters and preserve light/heavy/mixed strokes, single/double combinations, dashed forms, junctions, rounded corners, diagonals and half-lines.
- [ ] CPU edge geometry and actual GPU encode/readback prove compatible horizontal/vertical joins and junctions without unintended holes, including fractional cell dimensions and origins. Intentional dash gaps and endpoints remain visible.
- [ ] Strokes remain sharp/high-contrast without UI-style smoothing or font-bitmap stretching.
- [ ] Ordinary text/widget font paths and authoritative source, suffixes, width/cursor and copied characters remain unchanged; replaced box glyphs do not paint twice. Foreground/palette, inverse, conceal and clipping follow existing behavior.
- [ ] Local character/attribute changes, erase, conceal/reveal and palette changes clear stale strokes; incremental rendering agrees with a full rebuild.
- [ ] Viewport/resize/font-resource changes invalidate box projection sufficiently; skipped/hidden draws preserve pending damage and resume with current geometry.
- [ ] Zero/non-finite relevant geometry, extreme scale/size conditions, and non-drawable targets cannot generate malformed uploaded vertices, invalid-bound panics, unbounded geometry work, or out-of-bounds writes.
- [ ] Resize coverage includes shrink/grow and row-stride changes without total-cell growth; slot counts, range offsets and upload bounds remain valid.
- [ ] Existing `UNDERLINE_VERTICES_PER_CELL = 16 * 6` and underline offsets are unchanged; box resources use their own bounded slots.
- [ ] Focused test commands, actual results and unavailable GPU cases are reported honestly for T0003 integration; no skipped GPU case is labeled a pass.

## Out of Scope

Block Elements/Braille, ordinary text shaping, a new terminal width model, neighbor-driven run caches, global font/atlas overrides, new SGR/copy/reflow semantics, user-settings hot reload, or final Windows acceptance claims owned by T0003.
