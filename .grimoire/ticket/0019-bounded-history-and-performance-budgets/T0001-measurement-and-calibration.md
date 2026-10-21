# Establish reproducible measurements and calibrate numeric limits

**Ticket ID:** T0001
**Source:** [Spec 0031 R4–R6](../../spec/0031-bounded-history-and-performance-budgets.md), [ADR 0072](../../adr/0072-dual-bounded-terminal-history.md), [ADR 0073](../../adr/0073-session-history-accounting-and-text-unit-overflow.md).
**Status:** In progress — numeric contract adopted ([ADR 0075](../../adr/0075-calibrated-history-and-text-unit-limits.md)); native/host-matrix gaps remain

## Execution evidence (partial)

The [2026-10-20 evidence ledger](../../../docs/developer/evidence/0031/README.md) preserves repeated test-only history/Unicode candidate observations, parser/prepare captures, source/environment provenance and explicit gaps. The [protocol](../../../docs/developer/history-calibration.md) supplies synthetic workloads, process sampling and phase/topology instructions. No acceptance box is checked: end-to-end/native/GPU coverage and every required shipping numeric selection remain incomplete. T0002/T0003/T0004 numerical gates remain blocked; this is not a completed calibration outcome.

The [2026-10-21 calibration capture](../../../docs/developer/evidence/0031/calibration/README.md) repeats the deterministic candidate battery on the ADR 0074 stage-1 build (three processes per command, raw stdout preserved) and derives a recommended numeric contract with rationale: default 4000 / maximum 16000 history rows, default 4 MiB / maximum 16 MiB retained content bytes, a 1024-byte fixed per-unit protective limit, and microbenchmark-scope regression bounds for ingest, OSC, prepare and ring construction. The owner adopted those values on 2026-10-21 as [ADR 0075](../../adr/0075-calibrated-history-and-text-unit-limits.md), so T0002/T0003 may consume them; this ticket still does not implement them. The native output/presentation, resize-latency, idle CPU/GPU, residency and effects/lifecycle rows are still unsupplied, so T0004's numerical integrated acceptance remains blocked independently of the value decision.

Subsequent [Computer Use measurements](../../../docs/developer/evidence/0031/gui-20261020/README.md) add nine real output bursts, limited active/hidden sessions, font reload/resize and process-retirement samples. Unicode produced a long responsiveness interruption; an all-hidden-output automation attempt was invalidated. This extends baseline evidence but does not supply the full R5 matrix, exact presentation latency, GPU residency or required numerical contract.

## Goal

Produce independently repeatable current-tree baseline measurements and a usable numerical contract for shipping dual history bounds, fixed text-unit protection and current-scope performance/resource acceptance. No numerical shipping choice is assumed from the existing 1000-row constant or historical targets.

## Affected surfaces

- Existing `monospace_prepare` and parser microbenchmarks; profiling scripts/procedures.
- Host/input/presentation, resize, session lifecycle, render/upload and font/atlas observation surfaces as needed for the required metrics, without prescribing an instrumentation architecture.
- Bounded history/text-unit candidate fixtures or probes; durable calibration evidence and `docs/developer/profiling.md`.

## Approach

Reuse existing observation tools where their coverage is adequate; supply reproducible workload/measurement coverage for missing end-to-end metrics. Separate uninstrumented release timing, heap allocation capture, Windows process residency and GPU observation. Use bounded candidate fixtures/probes to compare capacities and exceptional text growth without requiring shipping configuration/defaults first.

Cover Spec 0031 R5's one-visible and active-plus-hidden-tab matrix, recording ordinary Unicode controls, large history, colored output, CJK/emoji, font reload, effects comparisons, idle and resource retirement. Record what each tool actually measures. Derive values with rationale and workload/environment applicability, not a threshold chosen merely to turn an observed failure into PASS.

## Dependencies and coordination

- **Blocked by:** None. Numeric-dependent shipping implementation is downstream, not a prerequisite for bounded calibration.
- **Blocks:** T0002 protective-limit acceptance; T0003 shipping defaults/maxima; T0004 numerical integrated acceptance.
- **Coordination risks:** Shared instrumentation and fixtures; distinguish probes from production behavior and instrumented from uninstrumented runs. Coordinate measurement hooks with the eventual consumers rather than freezing new APIs.

## Acceptance

- [ ] Another maintainer can repeat the named workloads using the recorded commands, inputs, build/features, machine/Windows/ConPTY/application/tool versions, GPU/backend, font/DPI/grid, session visibility, configuration, phase/dwell and sampling details.
- [ ] Comparable baseline procedures/captures cover output throughput, input-to-present and resize latency, idle CPU/GPU, frame/upload work, process memory, and atlas/GPU residency across the R5 matrix. Unavailable scenarios/metrics are explicit gaps; numerical acceptance depending on them remains blocked, not inferred from microbenchmarks.
- [ ] Repeated measurements report distributions/variability and cold/warm or peak/settled distinctions where applicable. DHAT elapsed time and allocation-site maxima are not misreported as latency or instantaneous process peaks.
- [ ] Durable calibration supplies the default row/content capacities, maximum accepted configuration values, fixed per-unit protective limit, and numerical performance/resource thresholds with rationale and applicability. Candidate values and fixed-grid/metadata/native/GPU/temporary-overlap costs are distinguishable.
- [ ] Bounded text-unit probes include ordinary combining/VS/ZWJ controls and exceptional growth. The selected limit supports the approved prefix/discard policy without redefining normal Unicode semantics.
- [ ] Required values are usable by downstream work; any unsupplied value or missing measurement keeps the corresponding gate visibly incomplete rather than delegating calibration again without an outcome.
- [ ] Summary and supporting artifacts meet R6 confidentiality/provenance/retrievability requirements. Relevant checks and standard gates are run or explicitly accounted for.

**Status of these boxes (2026-10-21):** none is checked yet; three are partially met, and the rest are not.
`Durable calibration supplies…` is met for the capacity, per-unit and microbenchmark-threshold values — adopted in [ADR 0075](../../adr/0075-calibrated-history-and-text-unit-limits.md) — and unmet for native/resource thresholds.
`Repeated measurements report distributions…` is met for the deterministic battery (3 processes × 7 observations per case, first samples preserved, microbenchmark median/p95, no DHAT-derived latency claims) and unsupplied for host cold/warm and peak/settled phases.
`Required values are usable by downstream work` is met for T0002/T0003's numbers and unmet where T0004 needs native thresholds.
`Another maintainer can repeat…` holds for the deterministic battery only; the host-workload, sampler and GPU rows have no recorded capture yet.

## Out of scope

Shipping live capacity reload, final text-unit protection, selecting a parser thread or broad storage rewrite, performance claims based solely on source/microbenchmarks, and simultaneously visible panes. Optimizations require a measured bottleneck and a bounded follow-up, not speculative work in this ticket.
