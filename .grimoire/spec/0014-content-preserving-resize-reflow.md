# Content-Preserving Resize Reflow

**Spec ID:** 0014
**Status:** Accepted (owner-confirmed)
**Date:** 2026-09-20

**Stable execution aliases:** 0012/T0001–T0006 = retained metadata, shared decoder/copy, canonical anchors, width projection, height/capacity and buffer/PTY transaction below; 0012/T0007 = Test Plan and final acceptance. Historical acceptance notes lacked broader Windows shell/nvim/clipboard and large-history latency/memory evidence; owner acceptance does not assert missing checks ran. Preserve one decoder/offset vocabulary and one composed prepared primary result.

## Requirements

Harbor must preserve retained primary-screen and scrollback logical content across width-only, height-only, simultaneous, and repeated terminal resizes, with two explicit exceptions: primary reflow may discard ordinary default-style, unprotected, non-hyperlinked spaces at a logical line's end; and Windows ConPTY live-primary resize follows producer geometry with style-only tail clipping under [ADR 0052](../adr/0052-conpty-live-primary-producer-geometry-and-styled-tail-clipping.md). Apart from these scoped exceptions and documented capacity eviction, round-trip resize preserves explicit line boundaries, meaningful styled or hyperlinked blanks, cell attributes, hyperlinks, wide characters, cursor and saved-cursor meaning, pending-wrap behavior, viewport review position, selection meaning, and copied text.

The first delivery must:

1. Re-wrap retained primary content by logical line instead of copying a top-left rectangle or trimming every physical row, subject to the Windows ConPTY live-primary geometry exception below.
2. Distinguish printed ordinary spaces, styled, protected or hyperlinked blank cells, unwritten capacity, and generated wide-edge padding; on primary resize reflow, ordinary default-style, unprotected, non-hyperlinked spaces at the end of a logical line may be discarded to avoid a visually empty continuation row, except where needed by a live or saved cursor.
3. Use durable logical content anchors for cursor, saved cursor, review position, selection endpoints, and future metadata consumers;
4. Preserve the current ring-buffer architecture while making logical identity and reflow metadata explicit;
5. Resize alternate screens as rectangular application surfaces while independently reflowing a saved primary screen;
6. Apply physical-row history capacity after reflow with explicit partial-line truncation and anchor invalidation;
7. Keep PTY and model geometry consistent when preparation or PTY resize fails; and
8. Provide deterministic model/copy regression coverage plus recorded Windows shell, `nvim`, and large-history resize evidence; and
9. Establish an acknowledged, bounded PTY-reader barrier so chunks observed before resize are parsed exactly once under the old geometry and the reader cannot publish across the resize transaction.

[ADR 0018](../adr/0018-non-reflow-resize-preserved-soft-wrap-markers.md) records the historical non-reflow baseline. ADR-0018/0042 status reconciliation remains unrecorded: ADR 0018 is Completed and ADR 0042 is Proposed; owner acceptance here does not edit those records.

## Solution

### Retained content and row metadata

Extend normal-buffer row metadata beyond the existing soft-wrap flag. Every retained row records:

- a monotonic, non-reused logical-line identity;
- the row's starting offset within that logical line;
- its meaningful content extent;
- whether it is a soft-wrap continuation; and
- whether the retained logical line has lost an older prefix to capacity eviction.

Rows joined by soft-wrap metadata share one logical-line identity. An explicit newline starts a new identity. Character overwrite retains identity; insertion or deletion adjusts affected anchor offsets according to affinity; structural row operations move metadata with content; clearing an independent row assigns a new identity. Reset and alternate-screen creation establish independent identity spaces. Ring-slot reuse must never cause an old content anchor to resolve to unrelated content.

A printed ordinary space is meaningful. A blank carrying visible non-default styling or a hyperlink is meaningful. A default-style erase removes content and leaves non-meaningful blank capacity; a visibly styled erase creates meaningful blank content. Explicit blank lines remain hard line boundaries even when they contain no meaningful cells.

