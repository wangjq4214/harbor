# Color Emoji and Whole Sequence Presentation (#181)

**Spec ID:** 0020
**Status:** Implementation present; integrated Windows/performance acceptance unverified
**Date:** 2026-10-10

**Stable execution aliases:** ticket-set0015/T0001 = native sequence presentation; T0002 = terminal atlas/invalidation; T0003 = integrated Windows/performance acceptance under R1–R7. Scripts referring to ticket-set0015 T0003 retain this meaning. Neither #153's evidence gap nor #176's final acceptance is a new blocking prerequisite; their contracts remain invariants.
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

Record revision and dirty-tree scope, OS/ConPTY/font/DPI versions, reproducible commands/steps, expected and observed results, artifacts and exclusions, following Validation Policy. Keep raw artifacts local or link retrievable CI artifacts; committed summaries are self-contained and do not link local-only captures. A delayed write alone does not prove distinct PTY reads; identify the actual fragmentation exercised.

Run or account honestly for:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
python scripts/docs/check_docs.py
```

Use PASS / FAIL / NOT RUN / BLOCKED for actual checks. Missing Windows/GPU capabilities do not turn skipped runtime checks into passes. Update behavior/support documentation only for implemented and evidenced scope.

## Acceptance status

Native sequence and renderer/atlas implementations are present (`harbor-text` sequence backend and `harbor-terminal::render::{sequence,color_atlas,text}`); historical 0015/T0001 and T0002 were marked Done with unchecked acceptance. That is not proof of every criterion. Final 0015/T0003 Windows/clipboard, font/format/capability, actual fragmentation, font/DPI/appearance, composition, forced repack/overflow and comparable ordinary/cold/warm performance evidence remains unverified here. R1–R7 retain those obligations; no numeric performance threshold is introduced. Follow the [profiling guide](../../docs/developer/profiling.md) and rerun affected evidence after code changes.

## Limitations and Out of Scope

- No universal emoji/font/platform promise, general complex-script shaping, arbitrary ligatures, or change to terminal width/copy semantics.
- No parser/PTY transport redesign, renderer marketplace, broad screen-storage rewrite, or blanket widget-text emoji feature.
- No initial Direct2D/wgpu shared-texture interoperation or all-text conversion to RGBA/whole-layout shaping.
- #181 is a separate follow-up, not a prerequisite or native sub-issue blocking #153, and not a replacement for #50 or #176.
- Spec 0015's documented DECAWM-disabled final-column VS16 width discrepancy remains outside this presentation change. Fit to the actual model-assigned rectangle and record the exclusion; this spec neither authorizes a new width exception nor resolves the historical discrepancy.
Implementation presence and historical ticket status are not a new test-execution or Windows support claim.
