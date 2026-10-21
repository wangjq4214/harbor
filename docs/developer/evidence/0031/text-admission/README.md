# T0002 bounded text-unit admission

## Scope and provenance

- **Source:** [0019/T0002](../../../../../.grimoire/ticket/0019-bounded-history-and-performance-budgets/T0002-bounded-text-unit-admission.md), [Spec 0031 R3–R4](../../../../../.grimoire/spec/0031-bounded-history-and-performance-budgets.md), [ADR 0073](../../../../../.grimoire/adr/0073-session-history-accounting-and-text-unit-overflow.md).
- **Numeric authority:** [ADR 0075](../../../../../.grimoire/adr/0075-calibrated-history-and-text-unit-limits.md), adopted from the [calibration capture](../calibration/README.md): **1024 UTF-8 source bytes including the lead scalar**. This run does not recalibrate the value or establish performance budgets.
- **Run:** 2026-10-21; final workspace run completed before 08:23:13 UTC. Base revision `bfde989d88baebfd4c10552d540813f7d846a5c0`, initially clean worktree. Verification uses that revision plus this uncommitted T0002 change, not a released artifact.
- **Dirty scope:** terminal model/writer/coordinator, narrowly scoped normal-buffer bookkeeping, default/continuation cell constructors, new `text_admission_tests.rs`, two defensive search test fixtures; Unicode contract, terminal concepts/security docs, this summary and ticket. Plan: `.grimoire/plans/0041-bounded-text-unit-admission.md` (local ignored planning artifact).
- **Environment:** Windows 11 build `10.0.26300.9711`, x86_64 MSVC; Intel i5-13600KF; rustc `1.97.1 (8bab26f4f 2026-07-14)`, Cargo `1.97.1 (c980f4866 2026-06-30)`, Python `3.12.10`. Native GPU: AMD Radeon RX 7900 XT, Vulkan, AMD driver `26.10.1 (LLPC)`. Bundled x64 ConPTY file version `1.24.2607.10001`, product `1.24.260710001`.
- **Build/workload:** debug test profile, workspace default features (terminal renderer enabled), plus explicit GPU-free terminal checks. New tests feed synthetic UTF-8 directly through the real parser/terminal, 2–3 rows and 4–12 columns, in whole buffers and 1/2/3/7/1023/1024/4096-byte chunks. No private terminal, URI or clipboard payloads. Font/DPI configuration and application/shell versions are not applicable to these headless fixtures. Native tests use their existing real-font test setup, not a new font qualification matrix.

## Implemented policy and consumer boundary

Before any suffix append or width promotion, admission checks the complete lead-plus-suffix source length. A scalar that would exceed the limit freezes the existing scalar-aligned prefix. All further extensions of that unit are dropped, including smaller scalars that would otherwise fill unused bytes. No alternate source string, discarded scalar list, orphan units or presentation cue is retained. The bounded cell-local overflow state remembers only whether the latest discarded scalar was ZWJ, enough to recognize the next independent unit under the existing writer's combining/selector/ZWJ rules.

Moves/clones/reflow retain that state; replacing/erasing text and derived wide continuation cells reset it. Dropped extensions do not promote width, move the cursor, change REP's admitted source, emit anchor changes, bump source versions or dirty cells. The new state fits existing padding: **`Cell` remains 56 B on this Windows x64 target**. This is not an allocator/process-memory measurement or a claim for every target layout.

Copy and read/search use `Cell::raw_text()` (`ch + suffix`); logical reflow clones the same cell. Renderer scalar overlays read only `ch/suffix`, and whole-unit requests use `raw_text()` with their existing bounded presentation/fallback policy. The overflow bookkeeping cannot reconstruct dropped text and is not a second source representation. T0003 UTF-8 history accounting should count this admitted source, not the bookkeeping or string capacity. Public direct cell mutation can deliberately bypass ingestion invariants.

## Criterion evidence

Tests are in [`text_admission_tests.rs`](../../../../../crates/harbor-terminal/src/text_admission_tests.rs). Expected and observed outcomes agree for these eight tests:

| Test suffix | Expected / observed |
| --- | --- |
| `exact_limit_and_sticky_scalar_aligned_prefix` | Exact 1024 B and 1023 B prefixes remain unchanged through repeated excess selectors/marks; suffix capacity stays bounded; independent `B` plus mark is admitted. |
| `fragmentation_and_discarded_zwj_tail_recover_in_both_buffers` | Both retained-joiner/dropped-emoji and dropped-joiner/emoji cases recover at the next independent emoji; primary/alternate cells, text, cursor and widths agree across seven chunk sizes including single bytes. |
| `long_joined_growth_is_bounded_before_history` | A 28,676 B joined unit stops at 1022 B on a scalar boundary in both live buffers, before any history movement; repeated dropped joiner/emoji pairs create no orphan cells, and the following independent emoji is admitted. |
| `dropped_selector_has_no_promotion_anchor_damage_or_rep_effect` | Dropped VS16/joiner/laptop preserves a neighboring search target, cursor, width, row/content versions and empty damage/anchor mutations in insert and overwrite modes; REP cannot resurrect the dropped laptop. |
| `pending_wrap_and_no_wrap_margin_do_not_move_on_discard` | Right-edge discard preserves pending-wrap/clamped cursor and width; next independent unit wraps/overwrites normally. A separate within-limit control explicitly retains the known no-wrap selector-width discrepancy. |
| `copy_read_search_render_and_history_reflow_share_prefix` | Copy, read/search and render snapshot source agree; dropped selector/laptop and synthetic dotted circle are absent; history movement and three reflows retain one searchable unit and its exact source. |
| `overflow_state_follows_edits_resize_and_saved_primary` | Insert/move, primary reflow, alternate rectangular resize and saved-primary round trip preserve admitted source and discard recovery; erase/overwrite resets state and ordinary promotion resumes. |
| `ordinary_and_isolated_units_keep_source_width_and_damage` | 65 B combining and 228 B ZWJ controls, ordinary emoji, wide combining, exact-limit isolated marks and supplementary selector retain source, assigned widths, damage and display-only-cue separation under byte fragmentation. |