Under full primary reflow (retained history and non-ConPTY primary), ordinary default-style, unprotected, non-hyperlinked spaces at the end of a logical line may be discarded. Discarded spaces are not retained for future widening, copy, or selection; a selection endpoint on discarded content may be clamped or invalidated. Preserve spaces required to maintain the live or saved cursor insertion position, significant interior spaces, styled/protected/hyperlinked blanks, and real hard blank lines. Windows ConPTY live-primary styled tails instead follow the scoped producer-compatible rule below.

### Approved Windows ConPTY live-primary exception

The approved scoped exception and its implementation/evidence are recorded in [ADR 0052](../adr/0052-conpty-live-primary-producer-geometry-and-styled-tail-clipping.md); approval alone is not acceptance.

Only the producer-owned live portion of a Windows ConPTY primary screen follows producer text/cursor geometry: a style-only trailing blank suffix, whether printed or created by styled erase/fill, must not itself create extra physical rows. Preserve trailing cell attributes within available target-row capacity and clip/discard overflowing styled tail capacity. Discarded overflow is not promised to return on widening or remain available to copy/selection; anchors must reflect surviving cells, with lost endpoints clamped or invalidated consistently.

For already soft-wrapped source rows, retain the producer's forced-prefix footprint, including ordinary or styled blanks; only the final old source row has a trimmable tail. An old forced 103-column prefix occupies three rows on the first shrink to 51, regardless of whether its last cells are spaces. Removing that footprint displaces producer-relative repaint. After widening collapses the prefix to one row, later narrowing may trim the now-final suffix.

Actual text, significant interior spaces, explicit hard blank lines, wide-cell invariants, and producer-compatible live/saved cursor and pending-wrap semantics remain required. Conceal is not a reason to discard hidden source text. Retained history and non-ConPTY primary keep the full rules above; this is not global blank reclassification or permission to ignore protected/hyperlinked content. History/live-spanning lines must preserve the policy boundary and existing `PreserveLiveTop` protection without pulling history into producer live coordinates. The same region-specific primary rule applies to saved-primary resize while alternate is active; alternate surfaces remain rectangular. Transactional resize and normal output interpretation are unchanged.

The logical projection, capacity, copy, and preservation requirements below are subject only to this scoped exception; their full-retention expectations remain mandatory for history/non-ConPTY primary.

### Logical stream and physical projection

Keep one crate-internal logical decoder shared by copy and reflow, independent of selection ownership, and one shared logical offset/affinity and content-anchor vocabulary. These internal contracts do not stabilize a public anchor API outside `harbor-terminal`.

Decode retained primary rows into a temporary logical atom stream shared by reflow and copy:

- a glyph atom carries its character, width of one or two cells, attributes, colors, hyperlink identity, and meaningful-blank state;
- a hard-break atom represents an explicit line boundary;
- a valid wide base/continuation pair becomes one width-two atom;
- continuation cells are not independent content; and
- generated padding used to avoid splitting a wide atom at the right edge is excluded from the logical stream and recreated during projection.

Malformed or orphan wide cells must be rejected or normalized through existing wide-cell invariants, not silently decoded as independent content.

A continuation-row soft-wrap marker joins adjacent physical rows. A non-wrapped row boundary creates a hard break, including each explicit blank line. Before packing a primary logical line into the new width, remove its maximal ordinary trailing-space suffix except any portion needed by a live or saved cursor. Reprojection packs the remaining complete atoms, regenerates continuation metadata and wide-edge padding, and never splits a width-two atom.

The new logical end after resize is the end of the retained prefix, not a hidden off-grid payload. A later resize decodes that prefix and cannot recover discarded spaces. Preserve the live and saved cursor's insertion boundaries by retaining enough spaces on their lines before trimming, even if this cursor-required content occupies another row.

