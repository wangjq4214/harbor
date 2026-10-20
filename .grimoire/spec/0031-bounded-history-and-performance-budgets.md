# Bounded history and performance budgets (N15 / #166)

**Spec ID:** 0031
**Status:** Draft — settled requirements; calibration, implementation and runtime acceptance pending
**Date:** 2026-10-20
**Sources:** [Issue #166 — History capacity and performance budgets](https://github.com/wangjq4214/harbor/issues/166); approved refinement handoff, including staged measurement and active/hidden-tab scope; [ADR 0072](../adr/0072-dual-bounded-terminal-history.md) as refined by [ADR 0073](../adr/0073-session-history-accounting-and-text-unit-overflow.md).
**Execution set:** [Ticket relationships and order](../ticket/0019-bounded-history-and-performance-budgets/README.md), including the [deferred visible-pane acceptance](../ticket/0019-bounded-history-and-performance-budgets/T0005-deferred-visible-pane-acceptance.md).

This contract specifies the approved semantics and calibration process, not completed bounded-resource delivery. Numeric calibration is required execution work, not a request to reopen the approved direction. Neither existing tooling nor historical targets establish current end-to-end results.

## Requirements

### R1 — Independent per-session history bounds

Provide configurable primary-history physical-row and retained-source-content byte limits. Each session independently enforces both; neither is a shared process-wide allowance. The byte count measures retained history source text in UTF-8. Primary history saved behind an active alternate screen remains governed. Live viewport and alternate-screen content are not primary scrollback and are excluded from the history-content byte count.

When either limit is exceeded, evict oldest physical history rows until both limits are satisfied, retaining newest content. Apply this contract to output-driven history growth and primary reflow/resize, including saved primary. Do not split an ordinary text unit to fit a history budget. Preserve existing partial-logical-line `head_truncated` metadata, content-anchor invalidation, whole-selection invalidation when an endpoint is lost, and review fallback to oldest retained content. Ring-slot reuse must not make evicted anchors identify unrelated text.

Keep the primary reflow / alternate rectangular-resize distinction and the existing ConPTY live-primary compatibility policy in [Spec 0014](0014-content-preserving-resize-reflow.md). These bounds extend capacity enforcement, not logical identity or buffer interpretation.

The content-byte ceiling is not a whole-process memory ceiling. Fixed/preallocated grid and metadata storage, allocator capacity, temporary preparation/reflow overlap, live/alternate content, native/font caches and GPU resources require separate measurement (R5).

### R2 — Complete-candidate live budget changes

History settings use the established startup and strict complete-candidate manual/watched reload contracts, not a separate partial-acceptance path. Accepted changes apply coherently to applicable existing sessions, including hidden sessions and saved primary history, and future sessions.

A successful shrink immediately evicts oldest excess history at acceptance. An increase permits future retention but never resurrects evicted content. A rejected candidate must not prematurely evict history under its proposed smaller budgets or publish them to future-session creation. Missing, malformed, invalid or failed candidates retain the existing reload policy in the [configuration reference](../../docs/app/configuration.md).

Preserve [ADR 0059](../adr/0059-atomic-manual-reload-and-terminal-only-fonts.md), its [normal bounded font-resize qualification](../adr/0060-font-reload-uses-normal-bounded-resize.md), and [ADR 0061's native compensation-failure qualification](../adr/0061-manual-reload-compensation-failure.md). The latter retains old accepted settings, resumes readers and reports recovery failure honestly; it does not authorize acceptance or eviction under a rejected history budget. Ordinary resize/recovery effects must not be misreported as successful budget application. [ADR 0062](../adr/0062-watched-reload-and-new-session-launch-settings.md) supplies the shared watched/manual acceptance boundary.

### R3 — Protective oversized-text-unit exception

Enforce a fixed protective limit on an individual retained text unit before unbounded live suffix growth can occur, including live primary and alternate content. Once protection triggers, retain the admitted prefix and discard further sequence extensions belonging to that unit until the next independent text unit. Do not retain discarded extensions as orphan units or insert synthetic source content. The next independent unit resumes normal processing.

Rendering, copy, search and reflow must consume the same admitted prefix; discarded extensions are not recoverable source text. PTY fragmentation must not bypass the limit or change that outcome. Normal combining, variation-selector and ZWJ units within the limit retain the original text/width/fallback contract. Unsupported font presentation is not a reason to discard text.

This is only [ADR 0073's](../adr/0073-session-history-accounting-and-text-unit-overflow.md) approved future exception to unqualified full retention in [ADR 0044](../adr/0044-unicode-text-unit-and-presentation-width-policy.md) and [Spec 0015](0015-unicode-terminal-text-correctness.md). It is not implemented by this document and does not resolve or authorize the unrelated DECAWM-disabled final-column width discrepancy documented in Spec 0015.

### R4 — Required numeric calibration outcome and gate

Before numeric-dependent shipping implementation or acceptance, produce durable, reproducible calibration evidence selecting, with rationale:

- the default physical-row and UTF-8 history-content byte capacities;
- maximum accepted configuration values for both capacities;
- the fixed text-unit protective limit; and
- numeric performance/resource thresholds and their workload/environment applicability for R5.

Compare candidate history sizes and text-unit sizes, including ordinary Unicode controls and exceptional growth, and account for fixed storage and temporary overlap rather than assuming source bytes equal residency. Record candidate values, measurement basis, expected/observed results, variability and why the selected values bound the intended workloads. The calibration outcome must supply usable values and associated acceptance criteria, not just a recommendation to measure later. Without it, numeric-dependent work/acceptance remains BLOCKED or NOT RUN, never PASS by an invented threshold.

Calibration may use bounded test fixtures/probes for candidate sizes; it does not require final shipping defaults, a full live-configuration implementation, or a particular harness architecture. This staged prerequisite must not create a circular dependency. The current hardcoded 1000-row capacity is a baseline fact only, not a selected future default or a sufficient byte/resource bound.

### R5 — Repeatable performance and resource acceptance

Capture a comparable baseline before choosing optimizations, and repeat the same workload after each optimization. Identify the measured bottleneck and re-profile before selecting further changes. Do not infer cost or savings from source inspection alone.

Cover both one visible session and an active visible tab with retained hidden-tab sessions. The workload matrix must include large history; sustained and colored/dirty-range-heavy output; CJK/emoji; terminal-font reload; visual effects on/off; and repeated session opening, switching, closing and resource retirement. Record cold and warm phases separately where applicable, including idle and settled post-retirement observations rather than treating immediate tab close as resource release.

Measure output throughput, input-to-present latency, resize latency, idle CPU/GPU, frame/upload work, process memory, and atlas/GPU residency. Keep measurement boundaries explicit: parser throughput, prepare-only work and allocation instrumentation are not end-to-end input/presentation latency. Report repeated samples and distributions/variability, not only a favorable single sample. Apply the calibrated thresholds to their declared scenarios without silently changing workload/configuration between comparisons. Preserve content, rendering, incremental/full-upload correctness and idle behavior while optimizing.

Distinguish allocation churn/count, global live-heap peak, process private-byte peak/settled residency, native/font allocations and GPU/atlas residency. DHAT elapsed time is not latency evidence; separate per-allocation-site maxima are not an instantaneous global peak. An enforced history-content ceiling cannot substitute for any of these measurements.

### R6 — Honest evidence and closure boundary

Each calibration/acceptance summary must be self-contained and durable, with retrievable supporting artifacts. Record revision and dirty-tree scope; build profile/features; machine/Windows/ConPTY and relevant application versions; GPU/backend; fonts/DPI; viewport/grid; session counts and visibility; complete relevant configuration/capacities; workload inputs/commands, duration and phase boundaries; tool/version/mode, sampling interval and sample count; expected versus observed outcomes; thresholds; distributions/variability; deviations and limitations. Mark each scenario PASS, FAIL, NOT RUN or BLOCKED honestly. Preserve comparable raw captures and enough procedure to repeat them; unavailable artifacts/metrics are explicit gaps, not inferred results. Protect terminal payloads, shell/environment values and other sensitive data in shared evidence; retain necessary artifacts through confidentiality-appropriate retrievable storage.

Simultaneously visible panes are explicitly deferred because pane workspace is not implemented. Retain named follow-up acceptance for that scenario linked to [#158 — Pane workspace with independent sessions](https://github.com/wangjq4214/harbor/issues/158) and #166 before #166 closure. Do not mark it PASS, silently omit it from the package boundary, or describe active/hidden tabs as full multi-visible-session acceptance. This spec does not create or update GitHub issues.

## Solution and integration contracts

Extend existing bounded history and shared retained-text semantics; reuse the existing complete-candidate reload boundary. Internal storage layout, configuration names and instrumentation APIs are not prescribed. The following exchanges are correctness/acceptance seams, not a new architecture:

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Settings acceptance | Configuration/reload → existing session state and future-session creation | Complete validated candidate; all required preparation succeeds before acceptance | Coherent accepted budgets; immediate successful shrink; no rejected-budget eviction or partial publication |
| History retention | Output and primary resize → primary/saved-primary history | Per-session row/content budgets and existing logical metadata | Newest retained physical rows within both limits; truncation, anchor/selection invalidation and review fallback |
| Text admission | Fragmented parser output → retained primary/alternate units | Fixed calibrated protection, existing sequence semantics | Same admitted prefix across fragments; discarded extensions do not become source units; recovery at next independent unit |
| Retained-content consumers | Screen/history → render, copy, search and reflow | Shared admitted text and existing anchors/projections | Agreement on surviving content; no reconstruction of discarded text or stale-target aliasing |
| Resource observation | Session/render/font lifecycle → performance evidence | Declared workload, phases and tool coverage | Comparable latency, work and residency observations through retirement, without substituting one metric for another |

### Current surfaces and evidence limits

Source inspection baseline: `439fcf0dd00a71a2b4b99d8321a75906337d8d72`, with the linked refinement records supplying approved future policy, not runtime evidence.

- [NormalBuf](../../crates/harbor-terminal/src/normal_buf.rs) currently hardcodes 1000 scrollback rows and preallocates the full ring's cells/parallel state. [Primary reflow](../../crates/harbor-terminal/src/primary_reflow.rs), [screen](../../crates/harbor-terminal/src/screen.rs) and [content anchors](../../crates/harbor-terminal/src/content_anchor.rs) supply existing retention/resize semantics. Larger row settings cannot be assumed cheap.
- The [cell writer](../../crates/harbor-terminal/src/screen/edit/cell_writer.rs) currently extends retained suffix text; [read model](../../crates/harbor-terminal/src/read_model.rs) and [literal search](../../crates/harbor-terminal/src/literal_search.rs) are existing consumers, not a requirement to invent a new search representation.
- [Configuration parsing](../../crates/harbor-config/src/lib.rs) and [terminal configuration transactions](../../crates/harbor-terminal/src/configuration.rs) are existing integration surfaces; the [configuration guide](../../docs/app/configuration.md) does not yet advertise these history controls.
- [`monospace_prepare`](../../crates/harbor-terminal/examples/monospace_prepare.rs) is a release prepare-only benchmark excluding draw/PTY/readback. [`osc_benchmark`](../../crates/harbor-terminal/src/parser/osc_benchmark.rs) is a parser microbenchmark. Neither proves the R5 end-to-end outcomes.
- [Profiling procedures](../../docs/developer/profiling.md), [DHAT analysis](../../scripts/profiling/dhat_analyze.py), [drill-down](../../scripts/profiling/dhat_drill.py), [font lifecycle markers](../../crates/harbor-text/src/lifecycle.rs) and [host telemetry](../../src/telemetry.rs) provide tools/procedures, not current results. The [roadmap](../../docs/developer/roadmap.md) and [progress](../../docs/developer/progress.md) preserve outstanding measured acceptance and pane scope.

## End-to-end verification

These are required future executions, not results of spec authoring. Use calibrated values (or bounded candidate fixtures during calibration) rather than inventing shipping numbers.

| Scenario / input | Expected observable outcome |
| --- | --- |
| Emit distinguishable old/new rows until the row cap alone is exceeded; repeat with multibyte/sequence-rich source exceeding the byte cap before the row cap, then with both exceeded | Each session meets both history limits through oldest-row eviction; newest content survives; history units are not split to fit bytes. Independent sessions do not consume one another's allowance. |
| Evict only a logical line's oldest prefix while reviewing/selecting it; continue output through ring reuse and narrow/widen repeatedly | Retained suffix is head-truncated; evicted anchors do not alias new content; lost selection endpoint invalidates the selection; evicted review falls back to oldest retained content. Copy/search/reflow agree on survivors under existing resize policies. |
| Fill primary history, enter alternate, produce alternate output and resize; exit alternate | Saved primary remains dual-bounded; live/alternate content is excluded from history bytes but measured separately. Primary follows its existing reflow/ConPTY policy; alternate remains rectangular without scrollback. |
| Manually reload and separately save/watch a valid smaller budget across active/hidden/alternate sessions, then increase it and open a new session | Shrink takes effect immediately at successful complete acceptance, including saved primary; increase does not restore old text; existing and new sessions use accepted budgets without session/PTY recreation. |
| Submit smaller budgets alongside an invalid recognized field, malformed document or failed preparation; exercise later-session/native failure and compensation paths | Rejected budgets do not cause premature eviction or partial publication. Accepted settings/future creation remain old; readers resume and recovery failures follow ADR 0061 without a false restoration/success claim. Account separately for ordinary bounded geometry recovery. |
| Stream a base with exceptional combining/selector/ZWJ extensions past the protective limit in live primary, alternate and later history; repeat with different PTY fragment boundaries; then send an independent unit | Retained size stops at the admitted prefix before unbounded growth; discarded extensions are neither orphaned nor synthetic source text; all consumers agree; fragmentation does not bypass protection; normal admission resumes at the independent unit. |
| Send ordinary combining/VS/ZWJ and wide-edge content within the limit; copy/search, resize, reload fonts and inspect presentation | Ordinary text/width/fallback semantics remain unchanged; display fallback is not source loss. Explicitly retain Spec 0015's known no-wrap width discrepancy as unresolved, not as this exception. |
| Execute the R5 workload/metric matrix on the baseline and changed build, with repeated session lifecycle/retirement and effects comparisons | Comparable distributions and residency/work observations meet calibrated budgets or explicitly report failures/unavailable cases; no DHAT or microbenchmark substitution for end-to-end latency. |

## Verification and acceptance record

Calibration must precede numeric-dependent acceptance; functional/model evidence alone cannot close resource or Windows runtime acceptance. Cover output, reflow, saved-primary handling, reload success/failure and fragmented text admission deterministically, then record integrated Windows output/input/resize, copy/search and lifecycle observations for the relevant scenarios above. Existing unresolved acceptance in Specs 0014/0015 is not declared complete by this work.

For execution, run or explicitly account for the current gates:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --workspace`
- `python scripts/docs/check_docs.py`

Passing these gates is not performance or native acceptance. `scripts/checklist_summary.py` is removed and is not a gate; missing historical optimization-plan/validation paths and stale issue links do not supply requirements or evidence. Spec authoring authorizes only artifact validation, not benchmark/QA execution or numerical claims.

## Decisions and exclusions

- **Dual limits, oldest-first physical eviction, separate memory evidence:** ADR 0072 refined by ADR 0073; retain existing ring/anchor direction rather than selecting a replacement storage design.
- **Per-session/saved-primary accounting, irreversible live shrink, fixed admitted-prefix protection:** ADR 0073. This narrowly qualifies full text retention, not ordinary Unicode width or font fallback.
- **Measurement-derived values and repeated profiling:** ADRs 0072/0073 and the approved handoff; no arbitrary defaults, maxima, safety cap or performance thresholds are chosen here.
- **One visible session and active/hidden tabs now; visible panes later:** ADR 0073 and the approved #158/#166 linkage. Deferred pane acceptance must remain discoverable before #166 closure.
- Unlimited history, a speculative parser thread, and broad storage rewrites without measured bottleneck evidence are not authorized. No new pane workspace, generalized search feature, metrics API, storage architecture or repair of the independent no-wrap width discrepancy is required by this spec.
