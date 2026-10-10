# Native Emoji Sequence Presentation

**Ticket ID:** T0001
**Source:** [Spec 0020](../../spec/0020-color-emoji-sequence-presentation.md), R1/R3/R4/R6/R7; [ADR 0054](../../adr/0054-color-emoji-offscreen-presentation.md); [issue #181](https://github.com/wangjq4214/harbor/issues/181)
**Status:** Todo

## Goal

A complete retained emoji unit can be requested from `harbor-text` and produces capability-scoped complete color/monochrome presentation or an explicit unsupported outcome. DirectWrite shapes the whole source, Direct2D composites it with reused offscreen resources, and native caches are bounded. Existing scalar consumers continue to work.

## Affected Surfaces

- `crates/harbor-text/src/contracts.rs`, `font.rs`, `backend/dwrite.rs` and the crate's public exports/native dependencies as needed.
- Presentation request/result identity, resolved face/run information, raster bounds/pixels, cache generations and backend tests.
- Existing scalar atlas/widget consumers only for compatibility verification; no widget emoji feature.

## Approach

Add a whole-unit request alongside the existing scalar API. Distinguish original source, font session/configuration and resolved fallback, effective size/DPI, style, presentation and raster-dependent appearance/geometry. Keep native types inside the backend and return backend-neutral presentation data.

Use whole-sequence DirectWrite analysis/fallback and reused Direct2D offscreen resources. Detect relevant image-format/API capabilities, including COLR v1 where applicable, rather than treating color-table version or a single output glyph as proof of support. Define the native-to-renderer pixel/color-space/alpha contract in coordination with T0002. Preserve complete monochrome results when available and make unsupported outcomes usable by the renderer's documented fallback path.

Keep resolution/shaping, negative results and CPU raster resources bounded under repeat requests and churn. A replaced font session must not inherit stale session-local face identities or failures. Exact native interfaces, encodings and private limits remain execution details under Spec 0020; return material architecture/acceptance gaps to refinement.

## Dependencies and Coordination

- **Blocked by:** None.
- **Blocks:** T0002's integrated rendering acceptance, because it consumes the actual presentation/tile and invalidation contract.
- **Coordination risks:** Shared `harbor-text` contracts, pixel/alpha representation and cache generations with T0002; preserve existing coverage/scalar consumers. T0003 may prepare fixtures/procedures early but cannot use this backend-only milestone as Windows application acceptance.

## Acceptance

- [ ] Complete requests for `♥️` and `👩‍💻` exercise whole-unit shaping/fallback with a recorded supporting Windows font/version; backend tests distinguish them from scalar-only handling and from unsupported results.
- [ ] Ordinary text, explicit text/emoji presentation and default emoji candidates are distinguished without using two-cell width as the classifier; the scalar monospace path remains available.
- [ ] Relevant actual native formats/API capabilities are identified, with unsupported behavior explicit. Color pixels/single glyph counts alone are not used to certify intended sequence presentation.
- [ ] Complete color and complete monochrome results are distinguishable from unsupported-sequence fallback; missing font/format/native capability is handled without panic or source substitution.
- [ ] Native pixel layout, raster bounds, color space, alpha and current-foreground layer semantics are documented in the result contract and covered by focused composition tests.
- [ ] Repeated warm requests reuse rasterized results and native offscreen resources; tests demonstrate finite shaping/negative/CPU raster resource bounds under churn and invalidation.
- [ ] Font-session/fallback/size/DPI/presentation/style and raster-dependent appearance/geometry changes cannot return incompatible cached results.
- [ ] Existing scalar `FontBook` and coverage-atlas/widget consumers pass applicable regressions; terminal width/copy state and GPU ownership are not moved into the backend contract.
- [ ] Applicable focused test commands/results and unavailable native cases are recorded honestly; this ticket does not claim integrated Harbor/ConPTY visual acceptance.

## Out of Scope

General complex-script shaping, arbitrary ligatures, a new terminal width engine, all-text layout conversion, Direct2D/wgpu shared textures, and application/GPU integration assigned to T0002.
