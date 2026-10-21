# Demand-driven content-anchor tracking during ingest

**Status:** Testing
**Results:** [Spec 0031 result summary](../spec/0031-bounded-history-and-performance-budgets.md#delivery-and-closure-summary) retains stage-1 ingestion measurements and gate outcomes; further T0004 native acceptance was waived by the owner. Raw evidence artifacts were deleted at the owner's request.
**Date:** 2026-10-20
**Sources:** [Spec 0031 result summary](../spec/0031-bounded-history-and-performance-budgets.md#delivery-and-closure-summary), user decision to fix before recalibrating the bounded-history budgets, and the user's choice of the incremental scope (gate idle work first, measure, then decide on incremental projection).
**Related:** [ADR 0042](./0042-content-anchors-and-buffer-specific-resize.md) (anchor semantics, unchanged), [ADR 0072](./0072-dual-bounded-terminal-history.md) and [ADR 0073](./0073-session-history-accounting-and-text-unit-overflow.md) (configurable history, whose calibration depends on this cost), [Spec 0031 R5](../spec/0031-bounded-history-and-performance-budgets.md).

## Context

Content anchors exist for exactly three consumers: pointer selections, a saved cursor position (`DECSC`), and the viewport review position. Resolving them requires a `ContentProjection` of the whole retained buffer.

Both ingest paths built that projection unconditionally. `Screen::write_char` captured an edit anchor on every width promotion, and `finish_anchor_mutations` built one at entry for every accepted PTY feed. Each build decodes every retained row, so terminal ingestion cost grew with retained history rather than with incoming bytes. Measured on a 1000-row history: 3.997 s for a 2000-row Unicode burst against 0.041 s for the same burst without the one scalar that promotes a printed narrow glyph to width 2, with 91.6–92.5 % of the time inside `ContentProjection::build`; a per-row feed cadence paid the same build once per feed.

When no anchor exists, the projection and the `AnchorMutationBatch` it resolves are unobservable: no selection is being reconciled, no saved cursor anchor is being adjusted, and no review anchor is being repaired. The work was therefore pure overhead in the default streaming case.

Two constraints shape the fix. Anchor semantics — affinity-adjusted offsets, eviction invalidation, review fallback to the oldest retained position, and whole-selection invalidation — are fixed by ADR 0042 and must not change. Selection state is owned by `PointerInteraction`, not `Screen`, and `Screen` must not begin querying the pointer.

## Decision

Make anchor bookkeeping demand-driven, in two stages.

**Stage 1 (implemented here).** Ingestion records mutations and builds projections only when some consumer needs them.

- A `Screen` reports whether anchor bookkeeping is required: an external interest flag, or an existing review anchor or saved cursor anchor.
- The I/O owner (`TerminalIo::feed_parser_bytes`) registers the pointer's selection state with the screen before parsing each feed. `Screen` never reaches into the pointer; the pointer-side state is pushed in, which is the same direction the existing selection reconciliation already travels.
- When bookkeeping is not required, `Screen` skips edit-anchor capture on width promotion and the feed path skips the reconciliation projection, dropping pending mutations without building one instead. When a consumer exists, behavior is bit-for-bit the previous path, including the two builds per promotion and the before/after structural diff per feed.
- The predicate grows automatically: saving a cursor or scrolling back into review re-enables tracking on the next check, so no separate enabling protocol is required.

**Stage 2 (not authorized by this record).** If measurement after stage 1 still shows ingest cost scaling with retained history while a selection or review anchor is active, replace whole-buffer projection with a maintained projection updated only for mutated logical lines, using the existing line-scoped mutation batch and `NormalBuf::retained_row`'s O(1) generation lookup. That work needs its own measured justification and must keep the ADR 0042 semantics intact.

## Consequences

- Streaming ingest without a selection, saved cursor or review state costs no projection builds; the measured worst case drops to the cost of the projection-free control.
- The interactive cases — selecting text while output continues, or reviewing scrollback while output continues — keep their current cost until stage 2; this record deliberately does not promise bounded ingest there.
- `Screen` gains one owner-supplied flag, and its editor must set it before parsing rather than relying on `Screen` discovering selection state. Tests that exercise anchors by direct mutation declare interest explicitly.
- Bounded-history calibration (T0001) must be re-measured on the changed build: the burst cost it was measured against is no longer representative, and its budget numbers would otherwise be obsolete on arrival.
- Anchor invalidation, affinity adjustment and review fallback are untouched, so reflow, copy, search and selection behavior are unchanged.
