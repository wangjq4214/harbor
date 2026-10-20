# Establish reproducible measurements and calibrate numeric limits

**Ticket ID:** T0001
**Source:** [Spec 0031 R4–R6](../../spec/0031-bounded-history-and-performance-budgets.md), [ADR 0072](../../adr/0072-dual-bounded-terminal-history.md), [ADR 0073](../../adr/0073-session-history-accounting-and-text-unit-overflow.md).
**Status:** Todo

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

## Out of scope

Shipping live capacity reload, final text-unit protection, selecting a parser thread or broad storage rewrite, performance claims based solely on source/microbenchmarks, and simultaneously visible panes. Optimizations require a measured bottleneck and a bounded follow-up, not speculative work in this ticket.
