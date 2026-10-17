# Decoration Metric Lifecycle — T0002 Evidence

## Scope and source snapshot

- **Date:** 2026-10-11
- **Base revision:** `f212a4723871bd09c3492853112b49b1c4ca042f`, with the uncommitted T0002 changes below. No staging, commit or issue edit was performed.
- **Requirements:** [Spec 0022 R3 and decoration/transition-specific R4](../.grimoire/spec/0022-box-drawing-and-decoration-lifecycle.md), [Spec 0018](../.grimoire/spec/0018-modern-sgr-decorations.md), [ADR 0056](../.grimoire/adr/0056-terminal-owned-procedural-box-drawing.md), and [Validation Policy](validation.md).
- **Plan:** `.grimoire/plans/0026-decoration-metric-lifecycle.md` (local, ignored Grimoire planning context).
- **Dirty production scope:** `crates/harbor-terminal/src/render/decoration.rs`, `layout.rs`, `pipeline.rs`, the shared viewport-validity/capacity seam in `box_drawing.rs`, optional renderer `pollster` dependency in `crates/harbor-terminal/Cargo.toml`, and rejected-preparation handling in `crates/harbor-app/src/terminal_view.rs`.
- **Dirty test scope:** `render/decoration_tests.rs`, `render/text_tests.rs`, and the existing real-GPU application bridge regression in `terminal_view.rs`.
- **Documentation scope:** this record, T0002 status, and local plan context only. Global spec/ADR/ticket-set and T0003 completion are not promoted.

