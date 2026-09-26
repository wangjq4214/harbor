# Primary Width Reflow

**Ticket ID:** T0004
**Source:** [Spec 0014](../../spec/0014-content-preserving-resize-reflow.md), GitHub #170
**Status:** Done

## Goal

Re-wrap retained primary-screen and scrollback logical lines at a new width while preserving logical content, attributes, hyperlinks, wide-cell invariants, cursor/pending-wrap meaning, and valid selections.

## Affected Surfaces

- **Normal-buffer resize:** Decode retained logical atoms, project them to the new width, and rebuild bounded physical rows and wrap metadata.
- **Coordinate projection:** Resolve live/saved cursor, selection, and review anchors against the new physical geometry.
- **Wide-cell and hyperlink state:** Regenerate edge padding, preserve complete glyphs and retained IDs, and maintain cell attributes.
- **Deterministic resize tests:** Width-only, repeated, and round-trip fixtures across live content and scrollback.

## Approach

Add a prepared primary width-reflow result rather than mutating the live ring while decoding. Pack complete width-one/two atoms without splitting wide glyphs, recreate soft-wrap flags and boundary padding, and preserve explicit hard breaks. Normalize requested width to at least two columns throughout model projection.

Project cursor insertion boundaries under the new width: a boundary at the right edge becomes last-column pending-wrap; a boundary in the row clears pending-wrap and uses the insertion column. Refresh selection and review physical projections from canonical anchors. Do not implement height/capacity overflow policy beyond what is required to construct a width result; T0005 owns final live/history placement and eviction.

## Dependencies and Coordination

- **Blocked by:** T0002 — supplies the only logical decoder/projectable atom contract; T0003 — supplies canonical positions and invalidation semantics.
- **Blocks:** T0005 — height, capacity, and review behavior operate on the reflowed row sequence.
- **Coordination risks:** `normal_buf.rs`, `screen.rs`, cursor code, `selection_model.rs`, and screen/terminal tests. Reuse T0002 atoms and T0003 anchors rather than adding resize-only representations.

## Acceptance

- [ ] Width-only resize re-wraps retained primary history and live content by logical line while preserving explicit newline and blank-line boundaries.
- [ ] Long colored lines, printed/styled trailing blanks, hyperlinks, and CJK glyphs retain logical and attribute identity across narrow/wide round trips.
- [ ] Every projected row satisfies the ADR-0016 wide-cell invariant and excludes generated padding from logical copy.
- [ ] Live and saved cursor remain at the same logical insertion boundary, with pending-wrap correctly derived for each width.
- [ ] Valid selection endpoints remain anchored and copy the same retained logical text after reflow; invalid endpoints clear the selection.
- [ ] Review projection continues to target the same retained content during width-only changes.
- [ ] Width requests of zero or one normalize consistently to two for terminal model projection.
- [ ] Deterministic repeated and round-trip width tests pass without claiming final PTY/alternate integration.

## Out of Scope

- Height-only policy, final capacity eviction, or head-truncated retained lines.
- Alternate-screen resize behavior and Terminal/PTY commit sequencing.
- N02 Unicode cluster and shaping work.