When a ConPTY `PreserveLiveTop` resize trims all ordinary atoms from the first live row of a history-spanning logical line, keep an empty boundary row rather than projecting history into the producer's live viewport. This row protects live geometry, not discarded text; it does not restore the removed spaces for copy.

Terminal, model, and PTY geometry normalize requested width to at least two columns and row count to at least one. This guarantees that every retained width-two atom has a valid physical projection without replacement characters or hidden overflow storage.

The logical offset used by the first delivery is an offset in this atom stream. Combining sequences, variation selectors, ZWJ sequences, and broader grapheme/shaping behavior remain N02 work; the anchor contract must permit atom representation to evolve without returning to physical-row identity.

### Content anchors and projections

A content anchor contains:

- logical-line identity;
- logical offset; and
- `Before` or `After` affinity.

Content anchors identify logical positions rather than immutable text snapshots. Overwrite preserves the offset, insertion or deletion before the position adjusts it according to affinity, row movement preserves identity, and reflow changes only the physical projection. Clearing, replacing, or evicting the referenced logical content invalidates the anchor.

Physical `(generation, column)` coordinates remain current-geometry projections used for rendering and pointer hit testing. Selection endpoints store content anchors canonically and cache or derive `GenPos` projections. Pointer input maps pixel position to `GenPos` and then to a content anchor. Reflow preserves canonical endpoints and refreshes projections; loss of either endpoint invalidates the complete selection.

Cursor and saved cursor use insertion-boundary anchors. Projection derives cursor cell and pending-wrap from the new right margin: a boundary at the right edge projects to the last column with pending-wrap set; a boundary inside the row projects to that insertion column with pending-wrap clear. A successful resize must preserve the live cursor anchor.

The review position anchors the content at the viewport top-left. Reflow keeps that content at the top where retained; if it is evicted, review moves to the oldest retained position rather than clamping the previous numeric `view_offset`.

### Primary-screen resize and capacity

On a primary-screen width change, rebuild retained history and live content from logical atoms at the new width. On height shrink, keep the live bottom and cursor while moving rows removed from the top of the live viewport into scrollback. On height growth, first pull the newest history rows back into the live viewport, then add blank capacity. A scrolled-back viewport remains tied to its review anchor.

For simultaneous width and height changes, first reflow at the new width and then choose the history/live viewport boundary for the new height.

Apply the existing physical-row capacity budget to the reflowed result. Evict overflow from the oldest physical rows. If this removes only the prefix of an oversized logical line, retain the suffix with `head_truncated` metadata. Anchors into evicted content become invalid; an evicted review anchor moves to the oldest retained position. A prepared resize that cannot preserve the live cursor is rejected rather than committed.

### Alternate-screen policy

Active and parked alternate screens do not text-reflow and do not gain scrollback. Resize them as rectangular application surfaces, preserving the top-left rectangle, repairing wide-cell boundary invariants, clamping cursor state, and relying on the full-screen application to redraw after PTY resize.

When an alternate screen is active, independently reflow the saved primary screen to the new geometry. Exiting alternate mode therefore restores the resized primary content, review state, cursor state, and scrollback rather than stale pre-resize geometry. Whole-screen swap isolation from ADR-0019 remains unchanged.

### Transactional resize

Acquire an acknowledged PTY-reader barrier before preparing model state. A barrier request carries a unique epoch. The reader must publish every byte from any read completed before acknowledging that epoch, publish the matching acknowledgement in order, and start no further read until the terminal resumes it. A barrier racing with read completion must neither lose nor duplicate the returned bytes. Platform PTY code must make a blocked read interruptible without exposing raw platform handles beyond `harbor-pty`.

Barrier acquisition waits for a bounded interval. On acknowledgement, the terminal drains and parses every preceding chunk under the old geometry. It then keeps the reader paused while preparing all model work, resizing the PTY, and committing the model. Bytes read after resume belong to the post-resize side of the reader-observed boundary and are parsed under the new geometry.

