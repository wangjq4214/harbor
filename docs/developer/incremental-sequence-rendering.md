# Incremental Sequence rendering verification

Implementation/evidence for [spec 0048](../../.grimoire/spec/0048-incremental-emoji-sequence-rendering.md), preserving [spec 0020](../../.grimoire/spec/0020-color-emoji-sequence-presentation.md) and the scalar-atlas/update contracts.

## Scope and implementation

Baseline: `3416f2a`. Compared with the same revision plus the incremental rendering working-tree changes in `render/sequence.rs`, `render/text.rs`, removal of an unused generation query in `render/color_atlas.rs`, focused tests, and a renderer-gated benchmark example. Existing context/ADR/spec edits were retained. No engine/update-domain or native shaping/cache changes were made.

- **R1:** Sequence retains classified candidates, bounded complete requests, resolved dependencies and geometry per visible row. Only damaged rows are classified; untouched rows are reused. Unchanged preparation returns without classification, atlas synchronization, geometry construction or uploads. Replayed damage compares only touched-row candidate inputs and preserves identical retained geometry.
- **R2:** Lazy independent row buffers support complete tiles and missing cues. Changed row geometry is compared byte-for-byte before writing. Removal releases that row's draw buffer; untouched buffers do not move. Text merges suppression changes with incoming content damage instead of promoting emoji presence to full dirtiness.
- **R3:** Dimensions, viewport, palette, effective raster scale and presentation generation key global reconstruction. Atlas synchronization compares actual placement/bounds before and after; changed references are resolved even outside content damage. Scalar additions/repacking also refresh fallback dependencies. UV-only movement does not request ordinary-text damage. Existing scalar-atlas eviction/full-upload behavior remains intact.
- **R4:** All new state is terminal-renderer-owned. Candidates/CPU geometry are bounded by the visible grid, requests by the native unit limit, active atlas requests by 128, and pixels/caches by existing limits. Historical request strings are dropped when rows change; empty geometry releases its row buffers. Full-suffix selector classification avoids truncation-dependent stale outcomes. No width, source/copy or acknowledgement semantics changed.

Tradeoff: each emoji-bearing row can add a draw call per populated Sequence layer. This favors independent updates and bounded storage over a shared variable-length batch allocator. Row bookkeeping and bounded active-request synchronization still run on changed preparations; whole-frame constant-time behavior is not claimed.

## Environment and recipes

Windows 11 Pro, build 10.0.26300.9711; Rust 1.99.1, x86_64 MSVC; one terminal/session, no window/PTY for microbenchmarks. Primary font: installed Consolas, physical 16 px, metrics 9×19 px/ascent 15. Native emoji fallback: installed Segoe UI Emoji. Effective test DPI 96 unless a resource-transition test selects another scale. Bundled ConPTY reports 1.24.2607.10001, but no new interactive ConPTY acceptance is claimed.

Font-file SHA-256 identities (exact installed-file provenance; PE file-version fields were unavailable):
- `consola.ttf`: `cf00b507b3286870cc5064ebd0633c303f70b491a4af25eec2d32df413db0179`
- `seguiemj.ttf`: `451df9bb5546055af625ff86aa72695df43e8e170b4c6c1316fe44e9c7a22f7c`

Native GPU: AMD Radeon RX 7900 XT, Vulkan, AMD driver 26.10.1 (LLPC). Renderer regression suite was also repeated on DX12 WARP. Timings use release/default renderer features; work assertions and readbacks use the debug test profile.

```sh
cargo test --workspace
cargo test -p harbor-terminal --no-default-features
cargo test -p harbor-terminal render:: --lib -- --include-ignored --test-threads=1
HARBOR_TEST_GPU=dx12-warp cargo test -p harbor-terminal render:: --lib -- --include-ignored --test-threads=1
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all --check
cargo run --release -p harbor-terminal --example sequence_prepare -- 24
cargo run --release -p harbor-terminal --example sequence_prepare -- 240
```

