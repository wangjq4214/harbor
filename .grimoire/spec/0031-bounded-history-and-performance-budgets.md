# Bounded history and performance budgets (N15 / #166)

**Spec ID:** 0031
**Status:** Accepted (owner-confirmed)
**Date:** 2026-10-20
**Sources:** [Issue #166 — History capacity and performance budgets](https://github.com/wangjq4214/harbor/issues/166); approved measurement and active/hidden-tab scope; [ADR 0072](../adr/0072-dual-bounded-terminal-history.md) as refined by [ADR 0073](../adr/0073-session-history-accounting-and-text-unit-overflow.md).
**Stable aliases:** 0019/T0001 = calibration; T0002 = text-unit protection; T0003 = dual-bounded history/reload; T0004 = Windows matrix; T0005 = pane measurements (waived).

## Delivery and closure summary

**Owner closure decision (2026-10-21):** remaining native/performance measurements and acceptance matrix, including deferred pane measurements, were waived. Pane implementation remains separate under [#158](https://github.com/wangjq4214/harbor/issues/158).

Windows x64 `Cell` layout remained 56 bytes, not a process-memory ceiling.

### Calibration and bounded optimization results

The owner adopted [ADR 0075](../adr/0075-calibrated-history-and-text-unit-limits.md) on 2026-10-21:

| Setting | Default / fixed value | Maximum configuration |
| --- | --- | --- |
| Primary-history physical rows | 4000 | 16000 |
| History UTF-8 source bytes | 4194304 (4 MiB) | 16777216 (16 MiB) |
| Individual retained text unit | 1024 UTF-8 source bytes, including lead scalar | Fixed, not user-configurable |

Calibration used release/default-feature builds on Windows 11 build 26300, i5-13600KF, with 24 visible rows and 80/160 columns; three processes and seven observations per storage/unit case. Owned ring backing at 4000 history rows was approximately 18–37 MB, with construction medians 2.97–5.97 ms and rectangular-resize medians 4.22–8.83 ms. At 16000×160, backing was about 147 MB and modeled old-plus-new overlap about 295 MB. These exclude suffix heaps, allocator/native/GPU costs and are not measured total residency or ConPTY resize latency.

Ordinary combining/VS/ZWJ controls survived the candidate limits; 64/256-byte candidates truncated extended controls, while 1024 bytes provided the adopted safety margin. OSC parser medians were approximately 98 MiB/s; prepare-only clean median was 4.7 µs and dirty/full medians approximately 2.35–2.42 ms. ADR 0075 retains the applicable microbenchmark regression bounds, not native presentation budgets.

[ADR 0074](../adr/0074-demand-driven-content-anchor-tracking.md) stage 1 removed unnecessary full retained-content projections when no anchor consumer exists. For the same 2000-row Unicode, 107×32, row-cadence ingestion-only workload, parser/screen time fell from 4.08–4.42 s to 41.1–41.7 ms; engine time fell from 5.75–6.12 s to 41.3–43.1 ms. Active selection/review/saved-anchor paths retain their previous projection cost; no incremental-projection rewrite or end-to-end latency improvement is established by those numbers.

### Verification and Windows observations

The isolated release/ConPTY smoke used Windows build 26300, bundled ConPTY 1.24.2607.10001, Python 3.12.10, requested Consolas 16, DPI 144 and observed grid 161×45. Three sequential cycles emitted 20000 Latin, 20000 colored and 2000 Unicode rows each; exceptional combining/ZWJ units were emitted on primary and alternate buffers. Primary restoration, independent recovery markers and final completion were observed. Four retained tabs/independent children were created; a watched font 16→20→16 and history shrink/growth smoke preserved child identities and visible tails. This did not audit exact desktop source retention, all hidden output, clipboard/search, failure paths or controlled fragmentation.

Limited process samples showed approximately 351–384 MiB private committed bytes for one session over 30 s and 749–750 MiB for four sessions over 35 s, with mean whole-machine CPU about 0.010% and 0.011% on 20 logical processors. The latter window mixed alternate restoration and idle; neither is a repeated/calibrated idle or output-peak scenario. Private bytes are not live heap, physical RAM, GPU residency or a per-tab budget. Earlier captures showed process-memory reduction after tab close, but did not establish leak freedom or comparable performance. Producer timings and automation durations were not input/output-to-present latency.

### Waived work and retained limitations

Native output/input/resize-to-correct-present budgets and complete repeated distributions were never supplied. The full two-topology sustained/dirty/Unicode, exact resize, font/alternate, effects-on/off, ten-cycle lifecycle and 0/30/120-second retirement matrix, DHAT/global live peak and GPU/atlas/upload/residency coverage were not completed. The owner waived them for closure; no favorable subset or invented threshold substitutes for them.

The known Spec 0015 no-wrap selector-width discrepancy remains. A native font-replacement GPU pixel comparison failed once during T0002 and passed on reruns; its cause was not established and it is not claimed repaired. Initial outdated search/history test fixtures were corrected before final passing suites; native timing/source coverage remains distinct from model assertions.

At the owner's request, raw logs, screenshots, CSV/JSON captures, manifests and copied smoke fixtures were deleted rather than merged. R4–R6 measurement/retention gates and unexecuted measurement scenarios below are historical requirements superseded by the closure waiver; R1–R3 functional semantics remain in force.

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

## End-to-end verification

This is the original verification matrix. Deterministic/model coverage and limited Windows observations are summarized above; the remaining native executions were waived by the owner, not marked PASS.

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

## Decisions and exclusions

- **Dual limits, oldest-first physical eviction, separate memory evidence:** ADR 0072 refined by ADR 0073; retain existing ring/anchor direction rather than selecting a replacement storage design.
- **Per-session/saved-primary accounting, irreversible live shrink, fixed admitted-prefix protection:** ADR 0073. This narrowly qualifies full text retention, not ordinary Unicode width or font fallback.
- **Measurement-derived values and repeated profiling:** ADRs 0072/0073 and the approved handoff; no arbitrary defaults, maxima, safety cap or performance thresholds are chosen here.
- **One visible session and active/hidden tabs now; visible panes later:** ADR 0073 and the approved #158/#166 linkage. Deferred pane acceptance must remain discoverable before #166 closure.
- Unlimited history, a speculative parser thread, and broad storage rewrites without measured bottleneck evidence are not authorized. No new pane workspace, generalized search feature, metrics API, storage architecture or repair of the independent no-wrap width discrepancy is required by this spec.
