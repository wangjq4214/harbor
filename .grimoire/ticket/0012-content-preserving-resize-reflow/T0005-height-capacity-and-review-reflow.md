# Height, Capacity, and Review Reflow

**Ticket ID:** T0005
**Source:** [Spec 0014](../../spec/0014-content-preserving-resize-reflow.md), GitHub #170
**Status:** Todo

## Goal

Complete primary-screen geometry behavior for height-only and simultaneous resize, bounded post-reflow eviction, partial logical-line retention, and content-anchored review fallback.

## Affected Surfaces

- **Primary live/history boundary:** Bottom/cursor-anchored shrink and growth behavior.
- **Capacity policy:** Physical-row budget after reflow, oldest-row eviction, truncated-head metadata, and anchor invalidation.
- **Review state:** Top-left content preservation and fallback to oldest retained content.
- **Prepared primary result:** Composition of width projection, new height, capacity, cursor validity, and full damage state.

## Approach

For height shrink, keep the live bottom and cursor and move removed top viewport rows into scrollback. For growth, pull newest history rows back before adding blank capacity. For simultaneous changes, consume T0004's new-width row sequence before choosing the new live/history boundary.

Apply the existing physical-row capacity to the completed sequence. Evict oldest rows exactly; if only an old prefix of one logical line is removed, mark the retained head as truncated without changing its logical identity. Invalidate anchors into removed content, clear a selection if either endpoint is lost, and move an evicted review anchor to the oldest retained position. Reject preparation if live cursor content cannot remain valid.

## Dependencies and Coordination

- **Blocked by:** T0004 — simultaneous resize and capacity operate on production reflowed rows and anchor projections.
- **Blocks:** T0006 — whole-Screen and PTY transaction integration needs the final prepared primary result.
- **Coordination risks:** `NormalBuf` retained sequence, `view_offset` compatibility, cursor projection, damage, and tests overlap T0004. Extend the same prepared result instead of adding a second rebuild path.

## Acceptance

- [ ] Height shrink moves top live rows into history while preserving the live bottom and cursor.
- [ ] Height growth pulls newest history back into the viewport before introducing blank rows.
- [ ] A scrolled-back viewport remains tied to its review content across height-only and simultaneous changes.
- [ ] Simultaneous resize reflows at new width before selecting the new live/history boundary.
- [ ] Post-reflow overflow evicts oldest physical rows within the existing bounded budget.
- [ ] Partial eviction of an oversized logical line sets truncated-head state; retained suffix anchors resolve and evicted-prefix anchors do not.
- [ ] Evicted selection endpoints clear the complete selection, and an evicted review anchor falls back to the oldest retained position.
- [ ] A prepared result with an invalid live cursor is rejected without mutating live state.
- [ ] Width-only, height-only, combined, repeated, ring-wraparound, and near-capacity tests pass with full viewport damage.

## Out of Scope

- Configurable or unlimited history capacity.
- Alternate-screen geometry and PTY failure integration.
- Performance optimization without measurement.