The same benchmark source was copied into a temporary detached worktree at baseline `3416f2a` and run there using the same compiler, GPU, fonts and grid sizes. Only that example and the existing bundled ConPTY runtime were supplied to the baseline; its production source was unchanged. The temporary worktree was removed afterward.

## Work observations

[Incremental tests](../../crates/harbor-terminal/src/render/sequence/tests/incremental.rs) are nested under `gpu::` so the existing explicit CI GPU recipe includes them. Counters live in the independent test-only `render/observations.rs` thread-local recorder, not conditional fields on renderer structs, and record classification, rebuilt rows, actual row-buffer write bytes, color-texture bytes and ordinary-text write offsets/bytes. Native shape/raster/cache counters remain separate.

A warm emoji on row 1, alternating cached ASCII x/y at row 0 column 0, produces engine damage `row 0, columns [0,2)` (neighbor expansion). Eight repeated edits at each grid size assert:

| Grid | Classified cells/edit | Sequence rebuilt rows | Sequence vertex/texture bytes | Ordinary vertex bytes/edit |
| --- | ---: | --- | ---: | ---: |
| 8×80 | 80 | none | 0 / 0 | 384 |
| 80×80 | 80 | none | 0 / 0 | 384 |
| 240×80 | 80 | none | 0 / 0 | 384 |

Native shape/raster calls do not increase. A separate unchanged preparation asserts zero classification, geometry work, texture and vertex writes, including persistent missing cues. Replaying nonempty pending damage on the same projection also asserts zero rebuilt rows/vertex writes: identical candidates retain resolved state rather than treating repeated damage as new Sequence content.

For comparison, baseline source deterministically scans every cell for each such content update, rebuilds the visible Sequence batch and forces a full ordinary vertex upload: at 240×80, 19,200 inspected cells, 192 Sequence vertex bytes for the one complete emoji, and 3,686,400 ordinary vertex bytes. These baseline **work counts are source-derived**, not instrumented measurements; timings below are actual executions.

Local complete-tile/missing-cue additions, replacements and removals assert work/write rows restricted to the touched row, while preserving the other row's CPU geometry and GPU readback. One missing cue writes 24 colored vertices (576 bytes), not all missing-cue rows. Removal drops obsolete draw buffers without writing an entire Sequence batch.

Pressure fixtures seed deterministic tiles through the existing atlas seam, not a new production pressure hook. Tests cover:
- Exhausted placement slots moving an untouched row's UVs, with a full color texture synchronization but no scalar damage on that row.
- Capacity release promoting an untouched row from scalar fallback or missing cue to a tile, removing obsolete cue geometry and requesting scalar damage only when suppression actually changes.
- 128-request admission overflow and later promotion of an untouched row; 140 subsequent appearance changes keep only two visible candidates/atlas entries, with bounded native shape/raster/pixel caches.
- Fresh full classification/geometry and pixel readback with the same presentation outcomes and fresh atlas placements as the pressure oracle. Existing native tests independently compare real complete preparations.

## Before/after preparation measurements

[Benchmark](../../crates/harbor-terminal/examples/sequence_prepare.rs): each mode has 100 warm-up and 500 measured samples, queue submit/device wait between iterations but **outside** the timed region. Timed region is Text preparation only: excludes mutation/update-read/acknowledgement, draw, PTY and display presentation. Bounded mode uses `put_str` rather than the existing `process_output` scroll-to-bottom path, which can supply full damage. Modes run in order; full invalidation is measured after Sequence churn, not on a restored ordinary-only fixture.

Values are median / p95 **microseconds**, single comparable run per revision/grid; not a statistical latency or whole-frame guarantee.

