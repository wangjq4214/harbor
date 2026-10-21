# Bound exceptional text-unit growth without changing normal Unicode

**Ticket ID:** T0002
**Source:** [Spec 0031 R3–R4](../../spec/0031-bounded-history-and-performance-budgets.md), [ADR 0073](../../adr/0073-session-history-accounting-and-text-unit-overflow.md), [Unicode contract and its protective exception](../../spec/0015-unicode-terminal-text-correctness.md).
**Status:** Done

## Goal

Prevent unbounded individual retained-text-unit growth in live primary and alternate content. Preserve the admitted prefix at the calibrated fixed limit, discard further extensions of that unit until the next independent unit, and keep every content consumer consistent.

## Affected surfaces

- Terminal text admission and cell storage, particularly `screen/edit/cell_writer.rs` and `model.rs`.
- Fragmented parser-to-screen ingestion, screen edits, reflow/anchors and the existing read/search/render/copy consumers.
- Deterministic terminal tests and applicable documentation of the deliberately lossy oversized-input exception.

## Approach

Apply the approved fixed protective limit before unlimited suffix accumulation, not only when rows enter history. Discarded extensions must not become retained orphan units or synthetic source text. Preserve normal within-limit combining, variation-selector, ZWJ, width and font-fallback semantics. Use the same admitted prefix for rendering, copy, search and reflow; do not add an alternate representation that reconstructs dropped text.

Choose internal mechanics within existing ownership boundaries. The exception does not repair or authorize the separate no-wrap final-column selector-width discrepancy.

## Dependencies and coordination

- **Blocked by:** nothing for the numeric limit — the 1024-byte protective limit and its rationale were adopted on 2026-10-21 ([ADR 0075](../../adr/0075-calibrated-history-and-text-unit-limits.md), [calibration capture](../../../docs/developer/evidence/0031/calibration/README.md)). Real unit recognition, streaming admission and discard recovery remain this ticket's own work.
- **Blocks:** T0004 integrated protection/consumer/runtime acceptance.
- **Coordination risks:** Shared cell/screen/tests with T0003. Agree how admitted text is consumed by history UTF-8 accounting; parallel work is permitted after calibration, with shared-file coordination rather than an artificial blocker.

## Acceptance

- [x] Live primary and alternate ingestion enforce the calibrated fixed limit before unbounded suffix growth; repeated excess extensions do not accumulate retained source text.
- [x] The already admitted prefix remains intact. Further extensions of that same unit are discarded without orphan/synthetic source units, and the next independent text unit resumes normal admission.
- [x] Different PTY fragment boundaries yield the same accepted prefix/discard behavior and cannot bypass the cap.
- [x] Copy, search/read, render and reflow observe the same surviving text, including after edits, history movement and resize; discarded extensions are not advertised as recoverable.
- [x] Ordinary combining/VS/ZWJ, wide-cell, isolated-mark and fallback controls within the cap retain the existing contract, source text and applicable anchor/damage behavior.
- [x] Tests distinguish this approved safety exception from unsupported-font presentation and the existing no-wrap discrepancy. Evidence does not claim either unrelated gap was fixed.
- [x] Focused deterministic results, relevant native checks or explicit not-run reasons, numeric provenance and standard gates are recorded. Final broad Windows/performance acceptance remains T0004.

## Delivery evidence

Implemented against `bfde989d88baebfd4c10552d540813f7d846a5c0` plus the T0002 worktree changes. [Focused evidence and gate results](../../../docs/developer/evidence/0031/text-admission/README.md) map each criterion to deterministic coverage and record native checks, the initial non-reproduced GPU font-replacement failure, and explicit not-run reasons. Eight focused admission tests and the final workspace/fmt/clippy/docs gates pass. `Cell` remains 56 B on Windows x64; admitted source is still `ch + suffix` for T0003's UTF-8 accounting. Completion is this bounded ticket only: broad native/performance acceptance remains T0004, the GPU reproducibility concern remains needs-verification, and neither font support nor the no-wrap discrepancy is claimed repaired.

## Out of scope

History configuration/reload, arbitrary trimming of normal units, a new Unicode representation/shaping architecture, and unrelated no-wrap width repair.
