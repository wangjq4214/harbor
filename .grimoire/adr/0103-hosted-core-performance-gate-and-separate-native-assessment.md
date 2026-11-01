# Hosted core performance gate and separate native assessment

**Status:** Superseded
**Superseded by:** [ADR 0104](./0104-advisory-performance-report-in-a-single-pr-comment.md)
**Date:** 2026-11-01
**Sources:** Owner agreement during pre-merge quality-gate refinement to a first version using hosted Windows core A/B measurements, with native experience assessed separately; [current quality workflow](../../.github/workflows/windows-quality-gates.yml); [testing guide](../../docs/developer/testing.md).

## Context

The existing pre-merge workflow aggregates correctness and release-package validation into the stable `quality` status. Its Windows lanes use GitHub-hosted runners; they do not currently establish performance non-regression. The owner wants performance evaluated before merging.

Historical microbenchmark bounds have declared environment and measurement boundaries in [ADR 0075](./0075-calibrated-history-and-text-unit-limits.md). The remaining native performance measurements in [Spec 0031](../spec/0031-bounded-history-and-performance-budgets.md) were waived at closure, not established as passing native baselines.

## Decision

Use core A/B performance assessment on hosted Windows as the first version of the pre-merge performance gate, while keeping native experience assessment separate. Integrate the performance gate with the existing quality-gate structure rather than treating downloadable reports alone as merge approval.

A core measurement pass is scoped to its measured workloads and environment. It must not be represented as proof of native input/output-to-present latency, resize-to-correct-frame latency, or physical-GPU performance. Existing WARP correctness tests do not replace those native measurements.

## Consequences

- The first version can build on existing core measurement tools and the quality workflow without claiming complete desktop performance coverage.
- Native experience remains a separately identified assessment boundary; separation does not establish a passing result or waive future native obligations.
- Exact required workloads, metrics, statistical rules, numeric thresholds, runner calibration and native assessment triggers remain unsettled requirements. This record does not choose them or assert that the performance gate is implemented.