Existing parser tests cover ordinary Unicode promotion/edges/style/hyperlink/erase and existing renderer tests cover source-preserving fallback. The two search stress tests now inject a 4001 B stored unit directly using the existing test-only mutable-cell API: this intentionally retains cancellation/linear-scan/indivisible-read defense coverage rather than weakening their expectations to match the new admission cap.

## Commands and outcomes

Commands run from repository root unless stated otherwise. No numerical latency or residency acceptance is inferred from test elapsed times.

| Check | Outcome |
| --- | --- |
| `cargo test -p harbor-terminal --lib --no-default-features parser::tests -- --nocapture` | PASS: 132 tests. |
| Focused `cargo test -p harbor-terminal --lib --no-default-features text_admission -- --nocapture` | PASS: final module, 8 passed, 0 ignored. |
| `cargo test -p harbor-terminal --lib --no-default-features --quiet` | PASS after defensive fixture fixes: 756 passed, 5 ignored (before adding the eighth focused test). |
| `cargo test --workspace --quiet` — final combined tree | PASS: terminal library 1080 passed, 8 ignored; all remaining executed workspace suites/doc tests passed. Ignored/manual scenarios remain unexecuted, not PASS. |
| `cargo fmt --check` | PASS after scoped hashline formatting edits; no broad formatting write. |
| `cargo clippy --all-targets --all-features -- -D warnings` | PASS on final code/test tree. |
| `python scripts/docs/check_docs.py` | PASS: 165 files after documentation changes. |
| `git diff --check` | PASS after documentation changes. |

### Preserved unfavorable observations

- Test authoring initially used a nonexistent `AnchorMutationBatch::is_empty` and copy expectations omitted hard breaks from selecting empty trailing rows. These test defects were corrected without changing copy or anchor behavior.
- The first workspace run failed two search stress assertions (`1023` retained bytes versus the former `4001` expectation). Those fixtures now explicitly bypass admission to continue testing the defensive consumer path; the final workspace run passes them.
- That first workspace run also failed the unchanged native `render::text::sequence_tests::t0002_native_changed_metrics_replacement_and_failed_preparation_are_coherent`, at `render/text_tests.rs:631`: same-metrics replacement produced unequal GPU pixel arrays. It uses short ordinary source, not an overflowing unit. The local raw failure log is `pi-bash-d1a7f371a67d365d.log` in the harness temp directory. **Cause/attribution remains unestablished**; this is not claimed fixed or proven baseline behavior.
- The same native test subsequently passed alone on the changed tree (1 passed), and in the final full workspace run. An exported clean baseline at the base revision also passed its complete terminal library suite (1072 passed, 8 ignored). The baseline export initially could not build because ignored bundled ConPTY DLLs were absent; copying the existing local runtime into that temporary export supplied the prerequisite, without a download or changes to the working tree. A favorable rerun does not establish native flake freedom.
- Rust emitted a nonfatal incremental-cache finalization access-denied note during several commands; builds/tests still completed. No cache cleanup or unrelated code fix was performed.

## Review, acceptance boundary and handoff

Inline review inspected the full production diff and new tests plus the unchanged copy/read, logical decoder/reflow, scalar/sequence render and internal-write boundaries. No confirmed product blocker was found. Acceptance check maps each T0002 criterion to the tests, implementation and evidence above. The residual native reproducibility concern is **needs-verification**, not a demonstrated text-admission defect. No fix to font presentation or the known DECAWM-disabled final-column selector-width discrepancy is claimed.

**NOT RUN in this ticket:** a dedicated interactive ConPTY oversized-source clipboard/visual/font-reload/application matrix, ignored supporting-font color/preedit scenarios, named-application qualification, output/input/resize end-to-end latency, release microbenchmark re-profiling, process/GPU residency and repeated active/hidden-session lifecycle measurements. These require the combined history/configuration delivery and T0004's runtime/performance work; model snapshots prove admitted render input, not correct displayed glyphs or end-to-end performance. The independently deferred visible-pane scope remains T0005/#158. Neither Spec 0015's outstanding global acceptance nor #166 is closed by this bounded delivery.
