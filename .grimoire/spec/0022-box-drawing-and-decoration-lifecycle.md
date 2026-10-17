# Box Drawing and Decoration Lifecycle

**Spec ID:** 0022
**Status:** Implementation present; integrated acceptance unverified
**Date:** 2026-10-11

**Stable execution aliases:** 0016/T0001 = procedural boxes (R1/R2/R4); T0002 = decoration metric lifecycle (R3/R4); T0003 = combined desktop acceptance (R5 and Definition of Done), including source/test comments and historical evidence labels referring to T0003.
**Sources:** [Issue #193](https://github.com/wangjq4214/harbor/issues/193), the user-approved refinement contract below, [ADR 0056](../adr/0056-terminal-owned-procedural-box-drawing.md), [modern SGR contract](./0018-modern-sgr-decorations.md), and [Validation Policy](../../docs/validation.md).

## Scope and Settled Contract

Deliver seamless terminal box drawing for the complete **U+2500–U+257F Box Drawing** block and correct underline, strikethrough, and overline presentation across metric/resource and viewport/DPI transitions. The user approved terminal-owned procedural geometry, the complete block, native font-resource replacement smoke without adding user-configuration hot reload, and the **Spec -> slice** endpoint on 2026-10-11.

Historical acceptance records retain their original scope; NOT RUN cases are not passes.

The native font item is deliberately scoped to Windows DirectWrite font-resource replacement through the existing renderer seam, with an actual metrics change. It is not a claim that a running Harbor desktop session supports user-triggered configuration reload. Actual cross-monitor DPI smoke remains required; unavailable conditions must be recorded truthfully and any deferral must be an explicit scope decision, not inferred from passing simulated tests.

## Requirements

### R1. Complete box-drawing presentation and joins

- Provide procedural terminal presentation for every character in U+2500–U+257F, including light/heavy and mixed strokes, single/double combinations, horizontal/vertical dashed forms, corners/junctions, rounded corners, diagonals, and half-line forms. Block Elements and Braille are not included.
- Compatible connecting strokes must meet at adjacent cell edges without unintended horizontal or vertical seams. Preserve the source character's intended arm directions, stroke distinctions, endpoints, and intentional dashed gaps; do not invent a connection through a space, concealed cell, or incompatible endpoint.
- Base geometry and connecting endpoints on consistent cell-boundary and pixel-alignment rules. Fractional cell dimensions and allocation origins must not create independently rounded gaps between compatible cells.
- Keep terminal strokes sharp and high-contrast. Do not apply UI-style smoothing or font-bitmap stretching as the seam repair.
- Cell geometry depends on its own character and viewport, not adjacent content. Continuous runs do not require cross-cell shaping, retained run caches, or neighbor-driven damage expansion.

### R2. Presentation ownership and existing semantics

- Keep procedural box drawing and its GPU resources in `harbor-terminal`. Do not globally override `harbor-text::FontBook`, the shared scalar atlas, or widget text rendering. Preserve the ordinary text/emoji/combining/preedit paths outside this selected presentation.
- Retain authoritative characters, suffix text, assigned grid width, cursor progression, selection, and copied content. Do not add glyph geometry or GPU state to the terminal core/update model; preserve [ADR 0045](../adr/0045-gpu-independent-terminal-core-boundary.md).
- Procedural strokes use existing effective foreground/palette and inverse semantics and are suppressed by conceal. Preserve backgrounds, clipping, and established decoration/selection/cursor presentation order. Do not also paint the replaced scalar box glyph over its procedural presentation.
- Keep box geometry in a separate, bounded fixed-slot per-cell layout. Existing underline slot sizing, `UNDERLINE_VERTICES_PER_CELL = 16 * 6`, and underline upload offsets remain unchanged.
- Initial rendering, single-cell character/attribute edits, erase, conceal/reveal, palette changes, viewport changes, resize, and font-resource replacement must refresh or clear obsolete procedural geometry. Full and incremental rendering must agree. Hidden/skipped frames must not consume pending damage prematurely.

### R3. Decoration metric and viewport invariants

- Underline, strikethrough, and overline must derive their geometry consistently from the current `TextMetrics` and viewport cell geometry. Relevant offsets and thicknesses must obey the metric-to-viewport projection contract rather than combining scaled underline/overline with an unscaled strikethrough.
- Preserve each decoration's established placement meaning and row-local clipping. Valid metric changes must keep decoration aligned with the corresponding glyphs and avoid unintended stretching, row overflow, or clipping caused by stale geometry.
- Preserve [ADR 0051](../adr/0051-modern-sgr-decoration-and-blank-retention-policy.md) and spec 0018: all explicit underline styles/colors, wide-cell slots, decorated spaces, non-space OSC 8 fallback, effective colors, conceal, and independent resets remain unchanged. Do not expand strikethrough-on-space behavior.
- Font replacement must prepare new native/atlas/render resources and metrics coherently before publishing the replacement. A failed replacement must not expose partially updated resources or invalidate the last valid presentation. A successful metric change must refresh dependent cell geometry and decoration even without new engine text damage.
- Preserve the current physical-font-size policy for DPI-only transitions. Do not multiply ordinary glyphs or decoration by the system scale a second time. Viewport allocation, origin, surface, and dependent presentation changes still require the appropriate projection invalidation.

### R4. Degenerate inputs and bounded updates

- Exercise zero, negative, and non-finite relevant metrics, non-drawable surface/allocation conditions, and extremely small/large scale factors, as well as ordinary supported scales. Invalid inputs must not contaminate retained valid resources, produce non-finite vertices, panic from invalid geometry bounds, or cause unbounded geometry work.
- A rejected transition must preserve the last valid resource state. Geometry that cannot be validly drawn must remain bounded and safe rather than being passed to the GPU as malformed coordinates. This contract does not select a new universal fallback font, arbitrary scale limit, or numerical tolerance.
- Preserve fixed-slot capacity and dirty-range byte bounds during resize, including changes to row stride without growth in total cell count. Geometry/size/projection changes must force sufficient rebuilding so old ranges, offsets, or placements cannot be reused against incompatible allocations.
- Acknowledgment of coherent engine updates remains separate from source inspection or skipped drawing. Incremental and complete uploads must converge to the same rendered state after relevant transitions.

### R5. Windows evidence and completion honesty

- Produce a self-contained evidence record following `docs/validation.md`, identifying revision/dirty scope, Windows and application versions, font settings, viewport/scale, reproducible commands or manual steps, expected/observed results, and individual PASS/FAIL/NOT RUN/BLOCKED outcomes.
- Run a named Windows application workload with box-drawing borders, not only a synthetic CPU fixture. Include relevant redraw/resize observations and explicitly identify the application and transport exercised.
- Run Windows native font-resource replacement smoke with DirectWrite and actual GPU rendering: replace the font resources with a configuration that changes metrics and observe box drawing, ordinary glyphs, and all three decoration kinds before/after. Distinguish this from restarting with a different startup font, reloading the same metrics, simulated geometry, and desktop user-settings hot reload.
- Perform actual cross-monitor window moves between displays with differing DPI/scale and observe the same presentation before/after. A static 1.5x display observation, offscreen scale mutation, or GPU unit test is not that runtime evidence.
- If an evidence case cannot be run, record why and preserve NOT RUN/BLOCKED. The issue permits explicit deferral, but the user has not authorized blanket deferral of the required native-resource or cross-monitor cases. Obtain and record the missing case's explicit scope decision before claiming completion by deferral. A failed case remains a defect/blocker, not a pass or an implicit deferral.
- Record fresh results for all standard quality gates listed below. Existing historical passes and configured CI jobs do not establish a new execution pass.

## Acceptance status

[Box CPU/GPU evidence](../../docs/procedural-box-drawing-evidence.md) and [decoration/native changed-metrics evidence](../../docs/decoration-metric-lifecycle-evidence.md) establish focused implementation, not integrated desktop acceptance. Historical 0016/T0003 was marked Done on user-reported verification with no apparent issues, but supplied no tested revision, environment, named application, native replacement/cross-monitor procedures or fresh gate results. Its detailed acceptance remained unchecked. R5 and the definition below remain required: no implicit deferral, broader support or #193 closure follows from that report. Preserve historical evidence; summaries must be self-contained, without local-only raw-artifact links.

## Definition of Done

- [ ] R1-R4 have focused CPU geometry and practical GPU encode/readback coverage, including joins, local updates, metric changes, degenerate inputs, and resize bounds.
- [ ] Existing ordinary text, emoji/combining/preedit, SGR/OSC 8/conceal, source/copy/width, and coherent damage/invalidation behavior remain intact within affected regression coverage.
- [ ] R5 has named Windows workload, native changed-metrics resource replacement, and actual cross-monitor records, or an explicitly authorized and documented deferral for each unavailable required case. No FAIL is presented as completion.
- [ ] Fresh standard quality results and evidence limitations are recorded; documentation claims only implemented and actually verified behavior.

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
python scripts/docs/check_docs.py
python scripts/docs/checklist_summary.py
```

## Out of Scope

- Block Elements, Braille, a new Unicode width policy, retained run shaping, or replacement of ordinary/widget text rendering.
- UI-style smoothing, font-bitmap stretching as seam repair, or changes to the fixed underline slot size.
- New SGR semantics, strikethrough-on-space expansion, copy/reflow policy changes, or broader protocol/identity work.
- User-configuration/font-settings hot reload, a font-management redesign, and DPI-driven enlargement of ordinary terminal physical font size.
- Universal font/platform/transport certification, unrelated #162/N11 work, or promotion of historical NOT RUN evidence into a new pass.
