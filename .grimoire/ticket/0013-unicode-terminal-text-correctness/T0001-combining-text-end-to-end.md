# Combining text end-to-end

**Ticket ID:** T0001
**Source:** [Spec 0015 R1/R3/R4](../../spec/0015-unicode-terminal-text-correctness.md), [ADR-0044](../../adr/0044-unicode-text-unit-and-presentation-width-policy.md), [ADR-0042](../../adr/0042-content-anchors-and-buffer-specific-resize.md)
**Status:** Done

## Goal

Ordinary terminal content preserves and paints combining marks with their base without an extra cursor advance, and selects/copies the original text through edits, wraps and resize. An isolated line-start mark gets a display-only dotted-circle cue and one cell while copying only its original scalar sequence.

## Affected surfaces

- **Retained text and editing:** `model.rs`, `screen.rs`, `screen/edit/cell_writer.rs`, `screen/edit/cell_ops.rs`, `normal_buf.rs`; parser-to-screen print boundary for sequences split across PTY reads.
- **Logical coordinates and copy:** `logical_content.rs`, `content_anchor.rs`, `primary_reflow.rs`, `selection_model.rs` and terminal copy path; preserve N01 logical identity, width and blank provenance.
- **Presentation:** `render/text.rs`, `damage.rs`, and the existing `harbor-text` glyph path for a combined base/mark and the display-only cue.
- **Focused verification:** screen, logical-content, reflow, selection, rendering, and parser fragmentation tests.

## Approach

Evolve the retained glyph/text unit to hold its original scalar sequence and grid width. Attach incoming combining marks to their eligible base without treating PTY read boundaries as text boundaries or advancing the cursor; for the agreed isolated line-start case retain the mark in a one-cell unit and draw the synthetic cue without storing it in copied text. Keep wide continuation and generated edge padding as projections, not independent copied content. Make editing and reflow use the same unit/width and keep content-anchor offsets meaningful across a text-unit extension. Mark a base cell dirty when a mark arrives after the base has already been rendered. Preserve current primary reflow and alternate rectangular resize policies.

Do not commit to a storage container, shaping library, or extra orphan contexts beyond the recorded contract. If those require a new semantic rule, return to clarify and record before changing acceptance.

## Dependencies and coordination

- **Blocked by:** None; the N01 source/anchor contract and ADR-0044 already exist.
- **Blocks:** T0002, which requires retained multi-scalar units and one authoritative text/width path; T0003 final integrated evidence.
- **Coordination risks:** T0002 will extend the same text-unit, renderer, and reflow paths. Preserve a single shared contract, not a combining-only copy/render workaround.

## Acceptance

Completion basis: automated checks passed and the user explicitly accepted T0001, reporting that the remaining behavior has no issues. The runtime details were not supplied; checkbox closure records user sign-off, not independently captured evidence for every matrix row. See `docs/verification/combining-text-t0001.md`.

- [x] A stream containing `e` then U+0301, whether in one or separate PTY reads, displays the accent on the base, advances only one cell, and copies exactly `e` + U+0301.
- [x] A line-start U+0301 occupies one cell, shows a synthetic dotted-circle cue, and copies only U+0301; the cue is absent from retained logical text.
- [x] Selection, copy, and primary reflow preserve combined text and style/hyperlink/meaningful-blank semantics across soft wraps, explicit breaks, repeated resize and documented capacity eviction; alternate-screen resize remains rectangular.
- [x] Overwrite, insert, erase, protected-cell and wide-cell/right-edge cases maintain complete grid units, cursor/pending-wrap and content-anchor/selection projections without retaining discarded marks or splitting wide bases.
- [x] Mutating a previously rendered base with a mark dirties the affected grid range and redraws it; focused tests cover the renderer path and split reads. Existing ordinary and wide-character tests remain passing.
- [x] Focused tests and reproducible steps are recorded for this increment; runtime presentation is not claimed solely from parser/model tests.

## Out of scope

VS/ZWJ emoji width and unsupported whole-sequence font presentation belong to T0002. Full complex-script shaping, ligatures and IME preedit changes are not required.
