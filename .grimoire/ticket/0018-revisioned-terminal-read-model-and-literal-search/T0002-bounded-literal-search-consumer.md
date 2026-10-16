# Bounded Literal Consumer, Cancellation and Provenance

**Ticket ID:** T0002
**Source:** [Spec 0027](../../spec/0027-revisioned-terminal-read-model-and-literal-search.md), R2–R6, R8/R9 and Search work / Result acceptance and activation seams; [set constraints](./README.md#source-baseline-and-constraints).
**Status:** Todo

## Goal

Provide a headlessly verifiable literal-search consumer over T0001's boundary, returning bounded logical match ranges and explicit coverage/status with query and content provenance. Obsolete or text-stale results cannot become actionable; current queries make useful progress while output continues.

## Affected Surfaces

Search consumer and tests using the `harbor-terminal` bounded read/range/validity contract. Reuse existing logical text units and anchors, not mutable-screen traversal or an independently assembled full-history string. The eventual application supplies actual session/query context; this ticket establishes and tests the result/validation contract without fixing worker placement or module layout.

Sources: [T0001](./T0001-bounded-versioned-read-model.md), [ADR 0042](../../adr/0042-content-anchors-and-buffer-specific-resize.md), [ADR 0044](../../adr/0044-unicode-text-unit-and-presentation-width-policy.md), [ADR 0052](../../adr/0052-conpty-live-primary-producer-geometry-and-styled-tail-clipping.md).

## Approach

Consume explicit bounded portions with coherent continuation and identity/revision. Carry query provenance through completion, cancellation and validation. Define how completion, partial/incomplete coverage and stale work are distinguished. Exercise validation against current content, not just anchor resolvability, so T0003 can use the same provenance for decoration and navigation.

Document concrete read/copy/scan, query/result retention and cancellation limits with implementation tests. Do not select numeric limits, case options, a thread/cache algorithm or a new revision strategy in this decomposition.

## Dependencies and Coordination

- **Blocked by:** T0001: literal semantics, page continuation and result coordinates require its actual bounded/versioned contract.
- **Blocks:** [T0003](./T0003-current-session-search-integration.md): needs delivered result/provenance, cancellation, coverage and validation behavior for real host acceptance and navigation.
- **Coordination:** Preserve T0001's content contracts; expose enough status for T0003 not to present partial coverage as complete no-match. Headless validation tests do not replace host acceptance/activation tests.

## Acceptance

- [ ] **A1 — Literal semantics (R3/R6):** Search uses only the shared boundary, finds occurrences spanning soft wraps and bounded page edges, and never creates a match across explicit lines, including blank lines. Results refer to complete retained text units/logical anchors; generated padding and synthetic display cues are not searchable source text.
- [ ] **A2 — Result/validation contract (R2/R5/R6):** Results carry session, buffer/content generation/revision and query provenance, logical ranges and explicit completion/partial/stale status. A consumer can validate current text/retention before publication and again before activation; a still-resolvable anchor is insufficient. Matching executes no target and mutates neither source/cells nor parser state.
- [ ] **A3 — Cancellation and late-completion tests (R2/R4/R6/R9):** Delay work, replace or cancel the query, and deliver the older completion last. It cannot publish as latest or revive obsolete results. Cancellation checking, retained resources and query/result retention stay within documented bounds. Session close/replacement and buffer/reset generation changes make corresponding work ineligible under the contract; shell EOF alone is not required to invalidate.
- [ ] **A4 — Text/continuation tests (R1/R3/R9):** Cover multi-batch oversized logical lines, head-truncated retained suffixes, hard breaks and meaningful blanks, CJK/combining/emoji and byte/scalar/cell distinctions. A revision change between portions cannot silently join incompatible text. Respect approved resize clipping; do not search discarded text or invent missing prefixes.
- [ ] **A5 — Mutation/progress tests (R4/R8/R9):** Exercise append during search, overwrite with still-resolvable anchors, erase/structural edits, reflow/height changes, partial eviction/ring reuse, alternate transitions/saved-primary resize and reset/replacement. Stale text is not returned as actionable; remapped valid results or deterministic invalidation/fresh bounded work follow T0001's documented rules. Under repeated output, demonstrate useful current-query results on retained content rather than an endless full-history restart.
- [ ] **A6 — Bounded work and honest coverage (R6/R8/R9):** Tests expose read/copy and scan batch work, limit/incomplete-read outcomes, cancellation and retained query/results resources against implementation-documented limits. Partial coverage cannot masquerade as complete no-match. Searching is requested work separable from presentation, not a per-frame/hover operation. Focused headless regressions ship here; T0004 adds integrated measurements rather than establishing these fundamentals.

## Out of Scope

Application query placement/default shortcuts, concrete rendering, full #159 search/result-preservation scope, regex/advanced case options, global cross-session search and full #160 shell workflows. No new scheduling/cache/thread architecture or fixed performance threshold is prescribed.
