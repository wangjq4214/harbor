# Content-Preserving Resize Reflow

**Spec ID:** 0014
**Status:** Draft
**Date:** 2026-09-20

## Requirements

Harbor must preserve retained primary-screen and scrollback logical content across width-only, height-only, simultaneous, and repeated terminal resizes. Except for documented capacity eviction, round-trip resize must preserve explicit line boundaries, meaningful blanks, cell attributes, hyperlinks, wide characters, cursor and saved-cursor meaning, pending-wrap behavior, viewport review position, selection meaning, and copied text.

The first delivery must:

1. Re-wrap retained primary content by logical line instead of copying a top-left rectangle or trimming every physical row.
2. Distinguish printed ordinary spaces, visibly styled or hyperlinked blank cells, unwritten capacity, and generated wide-edge padding.
3. Use durable logical content anchors for cursor, saved cursor, review position, selection endpoints, and future metadata consumers;
4. Preserve the current ring-buffer architecture while making logical identity and reflow metadata explicit;
5. Resize alternate screens as rectangular application surfaces while independently reflowing a saved primary screen;
6. Apply physical-row history capacity after reflow with explicit partial-line truncation and anchor invalidation;
7. Keep PTY and model geometry consistent when preparation or PTY resize fails; and
8. Provide deterministic model/copy regression coverage plus recorded Windows shell, `nvim`, and large-history resize evidence.

ADR-0018 remains the implemented non-reflow behavior until this specification is implemented and accepted. This specification does not itself change current runtime behavior.

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

### Logical stream and physical projection

Decode retained primary rows into a temporary logical atom stream shared by reflow and copy:

- a glyph atom carries its character, width of one or two cells, attributes, colors, hyperlink identity, and meaningful-blank state;
- a hard-break atom represents an explicit line boundary;
- a valid wide base/continuation pair becomes one width-two atom;
- continuation cells are not independent content; and
- generated padding used to avoid splitting a wide atom at the right edge is excluded from the logical stream and recreated during projection.

A continuation-row soft-wrap marker joins adjacent physical rows. A non-wrapped row boundary creates a hard break, including each explicit blank line. Reprojection packs complete atoms into the new width, regenerates continuation metadata and wide-edge padding, and never splits a width-two atom.

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

Prepare all model work before changing PTY geometry. The prepared resize owns all newly allocated normal/alternate/saved-primary buffers, row metadata, damage state, tab stops, cursor mappings, review mappings, selection projections, capacity results, and hyperlink reachability results.

The terminal resize sequence is:

1. normalize requested geometry;
2. prepare the complete model resize without mutating live state;
3. on preparation failure, return failure with PTY and model unchanged;
4. resize the PTY;
5. on PTY failure, discard prepared state and keep the model unchanged; and
6. on PTY success, install prepared state through an allocation-free, infallible ownership swap.

The committed resize restores a full-height scroll region, clamps horizontal margins, extends tab stops at eight-column intervals, marks the complete new viewport dirty, and cleans unreachable hyperlink registry entries without changing retained hyperlink identities.

### Copy behavior

Copy traverses the same logical content classification used by reflow. It joins soft-wrapped rows, emits hard breaks for explicit newlines and blank lines, skips continuation cells and generated padding, preserves meaningful ordinary and styled trailing spaces, and resolves retained hyperlink-bearing cells without treating URI metadata as copied text. Unconditional per-physical-row `trim_end()` is not part of the new algorithm.

### Seams

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Retained-content metadata | `NormalBuf` ↔ screen editing/cursor paths | Every write, erase, wrap, scroll, line operation, reset, and resize maintains logical identity, extent, and wide-cell invariants | Reconstructable logical lines and stable anchor resolution without replacing the ring buffer |
| Selection projection | pointer/`SelectionModel` ↔ Screen snapshot and anchor resolver | Pointer hit testing supplies current physical positions; Screen can map retained positions in both directions | Canonical selection anchors plus current render/copy bounds |
| Transactional geometry | `harbor-terminal::Terminal` ↔ `harbor-pty::PtyControl` | Prepared model state and a synchronous PTY resize result | PTY/model geometry changes together or the model remains unchanged |
| Alternate-screen ownership | active Screen ↔ saved primary and parked alternate Screens | ADR-0019 whole-screen isolation and buffer-specific resize policy | Resized primary restoration without text-reflowing full-screen application surfaces |
| Hyperlink retention | reflowed cells/pen state ↔ screen hyperlink registry | Compact IDs remain attached to retained atoms and all reachable owners participate in cleanup | Stable retained OSC 8 identity with bounded unreachable registry state |

## End-to-End Tests

### E2E: Round-trip long colored and CJK output

