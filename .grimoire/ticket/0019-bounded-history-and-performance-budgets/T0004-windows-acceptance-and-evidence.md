# Verify the combined current-scope Windows delivery and resource budgets

**Ticket ID:** T0004
**Source:** [Spec 0031 R1–R6 and end-to-end verification](../../spec/0031-bounded-history-and-performance-budgets.md), [ADR 0073](../../adr/0073-session-history-accounting-and-text-unit-overflow.md), [profiling procedures](../../../docs/developer/profiling.md).
**Status:** Todo

## Goal

Establish reproducible integrated Windows correctness and measured resource/performance acceptance for configurable bounded history plus text-unit protection in one-visible and active-plus-hidden-tab sessions. Publish only implemented, evidenced behavior and preserve explicit failures/exclusions.

## Affected surfaces

- T0001 measurement tooling, bounded workloads and baseline/calibration artifacts.
- Combined host/ConPTY/terminal/render/font/configuration/session lifecycle paths delivered by T0002/T0003.
- Durable verification summaries/retrievable raw captures, profiling/user documentation and applicable progress/roadmap claims.

## Approach

Execute the spec's end-to-end matrix against the integrated revision under comparable conditions. Correlate actual input-to-present and resize outcomes, throughput, idle CPU/GPU, frame/uploads, process private bytes and atlas/GPU residency; do not substitute parser/prepare microbenchmarks, resource-size arithmetic or DHAT timing.

Include capacity shrink/growth and rejected candidates across active/hidden/saved-primary sessions, exceptional and normal Unicode content, reflow/copy/search, colored/CJK/emoji output, font reload, effects on/off and repeated session creation/switch/close/retirement. Retain known unrelated Unicode discrepancies separately.

If the calibrated budgets are missed, record FAIL and the measured bottleneck. Return a bounded, evidence-backed optimization/follow-up rather than selecting an unprofiled parser thread/storage rewrite or silently moving the acceptance threshold. Repeat comparable captures after any authorized optimization and re-profile before choosing another.

## Dependencies and coordination

- **Blocked by:** T0002's shipping protection and T0003's shipping dual bounds/reload, plus the native measurement/threshold set that T0001 has not supplied (output-to-present, resize latency, idle CPU/GPU, residency, atlas). The capacity/per-unit values and microbenchmark regression bounds were adopted on 2026-10-21 ([ADR 0075](../../adr/0075-calibrated-history-and-text-unit-limits.md)) and are usable now; native acceptance must not be inferred from them.
- **Blocks:** Current-scope delivery/acceptance claims, not pane capability.
- **Coordination risks:** Mixed font/budget reload and native recovery, instrumentation overhead, hidden-session retirement and separate GPU/native lifetimes require careful phase attribution.

## Acceptance

- [ ] The combined spec R1–R3 scenarios have deterministic and applicable Windows runtime evidence, including real ConPTY input/output/presentation, repeated resize, normal/exceptional Unicode, clipboard/copy/search and primary/alternate/saved-primary transitions.
- [ ] One-visible and active-plus-hidden-tab captures cover all R5 workloads and required metrics. Repeated distributions/variability and cold/warm, idle, peak/settled and retirement observations are compared with T0001's applicable calibrated budgets.
- [ ] Each scenario states expected versus observed behavior and PASS/FAIL/NOT RUN/BLOCKED. Failed or unavailable evidence is not hidden behind passing model tests, an unrelated capture or a favorable rerun.
- [ ] Capture provenance includes revision/dirty scope, build/features, Windows/ConPTY/applications/tools, GPU/backend, fonts/DPI/grid, session visibility, capacities/configuration and exact reproducible workload/sampling/phase conditions. Summaries and supporting artifacts are durable, retrievable and confidentiality-safe.
- [ ] Allocation churn/live heap, process/native memory and GPU/atlas observations remain distinct; history bytes or calculated texture size are not misreported as total residency.
- [ ] Relevant focused checks and all current standard gates are run or explicitly accounted for with reasons. Native/performance gaps remain open despite successful format/Clippy/test/docs results.
- [ ] Configuration/profiling/status documentation claims only evidenced delivered scope. A failure produces an explicit remaining-work disposition; the accepted current delivery is not declared fully passing until its criteria are evidenced or scope disposition is explicitly approved.
- [ ] The deferred simultaneously-visible-pane acceptance remains linked as [T0005](./T0005-deferred-visible-pane-acceptance.md), [#158](https://github.com/wangjq4214/harbor/issues/158) and [#166](https://github.com/wangjq4214/harbor/issues/166). It is neither PASS nor silently excluded when reporting #166 closure eligibility.

## Out of scope

Implementing panes, treating active/hidden tabs as multi-visible-session proof, revalidating every unrelated terminal compatibility claim, or inventing optimizations/thresholds without measured justification. This ticket is a future execution contract, not authorization to run QA during artifact authoring.
