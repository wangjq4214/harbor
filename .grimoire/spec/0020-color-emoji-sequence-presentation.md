# Color Emoji and Whole Sequence Presentation (#181)

**Spec ID:** 0020
**Status:** Draft
**Date:** 2026-10-10
**Sources:** [Issue #181](https://github.com/wangjq4214/harbor/issues/181); the refinement conversation approving DirectWrite + Direct2D offscreen presentation and bounded uniform downscaling; [ADR 0054](../adr/0054-color-emoji-offscreen-presentation.md).

## Requirements

### R1. Supported whole-sequence presentation

Harbor displays supported color emoji and complete emoji ZWJ sequences when the installed font and Windows presentation capabilities provide them. Acceptance includes `♥️` (U+2665 U+FE0F) and `👩‍💻` (U+1F469 U+200D U+1F4BB) with a named, known supporting Windows font. Shape the complete retained unit, not independently resolved scalars. Ordinary `♥`, explicit text/emoji selectors, default emoji presentation and ZWJ candidates must remain distinguishable; a two-cell width alone is not an emoji classifier.

Distinguish complete color presentation, complete monochrome presentation and unsupported-sequence fallback. A nonempty bitmap, color pixels, a single output glyph, or successful copy alone is not proof of the intended joined presentation. Verify the named supported fixtures visually as well as through focused shaping tests.

### R2. Authoritative text, geometry and ownership

Reuse the `Cell` source text and assigned grid-width contract from [ADR 0044](../adr/0044-unicode-text-unit-and-presentation-width-policy.md) and [Spec 0015](0015-unicode-terminal-text-correctness.md). Preserve exact source scalars, assigned width, cursor/cell geometry, selection/copy and neighboring content. Font advances position glyphs within a unit; they do not advance subsequent terminal cells. No second width engine, new copy representation or renderer-originated model mutation is permitted.

Present oversized emoji artwork by uniform downscaling and positioning within the assigned cell rectangle, followed by final clipping. Do not enlarge its allocation or distort it through independent horizontal/vertical scaling. Unsupported presentation is also bounded to that rectangle. Continuation cells are not separate shaping inputs.

Keep native font handles, shaping output, raster caches, atlas UVs and GPU state outside the terminal model/update contract. Preserve [ADR 0045](../adr/0045-gpu-independent-terminal-core-boundary.md)'s GPU-independent core and coherent snapshot/damage consumption. Hidden/skipped draws must not discard pending presentation updates.

### R3. Native rasterization, color composition and fallback

Use DirectWrite whole-unit shaping/fallback and reused Direct2D offscreen resources to produce presentation tiles. Initially transfer raster pixel data to the wgpu renderer rather than introducing Direct2D/wgpu shared-texture interoperation. The actual resolved fallback face/run, offsets and presentation must belong to the complete source request, not just its first scalar.

Detect relevant native color-format/API capabilities and support the formats provided by the documented supporting font/platform through the chosen backend. Document supported and unsupported formats and platform behavior, including COLR v1 capability where relevant; do not assume COLR v0 covers modern Windows emoji fonts. No universal font/format/OS support is promised.

Fixed-color artwork retains its native colors rather than being tinted with terminal foreground RGB. Font layers that request the current foreground honor that color. The native pixel format, color space, alpha representation, GPU upload and blend behavior must agree, including partially transparent edges.

When complete color presentation is unavailable, preserve complete monochrome presentation where available. Otherwise use the documented leading-pictograph fallback if usable, or a visible missing-glyph cue. Unsupported format, missing glyph and unavailable native capability must not cause invisible content, panic, shifted neighbors, or source-text replacement. Distinguish this fallback from correct whole-sequence rendering in tests and evidence.

### R4. Cache identity, invalidation and resource bounds

The sequence-aware request/cache distinguishes original source text, font session/configuration and resolved fallback face(s), size, style, text/emoji presentation, effective DPI/raster scale, and appearance that affects raster output. Changes to any raster-dependent geometry or color must not reuse incompatible tiles. Session-local face IDs from a replaced font session are not sufficient cross-session identities.

Bound shaping, failed-resolution and CPU raster caches as well as CPU/GPU atlas storage. The separate color atlas is allocated lazily and bounded; ordinary text retains the existing R8 atlas. Numeric limits and private storage layout are execution details, but the delivered implementation must expose testable bounds and remain within them under churn and overflow.

Original text extension/replacement, fallback-font changes, font-size changes, DPI changes and relevant appearance changes invalidate presentation and repaint the affected units. Repacking/eviction must invalidate old atlas placements and rebuild affected geometry; no stale UV, glyph or cached failure may survive a relevant resource generation change. Cache hits must not repeat native rasterization merely because the next frame is drawn.

### R5. Fragmented updates and existing rendering compatibility

Fragmented PTY input can extend a unit already drawn. `♥` followed later by VS16 and `👩` followed later by ZWJ and `💻` replace the previous presentation, including clearing pixels no longer used. Dirty ranges include the unit's lead/assigned area even when an extension has no independent cell advance.

Keep current combining-mark, isolated-mark dotted-circle and transient preedit behavior intact. Do not paint the scalar base/mark overlay a second time for a unit already represented by a complete sequence tile. Existing conceal, clipping, selection, decorations and neighboring-cell behavior remain compatible. Full and incremental updates must converge on the same presentation.

### R6. Ordinary-text fast path and performance evidence

Preserve the current ordinary monospace R8 path rather than sending all terminal text through whole-layout shaping or allocating a color atlas for ordinary-only output. Do not route CJK into the emoji path solely because it is wide. Reuse native offscreen resources rather than recreating a device/context for each emoji.

Capture the simple monospace baseline before modifying its fast path and comparable after-change evidence under the same scenario. Include relevant shaping/raster calls, prepare/presentation cost, upload work and memory; distinguish cold emoji raster/readback from warm-cache draws. Report observations and limitations without inventing a performance pass threshold.

### R7. Focused and Windows evidence, quality gates

Provide focused shaping, resource-bound/atlas, geometry and dirty-range tests for supported and unsupported paths. Windows runtime acceptance must include screenshots and clipboard codepoint evidence for complete supported color sequences and bounded unsupported presentation, plus fragmented input and font/DPI invalidation.

Record revision and dirty-tree scope, OS/ConPTY/font/DPI versions, reproducible commands/steps, expected and observed results, artifacts and exclusions under `docs/verification/`, following [Validation Policy](../../docs/validation.md). Keep raw artifacts local or link retrievable CI artifacts; committed summaries are self-contained and do not link local-only captures. A delayed write alone does not prove distinct PTY reads; identify the actual fragmentation exercised.

Run or account honestly for:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
python scripts/check_docs.py
python scripts/checklist_summary.py
```

Use PASS / FAIL / NOT RUN / BLOCKED for actual checks. Missing Windows/GPU capabilities do not turn skipped runtime checks into passes. Update behavior/support documentation only for implemented and evidenced scope.

## Solution and Necessary Seams

Keep the existing per-scalar path for ordinary text and add a complete emoji-unit presentation path under `harbor-text`. DirectWrite analyzes the full unit with font fallback; Direct2D composites its supported presentation into a cached tile. The terminal renderer consumes backend-neutral presentation data, maintains the separate bounded color atlas, and derives fitted/clipped quads from authoritative cell geometry. This is an extension of the concrete current renderer, not a new terminal renderer or a general-purpose shaping system.

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Retained unit -> presentation request | `TerminalSnapshot` / `Cell` -> terminal text preparation -> `harbor-text` | Complete original text, style, font/session and raster parameters; assigned paint bounds without width recomputation | Sequence-aware request and ordinary-text fast-path selection |
| Native analysis -> tile | DirectWrite shaping/fallback -> reused Direct2D offscreen rasterization | Complete sequence, resolved face/run/offsets, capability and appearance inputs | Complete color/monochrome presentation or explicit bounded-fallback outcome with backend-neutral raster bounds/pixels |
| Tile -> GPU atlas | Presentation resources -> terminal-owned CPU/GPU atlas | Agreed format/alpha/color-space contract and bounded tiles | Color artwork or coverage data, valid placements and upload/repack information |
| Cell projection -> drawing | Assigned cell geometry and atlas placements -> wgpu text geometry/shader | Current atlas revision, authoritative width and clip rectangle | Uniformly fitted/clipped presentation without tinting fixed-color artwork or moving adjacent content |
| Source/resource changes -> damage | Current coherent update and font/DPI/appearance generation -> caches/geometry | Dirty units and relevant resource changes, pending damage retained across skipped draws | Invalidation, removal of obsolete pixels and rebuild when placements change |

### Inspected repository surfaces

- `crates/harbor-terminal/src/model.rs`: `Cell` source/suffix, `grid_width()`, `TerminalSnapshot`, `DirtyRange` and `UpdateDamage`.
- `crates/harbor-text/src/contracts.rs` and `font.rs`: existing face/glyph/size/style identities and the scalar `FontBook` API; the new sequence request must not pretend its identity is one scalar `GlyphKey`.
- `crates/harbor-text/src/backend/dwrite.rs`: scalar resolver/fallback, session-local face registry and coverage rasterization.
- `crates/harbor-text/src/atlas.rs`: coverage pixel store, packing, persistent resolution and eviction result.
- `crates/harbor-terminal/src/render/text.rs`: `paint_chars`, fixed-cell vertices, R8 GPU uploads, projection dirtiness, combining/orphan overlays and preedit.
- `crates/harbor-terminal/src/render/pipeline.rs` and `crates/harbor-app/src/terminal_view.rs`: concrete renderer preparation and viewport/scale integration; resource invalidation must follow the real host path.
- `crates/harbor-widget/src/renderer/widget_text_atlas.rs`: existing consumer of shared `harbor-text` coverage contracts; preserve compatibility without expanding widget emoji scope.

These are inspected integration points, not implementation or runtime acceptance claims.

## End-to-End Tests

| Case | Given / when | Required observable outcome |
| --- | --- | --- |
| Named supported font | Actual Harbor/ConPTY output of `A♥️B` and `A👩‍💻B` with a recorded supporting Windows font/version | Complete intended color artwork; unchanged assigned widths and A/B geometry; copied codepoints exactly match input |
| Text versus emoji | Display ordinary `♥`, a text-selector form, explicit emoji-selector form and default emoji candidates | Presentation requests distinguish intent; existing model-assigned widths remain authoritative; text is copied unchanged |
| Unsupported presentation | Use a verified unsupported font/format/capability case, including a controlled backend failure where needed | Complete monochrome if available, otherwise visible documented fallback; no spill into neighboring cells or changed copy/width |
| Oversized/native bearings | Present emoji with ink bounds outside its cell rectangle, near right/vertical edges | Oversized artwork is uniformly downscaled; final pixels remain clipped; no neighbor changes |
| Stream extension | Draw `♥`, then append VS16; draw `👩`, then append ZWJ and `💻` through fragmented ingestion and the recorded runtime path | Prior presentation is replaced; old pixels do not remain; source, width and neighbors agree with the model |
| Cache reuse and churn | Repeated units followed by many distinct requests, negative results and forced atlas repacks | Warm hits avoid repeated rasterization; bounded resources; current valid UVs after repack and visible fallback at unsupported results |
| Font/DPI/appearance transition | Replace font/session/size, change DPI, or change a raster-dependent appearance while text is retained; include a skipped draw before resuming | Current presentation replaces stale tiles/failures; no text loss; affected cells repaint on the next coherent draw |
| Color composition | Render partially transparent color artwork on contrasting backgrounds and exercise font foreground-dependent layers | No terminal tint on fixed artwork, no alpha fringe/double multiplication, foreground-dependent layers follow current color |
| Existing overlays | Mix emoji with `e` + U+0301, isolated U+0301, conceal, decorations, selection and preedit | Existing behavior survives; no duplicated base/mark paint; exact source copy remains unchanged |
| Fast-path comparison | Same ordinary monospace workload before/after, then cold/warm emoji workloads | Ordinary path remains R8; no ordinary-only color-atlas allocation or unnecessary whole-layout shaping; comparable costs and exclusions are recorded |

## Decisions and Traceability

| Assertion / choice | Authority |
| --- | --- |
| Complete color/ZWJ outcome, unchanged text/width/copy/neighbors, bounded fallback and required gates | Issue #181 |
| Retained-unit width and copy authority, presentation separated from preservation | ADR 0044; Spec 0015 |
| DirectWrite + reused Direct2D offscreen rasterization, pixel transfer rather than shared-texture interoperation | User confirmation; ADR 0054 |
| Ordinary R8 path and separate bounded lazy color atlas; bounded presentation caches | Confirmed recommendation; ADR 0054; issue #181's resource/performance obligations |
| Uniform downscaling, positioning and final cell clipping | User confirmation; ADR 0054 |
| Native resource ownership outside the model and coherent damage/hidden-draw behavior | ADR 0045; ADR 0054 |
| Presentation classifications, fixed versus foreground-dependent colors, cache generation semantics | Confirmed recommendation; ADR 0054 |
| Evidence scope, quality checks and truthful support claims | Issue #181; Validation Policy |

No additional product/architecture decision is pending. Exact supported native formats/API availability and known supporting fixtures are facts to establish during execution, not a guarantee that every platform can render every sequence. Private cache structures, numeric resource limits, exact native interface selection and pixel encoding are execution details constrained by the contracts above. A needed new semantic or architectural choice must return to refinement before this spec is changed.

## Limitations and Out of Scope

- No universal emoji/font/platform promise, general complex-script shaping, arbitrary ligatures, or change to terminal width/copy semantics.
- No parser/PTY transport redesign, renderer marketplace, broad screen-storage rewrite, or blanket widget-text emoji feature.
- No initial Direct2D/wgpu shared-texture interoperation or all-text conversion to RGBA/whole-layout shaping.
- #181 is a separate follow-up, not a prerequisite or native sub-issue blocking #153, and not a replacement for #50 or #176.
- Spec 0015's documented DECAWM-disabled final-column VS16 width discrepancy remains outside this presentation change. Fit to the actual model-assigned rectangle and record the exclusion; this spec neither authorizes a new width exception nor resolves the historical discrepancy.
- Current native API research and local font inspection are not runtime acceptance. This Draft contains no new implementation, test-execution or Windows support claim.
