# Bounded Versioned Read Model and Neutral Contracts

**Ticket ID:** T0001
**Source:** [Spec 0027](../../spec/0027-revisioned-terminal-read-model-and-literal-search.md), R1–R5, R8/R9 and bounded-read/decoration/shell seams; [set constraints and source baseline](./README.md#source-baseline-and-constraints).
**Status:** Done

## Goal

Expose a bounded read-only retained-content boundary, with coherent identity/revision and neutral logical range/action-target data, independently testable without the matcher, application UI or GPU. Support search and future metadata consumers without selecting those future features.

## Affected Surfaces

Existing `harbor-terminal` logical decoder, content anchors, ring/row metadata, `Screen` and public engine boundary: `crates/harbor-terminal/src/{logical_content,content_anchor,normal_buf,screen,model,lib,update}.rs`. Coordinate with `TerminalSnapshot`, `TerminalUpdate`, `read_update` and `acknowledge_update` without treating renderer projection epochs as content revisions.

Alignment: [ADR 0042](../../adr/0042-content-anchors-and-buffer-specific-resize.md), [0044](../../adr/0044-unicode-text-unit-and-presentation-width-policy.md), [0045](../../adr/0045-gpu-independent-terminal-core-boundary.md), [0035](../../adr/0035-cell-linked-bounded-registry-for-osc8-hyperlinks.md), [0052](../../adr/0052-conpty-live-primary-producer-geometry-and-styled-tail-clipping.md), [spec 0014](../../spec/0014-content-preserving-resize-reflow.md), and [OSC 133 metadata ADR](../../adr/0037-osc133-shell-integration-metadata.md).

## Approach

Build on retained logical text/anchor semantics and existing ring storage. Document bounded request/response, continuation and validity contracts and supply neutral decoration/action data with provenance. Keep core independent of application business types and GPU handles; session qualification must support the later host binding without using current-tab position as identity.

API spelling, budgets and internal revision/scan/cache mechanisms remain implementation details subject to the spec. Reflow may remap or invalidate, but this ticket's contract must allow T0002/T0003 to make useful search progress under output. No multi-renderer subscription architecture is implied by testing independent readers.

## Dependencies and Coordination

- **Blocked by:** None.
- **Blocks:** [T0002](./T0002-bounded-literal-search-consumer.md): needs coherent bounded text, continuations, ranges and validity rules.
- **Coordination:** T0003 binds actual session identity and projects ranges. Keep renderer acknowledgement independent; carry tests/contracts forward rather than expose mutable `Screen` or invent a second physical-row identity scheme.

## Acceptance

- [x] **A1 — Bounded read contract (R1/R8):** Visible rows and explicitly bounded retained history are available without mutable `Screen`. Document lifetime, consistency point, scope, actual copied size, continuation/truncation, invalidation and concrete limits/limit behavior. Demonstrate that obtaining a subset does not first clone/decode all history.
- [x] **A2 — Continuity and retention (R1/R3):** A logical line larger than a batch and one with an evicted prefix retain offsets and real start/end versus continuation information. Pages do not invent text/newlines or silently combine incompatible revisions. Only retained content is represented; preserve ordinary-tail and ConPTY styled-overflow clipping exceptions.
- [x] **A3 — Focused bounded/headless tests (R1/R3/R9):** Exercise visible/history subsets, long soft-wrapped lines over several batches, explicit blank lines/hard breaks, prefix eviction and incompatible-revision continuation. Two independent headless readers can consume the boundary without acknowledging renderer damage or consuming parser events; pending updates remain renderable after reads and skipped/failed drawing.
- [x] **A4 — Identity/content validity (R2/R4):** Reads and neutral data can identify terminal session, buffer/content generation and content revision, distinguishing primary/alternate, replacement/reset and repeated line IDs/offsets. Document revision advancement/invalidation and validation independently from drawing, acknowledgement and viewport projection. Resolvable anchors alone are not text-validity proof.
- [x] **A5 — Event-contract regressions (R2/R4/R9):** Test append, overwrite at a still-resolvable anchor, erase, structural edits, width/height/reflow, whole/partial eviction and ring reuse, primary/alternate transitions with saved-primary resize, reset and buffer replacement. Changed/lost content fails validity; surviving content and buffer identity remain distinguishable. Never treat old physical coordinates as current. Document the delivered remap or deterministic-invalidation behavior without changing existing buffer resize policy.
- [x] **A6 — Text/metadata and neutral data (R3/R5/R9):** Preserve complete shared Unicode text units and map logical anchors without equating UTF-8 bytes, scalars and cells. Test CJK/combining/emoji, meaningful blanks versus capacity/generated wide-edge padding and synthetic cues, styles and bounded safe hyperlink metadata. Neutral decoration ranges carry identity/version; target data supports search navigation and future URL/file consumers without executing targets, source mutation or GPU coupling. Metadata remains safe after registry cleanup or snapshot expiration.
- [x] **A7 — Boundary compatibility (R5/R9):** Verify core-only build/tests remain GPU-independent. Logical identity/offset/affinity and invalidation remain usable for future OSC 133 command positions without new parsing or command collection. Contract tests and API documentation accompany this ticket, not T0004.

## Out of Scope

Matcher/query UI, real application acceptance/activation and concrete highlight projection belong to T0002/T0003. No storage replacement, new OSC parsing, URL/file detection/opening, command workflow, numeric budget selection by this ticket text, mandatory reflow-remap strategy, new crate split or universal renderer interface.

## Delivery evidence

Implemented the public `harbor_terminal::read_model` boundary with hard page budgets, strict continuations and explicit rebase, shared logical text-unit coordinates, independent content revisions and bounded row-proof validation. Resize/reflow deterministically invalidates old generations; reads do not consume renderer updates or parser events.

[Contract, A1–A7 audit and verification results](../../../docs/terminal-read-model.md): 19 focused headless tests plus an API doctest, core-only and workspace suites, dependency isolation, clippy and formatting pass. The full documentation check now passes after the user-authorized repair of four retired-ticket links in local plans. T0002–T0004 application/search/runtime work is not claimed.