- **Given:** Main-screen and scrollback content containing long logical lines, CJK width-two glyphs, colors, attributes, hyperlinks, ordinary trailing spaces, styled blank cells, and explicit newlines.
- **When:** The terminal is repeatedly narrowed and widened, including a return to its original geometry.
- **Then:** Retained logical text, attribute and hyperlink identity, wide-cell invariants, selection meaning, and copied output match the original except for explicitly reported capacity eviction.

### E2E: Explicit blank lines and pending wrap

- **Given:** Content containing consecutive explicit blank lines and a live cursor at a right-margin insertion boundary with pending-wrap set.
- **When:** Width changes move that insertion boundary into a row interior, onto a new right edge, and back again.
- **Then:** Blank-line count and copy output remain stable, cursor projection follows the same logical insertion point, and pending-wrap is derived correctly at each width.

### E2E: Height-only review preservation

- **Given:** A primary screen with scrollback, a live cursor near the bottom, and a viewport reviewing older retained content.
- **When:** Height is reduced and then increased without changing width.
- **Then:** Removed top live rows enter history, growth pulls newest history back before adding blanks, the live cursor remains valid, and the review viewport stays anchored to the same retained content.

### E2E: Capacity eviction during narrow reflow

- **Given:** History near capacity, including one logical line that expands substantially at a narrower width, plus selections and metadata anchors in both old and retained portions.
- **When:** Reflow exceeds physical-row capacity.
- **Then:** Oldest physical rows are evicted, a partially retained line is marked head-truncated, anchors into evicted content are invalidated, retained anchors still resolve, an evicted review anchor moves to the oldest retained position, and no stale ring generation resolves to unrelated content.

### E2E: Alternate-screen resize and primary restoration

- **Given:** Primary history and selection exist before entering a persistent alternate screen used by `nvim`.
- **When:** Width and height change while the alternate screen is active and the application redraws, then Harbor exits alternate mode.
- **Then:** Alternate content followed rectangular resize policy without scrollback reflow, while the restored primary screen is reflowed to current geometry with retained content, cursor, review, and valid selection meaning.

### E2E: Resize failure is geometry-consistent

- **Given:** A valid terminal model and a PTY control configured to fail resize.
- **When:** Harbor prepares a different geometry and PTY resize fails.
- **Then:** Prepared state is discarded, model dimensions and projections remain unchanged, no selection is cleared merely because resize was attempted, and a later successful request can retry cleanly.

### E2E: Repeated mixed resize under output

- **Given:** Colored shell output, scrollback review, a retained selection, and intermittent new output.
- **When:** Width-only, height-only, and combined resizes repeat while output is processed between completed resize transactions.
- **Then:** Every committed state satisfies row, wide-cell, anchor, history, cursor, and PTY/model geometry invariants; copy returns the selected retained logical content unless documented eviction invalidated it.

## Decisions

### Durable logical anchors instead of physical generations

- **Choice:** Logical-line identity, logical offset, and affinity are canonical; generation/column is a current-layout projection.
- **Reason:** Width changes alter physical-row count, so retaining generation/column cannot preserve selection, review, cursor, search, or command meaning.
- **ADR reference:** [0042-content-anchors-and-buffer-specific-resize](../adr/0042-content-anchors-and-buffer-specific-resize.md)

### Explicit meaningful blanks and shared logical atoms

- **Choice:** Preserve printed spaces and visible styled/hyperlinked blanks explicitly, exclude unused capacity and generated padding, and use the same logical classification for reflow and copy.
- **Reason:** Cell value and unconditional trimming cannot distinguish all accepted text and presentation semantics.
- **ADR reference:** [0042-content-anchors-and-buffer-specific-resize](../adr/0042-content-anchors-and-buffer-specific-resize.md), [0035-cell-linked-bounded-registry-for-osc8-hyperlinks](../adr/0035-cell-linked-bounded-registry-for-osc8-hyperlinks.md)

### Preserve ring storage while extending row semantics

- **Choice:** Keep the bounded ring buffer and add logical metadata rather than introducing a document, rope, or unbounded history store.
- **Reason:** The accepted scope requires stable content semantics, not a wholesale storage rewrite.
- **ADR reference:** [0042-content-anchors-and-buffer-specific-resize](../adr/0042-content-anchors-and-buffer-specific-resize.md)

### Buffer-specific resize

- **Choice:** Reflow primary content, resize alternate screens as rectangular surfaces, and independently reflow a saved primary while alternate mode is active.
- **Reason:** Shell history is a logical text stream, while full-screen applications own two-dimensional redraw and ADR-0019 requires buffer isolation.
- **ADR reference:** [0019-alternate-screen-buffer-isolation](../adr/0019-alternate-screen-buffer-isolation.md), [0042-content-anchors-and-buffer-specific-resize](../adr/0042-content-anchors-and-buffer-specific-resize.md)

### Preserve wide-cell invariants by requiring two columns

