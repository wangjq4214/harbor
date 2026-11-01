# Advisory performance report in a single PR comment

**Status:** Testing
**Date:** 2026-11-01
**Supersedes:** [ADR 0103](./0103-hosted-core-performance-gate-and-separate-native-assessment.md)
**Sources:** Owner refinement: downloadable reports are not required; publish results in a PR comment and update that same comment on subsequent runs. Performance degradation should be reported, not block merging.

## Context

ADR 0103 proposed integrating hosted Windows core A/B performance assessment as a required pre-merge gate. The owner clarified that performance changes are informational, not grounds to block a merge, and prefers an updated PR comment rather than downloadable reports.

## Decision

Keep hosted Windows core A/B performance assessment separate from the existing required correctness and packaging gates. Report performance changes without making degradation a failure of the required `quality` status.

Maintain one performance-report comment per PR. Create it when absent and update it on subsequent runs, including reruns, instead of posting a new report comment each time. A downloadable report is not required for acceptance; the PR comment is the requested presentation surface.

Preserve the measurement boundary: core A/B results describe only the measured workloads and environment. Native experience is assessed separately; core or WARP correctness results do not establish native presentation latency or physical-GPU performance.

## Consequences

- Existing required correctness and release-package checks remain required. Performance reporting must not weaken them or claim to be an additional blocking gate.
- The performance report presents observed changes rather than a merge-approval decision. It does not require a numeric degradation threshold for blocking.
- PR measurement runs with read-only permissions. A separate `workflow_run` publisher executes default-branch code only, validates bounded attempt-specific JSON data against the source run and current PR, and renders the comment without executing PR code or trusting supplied Markdown.
- No downloadable report is required. A one-day internal artifact bridges the permission boundary; the PR comment and workflow summary are the presentation surfaces. Exact shared workloads and sampling/report semantics are documented in the [testing guide](../../docs/developer/testing.md#advisory-pr-performance-report). Stale SHAs/attempts and older published results are rejected. Missing or invalid measurement data is reported as unavailable; publication failures remain outside `quality`. Deployment requires the publisher on the default branch, and native/GitHub execution must not be inferred from local tests.
