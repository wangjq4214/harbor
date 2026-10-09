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

## Dependencies and surviving acceptance

[T0007 — Windows and Performance Acceptance](./T0007-windows-and-performance-acceptance.md) remains active. It requires the integrated [transactional resize](../../spec/0014-content-preserving-resize-reflow.md#transactional-resize), [primary height/capacity](../../spec/0014-content-preserving-resize-reflow.md#primary-screen-resize-and-capacity), [alternate-screen](../../spec/0014-content-preserving-resize-reflow.md#alternate-screen-policy), and [anchor/selection](../../spec/0014-content-preserving-resize-reflow.md#content-anchors-and-projections) contracts, not merely a partial implementation build. Identify the tested revision and dirty-tree scope; later behavior changes require affected evidence to be rerun.

Implementation sequencing remains metadata → shared logical decoder/copy → canonical anchors → width projection → height/capacity → whole-Screen/PTY integration. Keep one crate-internal logical offset/affinity vocabulary and one composed prepared primary result; do not add competing decoder, remap, or rebuild paths. Automated gates, Windows scenarios and performance preparation may proceed independently, but final acceptance needs integrated evidence.

## Historical implementation IDs and destinations

Historical T0001–T0006 contracts survive in Spec 0014; retirement does not establish acceptance. #169 scoped the text/anchor foundation; #170 scoped resize integration; #152 retains the overall acceptance scope. The width ticket's original blanket trailing-space preservation was explicitly superseded by [ADR-0046](../../adr/0046-trim-ordinary-trailing-spaces-during-primary-reflow.md); Spec 0014 preserves the ordinary-tail exception and cursor-required retention.

| Historical ID | Surviving contract destination | Scope |
| --- | --- | --- |
| T0001 (#169) | [Retained content and row metadata](../../spec/0014-content-preserving-resize-reflow.md#retained-content-and-row-metadata) | Logical identity, extent, mutation and blank provenance in the bounded ring. |
| T0002 (#169) | [Logical stream and physical projection](../../spec/0014-content-preserving-resize-reflow.md#logical-stream-and-physical-projection), [Copy behavior](../../spec/0014-content-preserving-resize-reflow.md#copy-behavior) | Shared decoder, hard breaks, attributes, hyperlinks and wide units. |
| T0003 (#169) | [Content anchors and projections](../../spec/0014-content-preserving-resize-reflow.md#content-anchors-and-projections) | Canonical logical positions, affinity, projection and invalidation. |
| T0004 (#170) | [Logical stream and physical projection](../../spec/0014-content-preserving-resize-reflow.md#logical-stream-and-physical-projection) | Width packing, normalized geometry and cursor/pending-wrap projection. |
| T0005 (#170) | [Primary-screen resize and capacity](../../spec/0014-content-preserving-resize-reflow.md#primary-screen-resize-and-capacity) | Height/live-history placement, bounded eviction and review fallback. |
| T0006 (#170) | [Alternate-screen policy](../../spec/0014-content-preserving-resize-reflow.md#alternate-screen-policy), [Transactional resize](../../spec/0014-content-preserving-resize-reflow.md#transactional-resize) | Buffer-specific preparation, PTY/model commit, failure/retry and reader barrier. |

## Evidence and open gates

[Automation](../../../docs/verification/content-preserving-resize-automation.md) records automated scope only. [Windows runtime](../../../docs/verification/content-preserving-resize-windows.md) and [performance](../../../docs/verification/content-preserving-resize-performance.md) records are NOT RUN and block final T0007 acceptance. Spec 0014's [Test Plan](../../spec/0014-content-preserving-resize-reflow.md#test-plan) remains the verification destination; retiring historical implementation tickets does not close #152/#170 or advance ADR acceptance.