- **Choice:** Normalize terminal width to at least two and project each width-two atom as a complete base/continuation pair.
- **Reason:** This preserves CJK content without replacement, hidden overflow state, or renderer-side invariant repair.
- **ADR reference:** [0016-wide-cell-normalization-during-editing](../adr/0016-wide-cell-normalization-during-editing.md), [0042-content-anchors-and-buffer-specific-resize](../adr/0042-content-anchors-and-buffer-specific-resize.md)

### Prepare before PTY resize and commit infallibly

- **Choice:** Allocate and validate the complete model result first, then resize PTY, then commit with an ownership swap.
- **Reason:** PTY-first remains safe only if all subsequent model work is guaranteed not to fail; preparation makes that guarantee explicit while preserving synchronous I/O ownership.
- **ADR reference:** [0013-synchronous-pty-io](../adr/0013-synchronous-pty-io.md), [0042-content-anchors-and-buffer-specific-resize](../adr/0042-content-anchors-and-buffer-specific-resize.md)

### Replace ADR-0018 only after accepted implementation

- **Choice:** Treat ADR-0018 as current behavior until production reflow and its evidence are accepted; then update decision status/history explicitly.
- **Reason:** A specification is not an implementation-completion claim.
- **ADR reference:** [0018-non-reflow-resize-preserved-soft-wrap-markers](../adr/0018-non-reflow-resize-preserved-soft-wrap-markers.md), [0042-content-anchors-and-buffer-specific-resize](../adr/0042-content-anchors-and-buffer-specific-resize.md)

## Test Plan

- **Reference-model and fixture tests:** Define logical atom decode/project fixtures before switching production resize. Cover long lines, explicit and soft line boundaries, consecutive blank lines, ordinary and styled trailing spaces, hyperlinks, pending wrap, CJK at boundaries, partially selected wide glyphs, head-truncated lines, and repeated round trips.
- **Normal-buffer tests:** Verify identity allocation/non-reuse, ring wraparound, metadata movement for scroll and structural edits, width/height policies, physical capacity, partial-line eviction, full damage, and no stale generation-to-content aliasing.
- **Anchor tests:** Verify before/after affinity under insert/delete, overwrite stability, row movement, reflow projection, destruction/eviction invalidation, review fallback, cursor insertion boundaries, saved cursor, and complete-selection invalidation when one endpoint is lost.
- **Selection/copy tests:** Verify pixel/`GenPos`/anchor conversion, forward and reverse drag, word and logical-line selection, selection retention across output and resize, meaningful trailing blanks, explicit newlines, soft-wrap joining, wide continuation skipping, and eviction behavior.
- **Alternate-screen tests:** Cover `?47`, `?1047`, `?1048`, and `?1049` resize/restore behavior, active and parked alternate buffers, saved-primary reflow, no alternate scrollback, RIS, and current whole-screen isolation.
- **Failure tests:** Inject model preparation failure and PTY resize failure; prove no partial model commit, no premature pointer clearing, unchanged model geometry on failure, and successful retry.
- **Focused commands:** Run targeted `harbor-terminal` and `harbor-pty` tests during development, then the standard gates: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `python scripts/check_docs.py`, and `python scripts/checklist_summary.py`.
- **Windows runtime evidence:** Record revision, dirty-tree scope, Windows/ConPTY/application versions, exact steps, expected and observed results, PASS/FAIL/NOT RUN/BLOCKED outcome, artifacts, and exclusions for PowerShell or `cmd`, `nvim`, clipboard selection/copy, primary/alternate transitions, and repeated mixed resize.
- **Performance evidence:** Record large-history resize latency and memory/cost under fixed machine, font, viewport, history, and build-profile conditions. This specification sets no unsupported pass threshold; preserve comparable captures so a threshold or optimization can be justified from evidence.

## Out of Scope

- Replacing the ring buffer with a rope, document model, or unlimited history.
- Search UI, command navigation UI, pane UI, font reload, or font shaping.
- Completing combining-mark, variation-selector, ZWJ emoji, ambiguous-width, or complex-script behavior assigned to N02.
- Reflowing alternate-screen application drawings as logical text.
- Changing OSC 8 activation policy, URI validation, or hyperlink ownership beyond preserving and cleaning existing IDs.
- Claiming Windows or application compatibility from model tests without the required runtime evidence.

## Future Evolution

- N02 may evolve a glyph atom into a richer text cluster while preserving logical-line identity and anchor semantics.
- N08 search and N09 shell-command navigation can store content anchors and use the same eviction invalidation contract.
- N15 may make history capacity configurable and establish measured resize budgets without changing bounded eviction semantics.
- After implementation and acceptance, supersede or update ADR-0018 explicitly and change ADR-0042 from Proposed to the appropriate implementation status.
