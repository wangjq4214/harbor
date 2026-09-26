# Content-Preserving Resize Reflow

**Source:** [Spec 0014: Content-Preserving Resize Reflow](../../spec/0014-content-preserving-resize-reflow.md); GitHub [#152](https://github.com/wangjq4214/harbor/issues/152), [#169](https://github.com/wangjq4214/harbor/issues/169), and [#170](https://github.com/wangjq4214/harbor/issues/170)
**Ticket folder:** `.grimoire/ticket/0012-content-preserving-resize-reflow/`

## Overview

This ticket set replaces Harbor's physical-row non-reflow resize with bounded, content-preserving primary-screen reflow. It first establishes the #169 text and anchor contract in executable model behavior, then delivers #170 through primary reflow, height/capacity handling, buffer-specific transactional integration, and Windows/performance evidence. ADR-0018 remains current until the final implementation and evidence ticket is accepted.

## Delivery Surfaces

1. **Retained terminal model:** `NormalBuf` row metadata, ring movement, logical-line identity, meaningful extent, soft-wrap and truncated-head state.
2. **Screen editing and copy:** print/erase/edit/scroll/reset metadata maintenance, logical atom decoding, wide-cell and hyperlink preservation, logical copy extraction.
3. **Coordinates and interaction:** content-anchor resolution, cursor/saved-cursor insertion boundaries, review anchors, `SelectionModel`, `GenPos`, pointer hit testing, and snapshot projections.
4. **Resize policy:** primary width reflow, height/live-history boundary, capacity eviction, alternate-screen rectangular resize, saved-primary reflow, margins, tabs, damage, and minimum geometry.
5. **PTY integration:** prepared model resize, synchronous PTY resize, infallible model commit, failure rollback, and retry.
6. **Verification:** deterministic model/E2E tests, standard quality gates, Windows shell/`nvim` sessions, clipboard checks, and large-history cost records.

## Dependency Graph

| Ticket | Blocks | Concrete reason |
| --- | --- | --- |
| T0001 | T0002 | Logical decoding and copy require authoritative row identity and meaningful-content metadata. |
| T0002 | T0003, T0004 | Anchor offsets and width reflow require the shared logical atom and hard-break contract. |
| T0003 | T0004 | Width reflow cannot preserve selection/cursor semantics until canonical anchors and projection exist. |
| T0004 | T0005 | Height/capacity behavior must operate on the production reflowed row sequence and mappings. |
| T0005 | T0006 | Whole-Screen prepared resize must consume the completed primary width/height/capacity result. |
| T0006 | T0007 | Runtime and performance acceptance require the integrated PTY/model and alternate-screen path. |
| T0007 | — | Final evidence and status updates close the accepted scope. |

## Coordination Risks

| Tickets | Risk | Strategy |
| --- | --- | --- |
| T0001, T0002 | Both touch `normal_buf.rs`, cell writing, erase, and editing tests. | Finish row metadata invariants first; T0002 consumes them without redefining identity or extent semantics. |
| T0002, T0003 | Logical offsets must mean the same thing to atom decoding and anchors. | Export one crate-internal logical offset/affinity vocabulary and test round-trip conversion before parallel follow-up work. |
| T0003, T0004 | Both affect cursor, selection, snapshots, and physical projections. | Land canonical anchor storage first; width reflow only refreshes projections and must not add a second remap model. |
| T0004, T0005 | Both rebuild retained rows and can overlap `NormalBuf::resize`. | Keep one prepared primary-resize result whose width and height/capacity phases are composed, not duplicated. |
| T0005, T0006 | Screen and Terminal resize APIs change across both tickets. | Agree the prepared-result ownership boundary in T0005; T0006 wraps it without moving reflow policy into PTY code. |
| T0007, all implementation tickets | Evidence can be invalidated by later behavioral changes. | Add focused tests per ticket, but record final Windows/performance evidence only against the integrated revision. |

## Parallel Candidates

The implementation path is intentionally mostly sequential because each stage establishes a semantic contract consumed by the next. Within T0007, automated quality-gate recording, Windows scenario preparation, and performance-scenario preparation may proceed independently after T0006, but final records must identify the same accepted revision and dirty-tree scope.

## Recommended Order

1. T0001 — Retained Content Metadata
2. T0002 — Logical Atom and Copy Contract
3. T0003 — Canonical Content Anchors
4. T0004 — Primary Width Reflow
5. T0005 — Height, Capacity, and Review Reflow
6. T0006 — Buffer-Specific Transactional Resize
7. T0007 — Windows and Performance Acceptance

## Ticket Index

| Ticket | File | Outcome |
| --- | --- | --- |
| T0001 | [T0001-retained-content-metadata.md](./T0001-retained-content-metadata.md) | Normal-buffer mutations maintain logical-line identity, meaningful extent, wrap state, and truncation-safe row metadata. |
| T0002 | [T0002-logical-atom-and-copy-contract.md](./T0002-logical-atom-and-copy-contract.md) | Reflow and copy share one tested logical stream preserving hard breaks, blanks, attributes, hyperlinks, and wide glyphs. |
| T0003 | [T0003-canonical-content-anchors.md](./T0003-canonical-content-anchors.md) | Cursor, review, and selection can use durable anchors while retaining physical projections for rendering and input. |
| T0004 | [T0004-primary-width-reflow.md](./T0004-primary-width-reflow.md) | Primary history and live content re-wrap across width changes with cursor, pending-wrap, and selection meaning preserved. |
| T0005 | [T0005-height-capacity-and-review-reflow.md](./T0005-height-capacity-and-review-reflow.md) | Height changes, simultaneous resize, bounded eviction, truncated lines, and review fallback behave deterministically. |
| T0006 | [T0006-buffer-specific-transactional-resize.md](./T0006-buffer-specific-transactional-resize.md) | Alternate/saved-primary policies and prepared PTY/model commit are integrated without partial geometry changes. |
| T0007 | [T0007-windows-and-performance-acceptance.md](./T0007-windows-and-performance-acceptance.md) | Full gates, Windows shell/`nvim`/clipboard evidence, performance records, and accurate decision/status documentation close the package. |
