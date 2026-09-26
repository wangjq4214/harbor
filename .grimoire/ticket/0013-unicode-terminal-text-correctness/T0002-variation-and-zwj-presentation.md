# Variation selectors and ZWJ presentation

**Ticket ID:** T0002
**Source:** [Spec 0015 R2/R3/R4](../../spec/0015-unicode-terminal-text-correctness.md), [ADR-0044](../../adr/0044-unicode-text-unit-and-presentation-width-policy.md), [ADR-0042](../../adr/0042-content-anchors-and-buffer-specific-resize.md)
**Status:** Todo

## Goal

Variation-selector and ZWJ emoji text remains intact in screen content, rendering, selection and copy, with ordinary ambiguous-width characters occupying one cell and explicitly emoji-presented sequences occupying two, including unsupported-font fallback without a width change.

## Affected surfaces

- **Text-unit extension and geometry:** `screen/edit/cell_writer.rs`, `model.rs`, `normal_buf.rs`, `logical_content.rs`, `primary_reflow.rs`, `content_anchor.rs`, `selection_model.rs`, and cell-edit invariants.
- **Presentation:** `render/text.rs`, `damage.rs`, `harbor-text` glyph atlas and DirectWrite fallback, plus font/DPI changes that invalidate old glyph presentation.
- **Focused verification:** fragmented parser/screen input, width/edge/edit/copy/reflow tests, fallback and renderer tests.

## Approach

Extend T0001's single retained text-unit and width source to cover variation selectors and ZWJ emoji sequences; do not create a parallel width interpretation for rendering, copy, or reflow. Preserve the input scalar sequence, including selectors and joiners, even when a fallback font cannot present the whole unit. Keep the assigned width and neighboring cells stable through sequence completion, wide-edge placement, edits and repeated resize. Render within the assigned cells using available font/fallback capabilities, document unsupported presentation rather than claiming full emoji interoperability, and dirty all changed cells after a unit's text or presentation changes. Exact substitute glyphs, storage structure, and a particular shaping engine remain implementation choices, not newly settled policy.

## Dependencies and coordination

- **Blocked by:** T0001's retained multi-scalar text-unit/copy/reflow contract; current single-character cells drop width-zero sequence members.
- **Blocks:** T0003's final integrated Windows/clipboard/font evidence.
- **Coordination risks:** Width and glyph cache changes overlap T0001's writer, renderer, copy, and reflow paths. Consume its shared unit and preserve N01 anchor/alternate-screen semantics; do not let font availability choose logical cell width.

## Acceptance

- [ ] `♥` is treated under the ordinary ambiguous-width one-cell policy while `♥️` and `👩‍💻` occupy two cells; selections and copy retain their exact original scalar sequences in the same or split PTY reads.
- [ ] Variation selectors and ZWJ members are not silently dropped, copied as synthetic replacement text, or advanced as independent extra cells; font-unsupported whole sequences retain source text and assigned width with an explicitly documented visual fallback.
- [ ] Sequence completion next to a right edge, wide-cell boundary, or pending-wrap does not split a two-cell unit or corrupt adjacent content; overwrite, erase, protected/styled/hyperlinked cells, and selection projections remain coherent.
- [ ] Copy and content anchors retain meaning through repeated primary reflow and capacity eviction; alternate-screen rectangular resize and saved-primary reflow follow existing N01 rules.
- [ ] Presentation and dirty-range tests demonstrate repaint after a selector/joiner or font/DPI change; fallback-font results distinguish text preservation from correct whole-sequence visual rendering.
- [ ] No new terminal-search UI is added; the shared text/anchor contract remains usable by future N08 search without a competing per-scalar width model.

## Out of scope

Full complex-script shaping, optional ligatures and a promise of universal color emoji or fallback font coverage are not required. Windows acceptance of the combined N02 package belongs to T0003.