| Fixture / mode | 24×80 before | 24×80 after | 240×80 before | 240×80 after |
| --- | ---: | ---: | ---: | ---: |
| Ordinary / clean | 0.0 / 0.1 | 0.4 / 0.5 | 0.0 / 0.1 | 0.5 / 0.5 |
| Ordinary / bounded | 6.0 / 8.5 | 4.1 / 7.8 | 39.1 / 53.7 | 3.6 / 5.2 |
| Ordinary / Sequence churn | 42.7 / 56.5 | 11.3 / 24.0 | 1214.5 / 1589.1 | 11.0 / 23.6 |
| Ordinary / full after churn | 42.0 / 60.2 | 43.4 / 55.7 | 1180.8 / 1457.9 | 1184.2 / 1385.9 |
| Persistent emoji / clean | 0.0 / 0.1 | 0.4 / 0.5 | 0.0 / 0.1 | 0.4 / 0.5 |
| Persistent emoji / bounded | 41.7 / 54.8 | 3.8 / 6.1 | 1215.8 / 1702.4 | 3.6 / 4.8 |
| Persistent emoji / Sequence churn | 45.1 / 59.2 | 10.2 / 15.0 | 1225.5 / 1653.3 | 10.4 / 14.5 |
| Persistent emoji / full after churn | 43.3 / 58.6 | 46.1 / 63.7 | 1202.3 / 1612.3 | 1180.0 / 1354.5 |

The clean-path key check adds about 0.4–0.5 µs in this capture. Full invalidation remains viewport-sized and can be slower; it is not the optimization target. Warm persistent-emoji bounded mode retains one shape/raster, one offscreen creation, 1,088 cached tile bytes and a lazily allocated 4,194,304-byte color atlas on both revisions. Churn warms two native presentations; subsequent cache hits do not shape/raster again.

Before production edits, the existing `monospace_prepare` whole-pipeline example also ran: 24×80 clean median/p95 6.6/6.8 µs; its snapped one-cell path 1758.1/2085.6 µs; explicit full 1752.1/2074.5 µs. This is baseline context only, not comparable to the new text-only timings or evidence of bounded engine damage.

## Results and remaining evidence

| Check | Result |
| --- | --- |
| Default workspace tests | PASS — 2,754 passed, 172 ignored across unit/integration/doctest groups |
| Renderer-free core | PASS — 801 library tests plus applicable integrations/doctests; explicit ignored tests remain excluded |
| All terminal renderer tests, including native/GPU tests | PASS — 100, both native Vulkan and DX12 WARP |
| Strict workspace Clippy, all targets/features | PASS |
| Workspace formatting and diff whitespace checks | PASS |
| Complete color/monochrome, leading scalar fallback, visible missing cue, fragmented VS16/ZWJ, combining/isolated marks, conceal, decorations, IME, palette/DPI/font changes and cached resize/scalar-atlas reuse | PASS — applicable tests in the renderer suite |
| Scroll/review, alternate/primary, changed stride, accumulated skipped/rejected preparation and stale/current acknowledgement | PASS — focused incremental test compares current Text pixels with fresh complete preparation |
| Full library invocation including every ignored calibration experiment | FAIL — 1,144 passed and one existing calibration experiment failed before the final two tests were added; the same failure reproduces on untouched baseline with renderer disabled |
| Fresh interactive application screenshots, clipboard codepoints and ConPTY-driven fragmented reads | NOT RUN — automated native/GPU readbacks are not substitutes for this evidence |

The unrelated ignored failure is `normal_buf::calibration::perf::calibration_text_unit_candidates`, combining-growth fixture at chunk size 1. It also fails at baseline `3416f2a` with `--no-default-features`; no engine/calibration source changed here. It was not repaired as part of rendering work.

Inline review and requirements check found no confirmed change-induced blockers. Automated criteria are established by the scoped checks above; full desktop acceptance is not claimed and the spec status remains unchanged. Packaging, all workspace ignored tests, whole-frame performance, interactive visual/copy acceptance, process-memory profiling and display-present latency were not rerun/measured.
