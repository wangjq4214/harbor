# Decoration Metric Lifecycle

**Ticket ID:** T0002
**Source:** [Spec 0022 R3 and decoration/transition-specific R4](../../spec/0022-box-drawing-and-decoration-lifecycle.md), [ADR 0056](../../adr/0056-terminal-owned-procedural-box-drawing.md), [modern SGR contract](../../spec/0018-modern-sgr-decorations.md), [Issue #193](https://github.com/wangjq4214/harbor/issues/193).
**Status:** Todo

## Goal

Underline, strikethrough and overline remain consistently aligned and row-bounded under changed font metrics and viewport/DPI transitions. Font replacement publishes coherent resources or retains the previous valid presentation.

## Affected Surfaces

- `crates/harbor-terminal/src/render/decoration.rs`, `decoration_tests.rs`, `pipeline.rs`, `layout.rs`, and `text_tests.rs`.
- Existing `TextMetrics`, `replace_fonts`, `sync_viewport`, and `sync_raster_scale` contracts and dependent retained geometry.
- Existing native host/application-bridge transition paths only where required to preserve/verify the established contract; no new settings reload feature.

## Approach

Make decoration projection consistent with current metrics and viewport dimensions, including strikethrough, while preserving all established modern-SGR semantics and fixed underline slots. Reuse the existing replacement seam and physical-font-size DPI policy. Validate genuinely changed metrics rather than reloading the same font configuration. Keep invalid-input handling bounded and preserve valid resources when rejecting a transition.

## Dependencies and Coordination

- **Blocked by:** None; decoration/metric tests can use existing ordinary text without T0001.
- **Blocks:** T0003 final native font/decoration/DPI and combined-tree acceptance needs these invariants.
- **Coordination:** Shared `pipeline.rs` and viewport/resource tests with T0001. Avoid duplicate guards or incompatible cell-edge changes; shared files do not create a semantic dependency.

## Acceptance

- [ ] All three decoration kinds apply the established metric-to-viewport projection consistently and remain aligned/inside their rows under valid offset, thickness, font metrics, fractional viewport/origin and scale changes.
- [ ] All explicit underline styles/colors, spaces and wide slots, OSC 8 fallback, inverse, conceal, overline and independent resets retain spec 0018 behavior; strikethrough-on-space is not expanded.
- [ ] Fixed 96-vertex underline slots and their bounded offsets remain unchanged. Full and incremental decoration results match after dirty edits and resize, including row-stride changes without cell-count growth.
- [ ] DirectWrite/native replacement tests use demonstrably changed metrics; successful replacement refreshes current text/cell/decoration resources even without new engine text damage.
- [ ] Failure of resource preparation/replacement does not publish partial metrics/atlas/pipeline state or discard the previous valid presentation.
- [ ] DPI-only scale updates preserve ordinary terminal physical font size and avoid double scaling; actual allocation/origin/surface changes still invalidate stale projection and retained geometry.
- [ ] Zero/negative/non-finite relevant metrics, extreme scales, and non-drawable targets remain safe: no non-finite uploaded vertices, invalid-bound panic, unbounded work or offset/capacity violation; rejected transitions retain valid state.
- [ ] Skipped/hidden rendering and projection/font/scale changes preserve coherent damage until safely consumed.
- [ ] CPU geometry and practical GPU encode/readback coverage distinguish metric changes from same-metrics session replacement and scale-only updates. Record executed commands/results and NOT RUN/BLOCKED cases for T0003 without claiming native desktop acceptance.

## Out of Scope

User-configuration hot reload, a font-management redesign, DPI enlargement of ordinary terminal physical font size, new SGR or retention semantics, box repertoire implementation owned by T0001, or substitution of simulated scale tests for cross-monitor evidence.
