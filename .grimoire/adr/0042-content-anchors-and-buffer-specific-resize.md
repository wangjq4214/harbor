# Content anchors and buffer-specific resize

**Status:** Proposed
**Date:** 2026-09-20

## Context

Issue #152 requires content-preserving main-screen reflow, but Harbor currently stores selection and review positions as physical-row generation coordinates and cannot distinguish every meaningful trailing blank from unwritten grid padding. The same resize behavior is also currently applied to primary and alternate screens even though full-screen applications treat the alternate screen as a two-dimensional drawing surface.

## Decision

Represent retained logical content explicitly enough to distinguish written ordinary spaces and styled or hyperlinked blank cells from unwritten cells and reflow-only wide-character padding. A printed ordinary space is meaningful. A default-style erase removes content and produces non-meaningful blank capacity; an erase carrying visible non-default styling produces a meaningful blank. Explicit blank lines remain logical boundaries even when they contain no meaningful cells.

Assign every logical line a monotonic, non-reused identity. Soft-wrap continuation rows share that identity and record their logical starting offset; explicit newlines create a new identity. Character-level edits retain the identity, structural row operations move row metadata with content, and clearing an independent row assigns a new identity. Reset and alternate-screen creation establish independent identity spaces.

Introduce durable content anchors identified by logical-line identity, logical offset, and before/after affinity; physical `(generation, column)` coordinates remain derived rendering and hit-testing coordinates rather than the long-term content identity. Cursor and saved-cursor positions use insertion-boundary anchors so pending-wrap can be re-derived from the new right margin. The viewport review position anchors its top-left content rather than preserving a numeric row offset.

Reflow retained primary-screen and scrollback logical lines on width changes. Do not text-reflow active or parked alternate-screen content: resize it as a rectangular application surface, repair boundary invariants, and clamp its cursor. While the alternate screen is active, independently reflow the saved primary screen to the new geometry.

Keep the existing physical-row capacity budget after reflow. Evict overflow from the oldest reflowed physical rows, allowing an oversized logical line to lose only its oldest prefix. Mark the first retained row of such a line as head-truncated. Invalidate anchors into evicted content, move an evicted review anchor to the oldest retained position, and require live-cursor content to remain valid before committing a resize.

For primary-screen height changes, preserve the live bottom and cursor: shrinking moves rows from the top of the live viewport into scrollback, while growing first pulls the newest history rows back into the viewport before adding blank space. A scrolled-back viewport remains tied to its review content anchor. For simultaneous changes, reflow at the new width before choosing the history/live boundary for the new height; alternate screens retain their rectangular top-left policy.

Decode retained physical rows into a temporary logical atom stream for reflow and copy. A glyph atom carries its character, one- or two-cell width, style, hyperlink, and meaningful-blank state; a hard-break atom represents an explicit line boundary. Wide base/continuation pairs form one atom, while generated edge padding is excluded and recreated during projection. Soft-wrap metadata joins rows and non-wrapped boundaries create hard breaks, including explicit blank lines.

Normalize terminal, model, and PTY width to a minimum of two columns so every retained width-two atom has a valid physical projection without replacement or hidden overflow storage. Rows retain a minimum of one. Resize continues to restore the full-height scroll region, clamp horizontal margins, extend tab stops at eight-column intervals, dirty the complete new viewport, prohibit alternate-screen scrollback, and clean unreachable hyperlink registry entries without changing retained hyperlink identities.

Build resize state transactionally before changing PTY geometry. A prepared resize owns all newly allocated buffers, metadata, cursor/review mappings, and buffer-specific resize results. If preparation fails, neither PTY nor model changes. After successful preparation, resize the PTY; on PTY failure discard the prepared state, and on success commit through an allocation-free, infallible ownership swap.

Store selection endpoints canonically as content anchors, with physical `GenPos` values treated as geometry-specific projections for rendering and hit testing. Pointer input maps pixels to `GenPos` and then to content anchors; resize preserves canonical anchors and refreshes projections. If either selection endpoint loses its logical content, invalidate the complete selection.

Content anchors denote logical positions rather than immutable text snapshots. Overwrite retains an anchor at the same logical offset; insertion or deletion before an anchor adjusts its offset according to affinity; row movement preserves identity; reflow changes only projection; clearing, replacing, or evicting the referenced logical line invalidates the anchor. Review anchors fall back to the oldest retained position, while a successful resize must preserve the live cursor anchor.

ADR-0018 remains the implemented behavior until this proposal is delivered and accepted; this record does not by itself supersede the current non-reflow implementation.

## Consequences

- Selections, review positions, cursors, and future search or command metadata retain content meaning across width changes and invalidate explicitly when their content is destroyed or evicted.
- Reflow and copy consume explicit content extent/provenance instead of inferring meaning with unconditional trailing-whitespace trimming.
- Normal-buffer row metadata must carry logical identity, logical offset, content extent, soft-wrap state, and truncated-head state without requiring a wholesale ring-buffer replacement.
- Cursor remapping derives physical cursor position and pending-wrap from an insertion boundary under the new width instead of clamping old coordinates.
- Alternate-screen applications remain responsible for redrawing their two-dimensional interface after PTY resize, while primary content remains recoverable on exit.
- Resize preparation may allocate and fail, but commit after successful PTY resize is an infallible ownership swap, preserving PTY/model geometry consistency.
- Selection APIs require a canonical anchor representation plus current physical projections; `GenPos` remains useful but is no longer the durable selection identity.