The prepared resize owns all newly allocated normal/alternate/saved-primary buffers, row metadata, damage state, tab stops, cursor mappings, review mappings, selection projections, capacity results, and hyperlink reachability results.

The terminal resize sequence is:
1. normalize requested geometry;
2. request a unique reader-barrier epoch and wait for its acknowledgement up to the configured protocol timeout;
3. on barrier failure or timeout, preserve old PTY/model geometry, resume the reader, and allow retry;
4. parse all reader events preceding the matching acknowledgement under the old geometry;
5. prepare the complete model resize without mutating live state while the reader remains paused;
6. on preparation failure, discard prepared state, resume the reader, and keep PTY/model geometry unchanged;
7. resize the PTY;
8. on PTY failure, discard prepared state, resume the reader, and keep the model unchanged;
9. on PTY success, install prepared state through an allocation-free, infallible ownership swap; and
10. resume the reader on every success or error path.

The barrier orders chunks observed by the PTY reader. It does not claim to identify when the child generated bytes still unread in an operating-system pipe.

The committed resize restores a full-height scroll region, clamps horizontal margins, extends tab stops at eight-column intervals, marks the complete new viewport dirty, and cleans unreachable hyperlink registry entries without changing retained hyperlink identities.

### Copy behavior

Copy traverses retained logical content. It joins soft-wrapped rows, emits hard breaks for explicit newlines and blank lines, skips continuation cells and generated padding, and preserves any meaningful ordinary and styled trailing spaces still retained. Primary resize may discard the ordinary tail, or Windows ConPTY live-primary styled overflow, under the respective policies above; copied text after resize need not include discarded spaces. Copy resolves retained hyperlink-bearing cells without treating URI metadata as copied text. Unconditional per-physical-row `trim_end()` is not part of the algorithm.

Trimming is restricted to primary resize, not the copy operation itself. Before resize, printed ordinary spaces are copied as written; after resize, only the retained prefix is copied.

### E2E: ConPTY live styled-tail geometry and cmd repaint

- **Given:** Bundled ConPTY `1.24.260710001`, the original conceal/overline script and pending cmd input, plus minimized ordinary tails, 65-blank overline/underline/background tails and 103-blank styled erase rows.
- **When:** Narrow 103 -> 51 columns, widen again, and repeat mixed resizes, then type, backspace and submit input.
- **Then:** Live model geometry agrees with the producer before CUP + ED repaint; style-only tails do not add rows, retained target-row styling survives, and clipped overflow is not claimed for restoration/copy. The `END` marker, complete prompt and pending input survive repaint and subsequent editing/submission. Also verify live/saved cursor, wide cells and history/live boundaries; history/non-ConPTY retain complete styled content and alternate remains rectangular. Existing full-retention E2E cases apply to those unaffected policies, not clipped live overflow.

## Test Plan

