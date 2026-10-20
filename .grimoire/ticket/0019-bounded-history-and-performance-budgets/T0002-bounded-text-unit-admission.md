# Bound exceptional text-unit growth without changing normal Unicode

**Ticket ID:** T0002
**Source:** [Spec 0031 R3–R4](../../spec/0031-bounded-history-and-performance-budgets.md), [ADR 0073](../../adr/0073-session-history-accounting-and-text-unit-overflow.md), [Unicode contract and its approved future exception](../../spec/0015-unicode-terminal-text-correctness.md).
**Status:** Todo

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

- **Blocked by:** T0001's usable measured protective limit/rationale before shipping acceptance.
- **Blocks:** T0004 integrated protection/consumer/runtime acceptance.
- **Coordination risks:** Shared cell/screen/tests with T0003. Agree how admitted text is consumed by history UTF-8 accounting; parallel work is permitted after calibration, with shared-file coordination rather than an artificial blocker.

## Acceptance

- [ ] Live primary and alternate ingestion enforce the calibrated fixed limit before unbounded suffix growth; repeated excess extensions do not accumulate retained source text.
- [ ] The already admitted prefix remains intact. Further extensions of that same unit are discarded without orphan/synthetic source units, and the next independent text unit resumes normal admission.
- [ ] Different PTY fragment boundaries yield the same accepted prefix/discard behavior and cannot bypass the cap.
- [ ] Copy, search/read, render and reflow observe the same surviving text, including after edits, history movement and resize; discarded extensions are not advertised as recoverable.
- [ ] Ordinary combining/VS/ZWJ, wide-cell, isolated-mark and fallback controls within the cap retain the existing contract, source text and applicable anchor/damage behavior.
- [ ] Tests distinguish this approved safety exception from unsupported-font presentation and the existing no-wrap discrepancy. Evidence does not claim either unrelated gap was fixed.
- [ ] Focused deterministic results, relevant native checks or explicit not-run reasons, numeric provenance and standard gates are recorded. Final broad Windows/performance acceptance remains T0004.

## Out of scope

History configuration/reload, arbitrary trimming of normal units, a new Unicode representation/shaping architecture, and unrelated no-wrap width repair.
