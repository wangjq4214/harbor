# Logical Atom and Copy Contract

**Ticket ID:** T0002
**Source:** [Spec 0014](../../spec/0014-content-preserving-resize-reflow.md), GitHub #169
**Status:** Done

## Goal

Provide one deterministic logical-content decoder used by copy and future reflow so physical wrapping, wide-cell padding, and unconditional row trimming no longer define retained text semantics.

## Affected Surfaces

- **Logical content model:** Temporary glyph and hard-break atoms with character, width, style, hyperlink, and meaningful-blank state.
- **Screen reader/copy:** Replace physical-row `trim_end()` extraction with logical stream traversal.
- **Normal-buffer queries:** Expose bounded retained rows and row metadata in logical sequence order without leaking ring layout.
- **Fixtures/reference model:** Worked examples for long lines, explicit blank lines, ordinary/styled trailing spaces, hyperlinks, CJK, pending wrap, and ring wraparound.

## Approach

Decode soft-wrap-linked rows into glyph atoms and emit hard breaks only at explicit row boundaries. Collapse each valid wide base/continuation pair into one width-two atom; omit continuation cells and generated edge padding as independent content. Preserve meaningful ordinary and styled blanks exactly. Share this decoder with copy now and make its projection contract consumable by T0004 rather than implementing a second reflow-specific interpretation.

Keep first-delivery offsets defined over the accepted atom stream. Explicitly retain N02 combining/variation-selector/ZWJ limitations without weakening current `char` and wide-cell coverage.

## Dependencies and Coordination

- **Blocked by:** T0001 — atom boundaries and meaningful content require stable row metadata and extent.
- **Blocks:** T0003 — content anchors use atom-stream offsets; T0004 — width reflow consumes the same decoder.
- **Coordination risks:** `screen/reader.rs`, `normal_buf.rs`, and selection/copy tests overlap later anchor work. Keep the decoder crate-internal and independent of selection ownership.

## Acceptance

- [ ] One logical decoder reconstructs soft-wrapped lines and explicit hard breaks from retained ring order.
- [ ] Consecutive explicit blank lines produce corresponding copy newlines.
- [ ] Printed trailing spaces and visible styled/hyperlinked blanks survive copy; unused capacity and generated padding do not.
- [ ] Wide continuations never duplicate copied characters, and malformed/orphan wide cells are rejected or normalized through existing invariants rather than silently treated as content.
- [ ] Hyperlink IDs and all cell attributes remain attached to glyph atoms while URI values are not inserted into copied text.
- [ ] `selected_text` no longer relies on unconditional per-physical-row `trim_end()` for the covered logical range.
- [ ] Reference fixtures cover long colored lines, CJK boundaries, explicit and soft line breaks, pending-wrap content, ring wraparound, and current N02 exclusions.

## Out of Scope

- Reprojecting atoms to a new width.
- Combining-mark, grapheme-cluster, variation-selector, ZWJ, or shaping implementation.
- Search, command navigation, or hyperlink activation changes.
