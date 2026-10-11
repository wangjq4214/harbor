# Procedural Box Drawing — T0001 Evidence

## Scope and source snapshot

- **Date:** 2026-10-11
- **Base revision:** `902b980ba6056ba1ac4e6bfcf1d1b93fc6aed738`, with the uncommitted T0001 changes listed below. No commit, staging or issue edit was performed.
- **Requirement source:** [T0001](../.grimoire/ticket/0016-box-drawing-and-decoration-lifecycle/T0001-procedural-box-drawing.md), [Spec 0022 R1/R2 and box-specific R4](../.grimoire/spec/0022-box-drawing-and-decoration-lifecycle.md), [ADR 0056](../.grimoire/adr/0056-terminal-owned-procedural-box-drawing.md).
- **Plan:** `.grimoire/plans/0025-procedural-box-drawing.md` (local Grimoire planning context).
- **Dirty production scope:** new `crates/harbor-terminal/src/render/box_drawing.rs`; integration in `render/mod.rs`, `render/pipeline.rs`, and scalar-base suppression in `render/text.rs`.
- **Dirty test scope:** new `render/box_drawing_tests.rs` plus the repertoire scalar-collection regression in `render/text.rs`.
- **Documentation scope:** this evidence record and local plan/ticket status. T0002/T0003 and global spec/ADR completion statuses are not promoted by this record.

This is focused offscreen renderer evidence, not final Windows desktop/application acceptance. The complete U+2500–U+257F repertoire is terminal-local; shared `harbor-text` and widget font resolution/atlases remain unchanged. Box resources own independent **156-vertex cell slots** with checked counts, device-buffer bounds, and row/range uploads. The existing **`UNDERLINE_VERTICES_PER_CELL = 16 * 6`** and underline offsets are unchanged.

## Execution environment

- Windows 11, build `10.0.26300`; Rust `1.97.1`, Cargo `1.97.1`; Python `3.12.10`.
- GPU: AMD Radeon RX 7900 XT, discrete; Vulkan; AMD proprietary driver, reported driver information `26.8.1 (LLPC)`.
- Readback targets: `Rgba8Unorm`, 256 × 256, single sample, transparent clear, hard render-pass scissor at 248 × 248.
- Geometry fixtures: cells 16 × 24 and 14 × 24; fractional cells 9.3 × 17.7 and 12.75 × 21.25; origins (0, 0), (3.35, 4.45), and (0.5, 1.5). Retained reprojection also uses 10.25 × 19.75 at (3.3, 4.7). No UI antialiasing or bitmap stretching is used.
- Native-font fixture: requested Consolas at physical sizes 16 and 23, through `load_system_fonts` and the existing `TerminalRenderPipeline::replace_fonts` seam; asserts a real cell-width metric change. DPI identity transitions 1.25, 1.5, 2.0, 1.0 preserve physical box pixels. These are not actual cross-monitor moves.
- Application under test: Cargo-built Harbor renderer test harness, not a named interactive terminal workload. Test streams enter the real parser/engine directly; no workload transport certification is implied.

## Reproducible commands and observed outcomes

Run from the repository root:

| Command | Expected result | Observed result | Outcome |
| --- | --- | --- | --- |
| `cargo test -p harbor-terminal box_ -- --nocapture` | Complete focused CPU/GPU checks, actual adapter output and readback assertions | 8 passed, 0 failed, 0 ignored; three actual GPU cases emitted PASS after assertions | PASS |
| `cargo fmt --check` | No Rust formatting differences | Exit 0 | PASS |
| `cargo clippy --all-targets --all-features -- -D warnings` | No lint errors | Exit 0 | PASS |
| `cargo test --workspace` | Integrated regression suite completes | 2392 passed, 0 failed, 10 ignored, including doctests | PASS for executed cases only |
| `python scripts/check_docs.py` | Documentation language and local links are valid | Exit 0 | PASS |
| `python scripts/checklist_summary.py` | Protocol inventory can be computed | Exit 0; 855 done, 144 open, 999 total (inventory, not release acceptance) | PASS |
| `git diff --check` | No diff whitespace errors | Exit 0 | PASS |

Cargo reported access-denied notes when finalizing some pre-existing incremental-cache directories. Compilation and the recorded commands still completed successfully; the notes indicate lost cache reuse, not a passing substitute for a failed build.