This record establishes focused CPU and **actual offscreen GPU** evidence, including native DirectWrite changed-metrics replacement. It does not establish Windows desktop/application or actual cross-monitor acceptance. The existing T0001 box behavior remains integrated and its regressions run in the combined workspace suite; this record does not rewrite [T0001's historical evidence](procedural-box-drawing-evidence.md).

## Implementation and review

All three decoration kinds share metric-to-viewport scaling, row-local placement and clipping. Underline retains its top-edge meaning, style band and grid-relative patterns; strikethrough retains its center meaning; overline starts at the row top. Finite offsets outside the row clamp back inside it. Invalid metric records produce safe empty CPU slots or reject a pipeline transition. Wider arithmetic and fixed pattern iteration bound extreme geometry without introducing a fallback font or arbitrary raster-scale limit.

The **96-vertex underline slot**, six-vertex strike/overline slots, colors, explicit underline styles, spaces/wide slots, OSC 8 fallback, inverse, conceal and independent resets remain unchanged in meaning. Strikethrough-on-space is not expanded. Counts/device limits and ranges are checked before retained uploads; both grid dimensions and viewport identity force sufficient rebuilding. Non-drawable decoration preparations preserve allocated resources, suppress drawing and stay dirty for recovery.

Font replacement creates a complete candidate at the existing raster scale, then checks scoped wgpu validation/internal/allocation errors before publishing it. Failed CPU preflight or actual GPU candidate validation leaves the prior metrics, native session, atlas, buffers, pipelines and pixels unchanged. `prepare` and `sync_viewport` now return acceptance booleans; the product bridge must not draw or acknowledge a rejected update. Scale changes preflight the update before mutating presentation identity. This is a repair of the existing seam, not a desktop font-settings reload feature.

Inline review/check covered the complete production/test diff, new evidence file, source ticket/spec/ADR constraints, resource upload helpers, native metrics, engine acknowledgment and existing application bridge consumers. A review finding about success-shaped rejection was fixed by the boolean/bridge handoff and verified by actual GPU rejection/recovery coverage. No confirmed in-scope blockers remain. No core/model fields, terminal width/copy policy, shared font override, new SGR behavior or host reload capability were added.

## Execution environment

- Windows 11, `10.0.26300.9457`; Rust `1.97.1 (8bab26f4f 2026-07-14)`; Cargo `1.97.1`; Python `3.12.10`.
- Hardware adapter: **AMD Radeon RX 7900 XT**, discrete, Vulkan; AMD proprietary driver; `26.8.1 (LLPC)`.
- Software adapter: **Microsoft Basic Render Driver**, CPU, DX12/WARP; driver `10.0.26100.9278`.
- Application under test: Cargo-built Harbor terminal/bridge harness, not an interactive named terminal workload. Fixture bytes enter the actual parser/engine directly; no ConPTY/SSH/tmux workload certification is implied.
- Native fonts: requested **Consolas**, physical size **16 → 23**, through `load_system_fonts` and `TerminalRenderPipeline::replace_fonts`.

Measured native metrics, not a same-configuration reload:

| Metric | Consolas 16 | Consolas 23 |
| --- | --- | --- |
| Cell width | 9 | 13 |
| Line height | 19 | 27 |
| Ascent | 15 | 22 |
| Underline top offset | 15.984375 | 22.22754 |
| Underline thickness | 1.5 | 1.5 |
| Strike center offset | 6.7429686 | 9.552392 |
| Strike thickness | 1.5 | 1.5 |

- Native replacement target: `Rgba8Unorm`, 256 × 128; allocation origin (3.25, 4.75), normal terminal padding. Reprojection also uses origin (7.5, 1.25), allocation 230 × 120, surface 240 × 120.
- Decoration readback: `Rgba8Unorm`, 256 × 192, transparent clear, single sample, 128 × 160 hard scissor. Cells include 16 × 24, 12.75 × 21.25 and 10.5 × 19.5; fractional origins (3.35, 4.45) and (1.25, 2.75). Resize replay includes 4 × 8 → 8 × 4 → 4 × 4 → 3 × 8 → 4 × 8 → 5 × 9.
- Scale-only identities: smallest positive f32, 1.25, 1.5, 2.0, `f32::MAX / 192`, and 1.0. Invalid/non-finite/zero/negative/overflowing scale requests are rejected. These offscreen identity transitions are **not actual cross-monitor moves**.
- Controlled native candidate failure: an invalid depth-format color target triggers a real scoped wgpu validation error after CPU preflight without losing the device. Physical GPU OOM/internal errors and device loss were not induced. OOM/internal error filters are installed, but no separate OOM/internal fault-injection or device-loss recovery result is claimed.

## Reproducible commands and outcomes

Run from the repository root. In Bash/Git Bash, the WARP command below sets the process environment; PowerShell equivalent: `$env:HARBOR_TEST_GPU='dx12-warp'`, run the focused command, then remove that environment variable before the default-adapter gates.

| Command | Expected result | Observed result | Outcome |
| --- | --- | --- | --- |
| `cargo test -p harbor-terminal t0002_ -- --nocapture` | CPU invariants and two actual GPU cases complete | 4 passed, 0 failed, 0 ignored; native metric values and two PASS evidence messages | PASS |
| `HARBOR_TEST_GPU=dx12-warp cargo test -p harbor-terminal t0002_ -- --nocapture` | Same checks on asserted DX12 CPU adapter | 4 passed, 0 failed, 0 ignored; actual WARP adapter output and GPU assertions | PASS |
| `cargo test -p harbor-app rendered_bridge_replays_hidden_output_and_retained_resize_after_skipped_frames -- --nocapture` | Rejected/skipped targets retain engine damage; valid recovery consumes it | 1 passed, 0 failed; no adapter skip; also included in workspace execution | PASS |
| `cargo fmt --check` | No Rust formatting differences | Exit 0 | PASS |
| `cargo clippy --all-targets --all-features -- -D warnings` | Strict lint gate completes | Exit 0 | PASS |
| `cargo test --workspace` | Combined-tree regression gate completes | 2396 passed, 0 failed, 10 ignored, including doctests | PASS for executed cases only |
| `cargo check -p harbor-terminal --no-default-features` | GPU-independent build remains valid | Exit 0, no warnings after renderer-only helper gating | PASS |
| `python scripts/docs/check_docs.py` | Documentation language/local targets valid | Exit 0; language and local-link checks passed | PASS |
| `python scripts/docs/checklist_summary.py` | Protocol inventory computes | Exit 0; 855 done, 144 open, 999 total (inventory, not acceptance) | PASS |
| `git diff --check` | No diff whitespace errors | Exit 0 over the combined diff | PASS |

The commands were rerun after behavioral fixes and formatting. Cargo emitted access-denied notes about finalizing some pre-existing incremental-cache directories; compilation and recorded commands still completed. These notes indicate lost cache reuse, not a substitute for a failed build. An added renderer-only helper warning in the headless check was corrected and that check rerun cleanly.

The workspace's **10 ignored cases remain NOT RUN**: OSC/update manual throughput probes, three native emoji/color/preedit GPU cases, two supporting-Windows-font capability cases, cold-start Latin-frame manual E2E, interactive secondary-window lifecycle and one parser documentation example. The new T0002 GPU tests fail explicitly on unavailable adapters rather than returning a success-shaped skip. Raw run logs remain local; this document is the self-contained handoff and does not link local-only artifacts.

## Criterion-to-evidence assessment

| T0002 criterion | Implementation and executed evidence | Assessment |
| --- | --- | --- |
| Three decoration kinds project current metrics consistently and stay row-bounded | Shared projection/thickness helper; analytical CPU assertions for each kind across changed metrics, offsets, thicknesses, fractional origins/cells and scales; real GPU current-metric replacement/reprojection | Match |
| Established modern SGR semantics, no strike-on-space expansion | Existing semantic predicates/color helpers retained; existing underline/overline/strike space, wide, OSC 8, inverse, conceal and reset regressions executed in workspace | Match |
| Fixed underline slots, full/incremental convergence and changed stride | Unchanged 96-slot constant/offsets; checked capacity/ranges; actual dirty replay and fresh-layer readback equality through equal-count stride change, shrink/grow, local edits and viewport changes | Match |
| Native replacement demonstrably changes metrics without engine text damage | DirectWrite Consolas 16 → 23 assertions; empty engine damage; replaced scalar GPU resource identities; text/decoration/whole-pipeline pixels change and match a fresh current-metric pipeline | Match |
| Failed preparation does not publish partial state or discard valid presentation | Malformed/oversized candidate rejection plus actual scoped GPU validation failure; prior viewport/metrics/session/atlas/pipeline/buffer identities and byte-exact GPU pixels retained | Match |
| DPI-only changes preserve physical font size; actual projection changes invalidate | Same-metrics session and scale-only transitions separately tested; scalar resources, metrics and whole-pipeline pixels unchanged under accepted scale identities; allocation/origin/surface reprojection matches fresh pipeline | Match |
| Invalid/non-finite/extreme/non-drawable inputs are safe and bounded | CPU metric/view matrix and extreme indices; finite normalized slots; fixed loops; checked overflow/device sizes; invalid ranges force full rebuild; GPU malformed target suppression and resource-preserving recovery | Match |
| Hidden/skipped and rejected rendering retain coherent damage | Actual retained engine edit replay and stale-ack rejection; product bridge GPU test verifies rejected-target damage retention and valid recovery; no premature acknowledgment on false preparation | Match |
| Practical GPU evidence is distinct from desktop acceptance and handed to T0003 | Actual Vulkan and WARP encode/readback plus native metric values and commands here; remaining runtime cases below explicitly NOT RUN | Match |

## T0003 handoff and evidence boundaries

| Evidence case | Outcome | Reason / next owner |
| --- | --- | --- |
| Decoration CPU geometry and actual retained GPU readback | PASS | Focused Vulkan/WARP cases above |
| Native changed-metrics ordinary-text/all-three-decoration **offscreen resource** replacement and failure retention | PASS | Real DirectWrite fonts and renderer replacement seam; not desktop user-settings hot reload |
| Named Windows application box workload with redraw/resize and identified transport | NOT RUN | T0003 owns interactive combined acceptance; not performed here |
| Combined native changed-metrics observation including boxes, ordinary glyphs and every decoration kind | NOT RUN | T0003 must supply the combined observation; T0001 box evidence plus these separate T0002 checks are not substituted for it |
| Actual Harbor window moves between monitors with differing DPI/scale | NOT RUN | No cross-monitor procedure was performed; static/offscreen scale mutation is not this evidence |

T0003 must run its required combined-tree quality gates and runtime procedures or obtain an explicit scope decision for each unavailable required case. This record does not authorize blanket deferral, mark desktop acceptance, promote global completion, or close Issue #193.
