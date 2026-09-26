# Canonical Content Anchors

**Ticket ID:** T0003
**Source:** [Spec 0014](../../spec/0014-content-preserving-resize-reflow.md), GitHub #169
**Status:** Todo

## Goal

Represent cursor, saved cursor, review, and selection meaning with durable logical positions while retaining generation/column projections for current rendering and pointer interaction.

## Affected Surfaces

- **Anchor model:** Logical-line identity, atom offset, before/after affinity, resolution, adjustment, and invalidation.
- **Selection model:** Canonical anchored endpoints with current `GenPos` projections and whole-selection invalidation when either endpoint is lost.
- **Pointer/snapshot boundary:** Pixel-to-`GenPos` hit testing followed by physical-to-logical conversion; row projection metadata in `TerminalSnapshot` or an equivalent bounded boundary.
- **Cursor/review model:** Insertion-boundary representation for live/saved cursor and top-left content anchoring for scrollback review.

## Approach

Introduce one crate-internal `ContentAnchor` vocabulary based on T0002 atom offsets. Keep `GenPos` as an ephemeral current-layout projection. Convert pointer-selected physical positions to anchors at interaction time; derive render/copy bounds from current projections without maintaining a second durable coordinate model.

Implement accepted mutation semantics: overwrite keeps offset, insertion/deletion before an anchor adjusts by affinity, structural row movement preserves identity, and logical-line destruction or eviction invalidates the anchor. Cursor anchors represent insertion boundaries; review anchors identify viewport top-left content. This ticket proves conversion and mutation behavior on unchanged geometry; production resize projection changes arrive in T0004/T0005.

## Dependencies and Coordination

- **Blocked by:** T0002 — canonical offsets and hard boundaries must match the shared logical atom stream.
- **Blocks:** T0004 — width reflow needs canonical cursor, saved-cursor, review, and selection positions to preserve meaning.
- **Coordination risks:** `selection_model.rs`, `pointer.rs`, `model.rs`, `screen/reader.rs`, and cursor code. Preserve existing public selection/render behavior while changing internal ownership.

## Acceptance

- [ ] Content anchors round-trip to/from retained physical positions on current geometry, including wide glyph boundaries and explicit blank lines.
- [ ] Ring movement preserves anchor identity, and stale identities never resolve through reused generations or ring slots.
- [ ] Overwrite, insert, delete, row movement, independent-row clear, reset, and eviction follow ADR-0042 adjustment/invalidation semantics.
- [ ] Selection stores canonical anchored endpoints; loss of either endpoint clears the complete selection.
- [ ] Pointer hit testing still uses current geometry and produces the same character/word/logical-line selections before conversion to anchors.
- [ ] Cursor and saved cursor can be represented as insertion boundaries without changing current no-resize behavior.
- [ ] Review position can identify top-left retained content independently of numeric `view_offset`.
- [ ] Boundary-contract and focused selection/cursor tests pass without enabling production reflow.

## Out of Scope

- Atom reprojection to a different width.
- Search or OSC 133 command anchor consumers.
- Public stabilization of anchor types outside `harbor-terminal`.