Workspace exclusions remain exclusions: manual OSC/update throughput probes; Windows native emoji/color capability cases and color/preedit sequence GPU cases; cold-start Latin-frame manual E2E; interactive secondary-window host/GPU lifecycle; one parser documentation example. None of these ignored cases is labeled as having passed by this record. The new box GPU tests require an adapter and fail explicitly on missing infrastructure rather than returning a success-shaped skip.

Raw logs remain local; this document contains the commands, environment and results necessary for the handoff without linking local-only artifacts.

## Criterion-to-evidence assessment

| T0001 criterion | Implementation and executed evidence | Assessment |
| --- | --- | --- |
| All 128 forms, weights and special families | Explicit directional descriptor repertoire; independent scalar-ordered arm oracle; CPU vertices for every scalar; GPU nonempty pixels for every scalar; raster connected-component oracle for solid/mixed junctions, double corners/tees/crosses, and dash counts | Match |
| Compatible joins including fractional geometry | Indexed shared-edge rounding; exhaustive compatible CPU endpoint-pair checks across three projections; actual GPU horizontal and vertical borders with corners/tees and light/heavy/double lanes at integer/fractional dimensions/origins | Match |
| Sharp high-contrast strokes and intentional gaps | Single-sample colored triangles, no font sampling in box presentation; exact red-or-transparent GPU pixels; explicit dash/half-line/double-center holes; rounded/diagonal and stroke-weight CPU assertions | Match |
| Existing semantics and no duplicate scalar paint | Existing effective palette/inverse foreground helper, conceal suppression, cell/allocation/surface clipping; GPU inverse/scissor assertions; cache-populated scalar box layer paints nothing; suffix/ordinary/preedit glyphs still paint; parser source/copy/width/cursor assertions; shared font/widget paths unchanged and workspace regressions pass | Match |
| Local edits and stale geometry clearing | Actual retained GPU readback equals a fresh layer after character/weight edits, attributes, conceal/reveal, erase/replacement, inverse and OSC palette changes | Match |
| Viewport/resize/font invalidation and skipped frames | Automatic viewport/grid identity plus pipeline invalidation; changed-native-font metrics reproject box pixels with empty engine damage; scale-only transitions preserve physical pixels; non-drawable prepare disables draw and remains dirty; pending engine damage survives reads/skips and resume matches rebuild | Match |
| Degenerate/extreme inputs and safe bounds | Complete CPU repertoire exercised with zero/negative/NaN/infinities/tiny/huge dimensions/origins/padding and extreme indices; finite normalized vertices; checked allocation overflow/device limits and zero-grid rejection; GPU invalid/tiny/huge preparations and recovery preserve resources and converge | Match |
| Shrink/grow and equal-count row-stride change | GPU 4 × 8 → 8 × 4 → 4 × 4 → 3 × 8 → 4 × 8 replay; both grid dimensions force rebuilding; slot counts/buffer bounds and full/incremental pixels agree | Match |
| Separate box/underline layouts | New independent slot constant/buffer/offsets; no decoration source changes; existing decoration regression coverage executed in workspace | Match |
| Honest evidence for T0003 | Actual adapter/readback and command results recorded here; final runtime cases below remain NOT RUN | Match |

Inline review covered the tracked integration diff and both new files, plus the upload/color/font/viewport/engine-update consumers. No confirmed in-scope blockers remain. The approach follows ADR 0056 without a new core/model field, neighbor cache, shared atlas override, width policy, or decoration lifecycle redesign.

## T0003 handoff and remaining runtime evidence

| Evidence case | Outcome | Reason / next owner |
| --- | --- | --- |
| Focused box CPU and actual GPU encode/readback | PASS | Executed above, no unavailable box GPU case |
| Changed-metrics native font-resource **box-only** offscreen reprojection | PASS | Real renderer replacement seam with native fonts and GPU pixels; not the combined desktop observation |
| Named Windows application border workload, with redraw/resize and specified transport | NOT RUN | T0003 owns application/combined acceptance; not performed in T0001 |
| Combined DirectWrite changed-metrics resource smoke observing boxes, ordinary glyphs and underline/strikethrough/overline before/after | NOT RUN | Needs T0002 and T0003 integration; box-only test is not a replacement |
| Actual window moves between monitors with different DPI/scales | NOT RUN | Not performed; automated viewport/scale mutation is not this evidence |

T0002 may touch `pipeline.rs` and viewport/retained tests; preserve box invalidation hooks and independent slots when integrating. T0003 must rerun its required quality gates over the combined tree and obtain the named workload/native-resource/cross-monitor evidence, or an explicit authorized deferral for each unavailable required runtime case. This record does not authorize deferral or close Issue #193.