- **Reference-model and fixture tests:** Define logical atom decode/project fixtures. Cover long lines, explicit and soft line boundaries, consecutive blank lines, ordinary and styled trailing spaces, hyperlinks, pending wrap, CJK at boundaries, partially selected wide glyphs, head-truncated lines, and repeated round trips.
- **Normal-buffer tests:** Verify identity allocation/non-reuse, ring wraparound, metadata movement for scroll and structural edits, width/height policies, physical capacity, partial-line eviction, full damage, and no stale generation-to-content aliasing.
- **Anchor tests:** Verify before/after affinity under insert/delete, overwrite stability, row movement, reflow projection, destruction/eviction invalidation, review fallback, cursor insertion boundaries, saved cursor, and complete-selection invalidation when one endpoint is lost.
- **Selection/copy tests:** Verify pixel/`GenPos`/anchor conversion, forward and reverse drag, word and logical-line selection, selection retention across output and resize, meaningful trailing blanks, explicit newlines, soft-wrap joining, wide continuation skipping, and eviction behavior.
- **Tail-space reflow tests:** Model a PowerShell-style full-width padded table, narrow and widen it repeatedly, and compare visible rows, scrollback, before/after copy, anchors, and next-output cursor position. Assert discarded ordinary tails do not return on widening; include saved/live cursor in a tail, styled/protected/hyperlinked blanks, interior spaces, explicit blank lines and selected tail endpoints (clamped or invalidated without corrupting surviving content).
- **ConPTY live compatibility tests:** Exercise the producer/model and original-script cases above, styled erase/fill, cursor-required extents and history/live-spanning lines. Check retained target-row attributes and honest overflow copy/anchor loss separately from unchanged full history/non-ConPTY styled-tail retention. No test execution or acceptance is asserted by this contract update.
- **Alternate-screen tests:** Cover `?47`, `?1047`, `?1048`, and `?1049` resize/restore behavior, active and parked alternate buffers, saved-primary reflow, no alternate scrollback, RIS, and current whole-screen isolation.
- **Barrier protocol tests:** Deterministically interleave barrier requests with a read completed before publication and with a blocked read. Prove ordered matching acknowledgements, exactly-once chunks, no publication while paused, no stale-epoch reuse, and successful resume/retry.
- **Failure tests:** Inject barrier interruption failure, barrier timeout, model preparation failure, and PTY resize failure; prove bounded return, no unacknowledged resize, no partial model commit, no premature pointer clearing, unchanged authoritative geometry on failure, mandatory reader resume, no reader leak, and successful retry.
- **Quality gates:** Targeted `harbor-terminal` and `harbor-pty` tests, format, clippy across all targets/features with warnings denied, workspace tests, and documentation checks follow validation policy.
- **Windows runtime evidence:** Record revision, dirty-tree scope, Windows/ConPTY/application versions, exact steps, expected and observed results, PASS/FAIL/NOT RUN/BLOCKED outcome, artifacts, and exclusions for PowerShell or `cmd`, `nvim`, clipboard selection/copy, primary/alternate transitions, repeated mixed resize, and interruption of a blocked ConPTY read during barrier acquisition. Explicitly reproduce PowerShell's padded directory table before and after resizing, checking that resizing does not introduce empty rows between its entries.
- **Performance evidence:** Record large-history resize latency and memory/cost under fixed machine, font, viewport, history, and build-profile conditions. This specification sets no unsupported pass threshold; preserve comparable captures so a threshold or optimization can be justified from evidence.

## Out of Scope

- Replacing the ring buffer with a rope, document model, or unlimited history.
- Globally trimming copy or selection outside resize, or treating styled/protected/hyperlinked blanks, interior spaces and zero-width characters as ignorable beyond the explicitly approved Windows ConPTY live-primary style-only tail exception.
- Introducing off-grid tail storage and a virtual cursor to retain ordinary tail content after resize; superseded by the explicitly accepted information-loss policy.
- Changing ordinary PTY output interpretation when no resize occurs; the reported regression occurs after resize.
- Search UI, command navigation UI, pane UI, font reload, or font shaping.
- Completing combining-mark, variation-selector, ZWJ emoji, ambiguous-width, or complex-script behavior assigned to N02.
- Reflowing alternate-screen application drawings as logical text.
- Changing OSC 8 activation policy, URI validation, or hyperlink ownership beyond preserving and cleaning existing IDs.
- Claiming Windows or application compatibility from model tests without the required runtime evidence.
- Suspending the shell process tree or claiming a generation timestamp for bytes still unread in an operating-system pipe; the accepted barrier orders reader-observed chunks only.

## Future Evolution

- N02 may evolve a glyph atom into a richer text cluster while preserving logical-line identity and anchor semantics.
- N08 search and N09 shell-command navigation can store content anchors and use the same eviction invalidation contract.
- N15 may make history capacity configurable and establish measured resize budgets without changing bounded eviction semantics.
