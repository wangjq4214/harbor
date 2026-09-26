# Unicode terminal text correctness (N02 / #153)

**Spec ID:** 0015
**Status:** Draft
**Date:** 2026-09-26

## Requirements

1. Ordinary terminal output preserves combining marks with their base in retained content and copied text, draws them together, and does not advance an extra cell. A combining mark received alone at the start of a line remains in the original text, occupies one cell, and uses a display-only dotted-circle cue; the cue is never copied.
2. Variation selectors and ZWJ emoji sequences use a shared retained-text and cell-width contract. Ordinary ambiguous-width characters occupy one cell; sequences explicitly requesting emoji presentation, including `♥️` and `👩‍💻`, occupy two. If the selected font cannot render a whole sequence, the original text and assigned width remain intact and a documented display fallback is permitted. Preserving copy text alone is not evidence of correct presentation.
3. Rendering, screen edits, selection, copy, logical-line reflow, content-anchor projection, and damage tracking must agree on the same text units and widths. Future search must consume this text representation rather than reconstructing a competing scalar/cell model; implementing a search UI is not required by N02.
4. Cover sequences divided across PTY reads, wide-cell/right-edge placement, overwrite and erase, font fallback, and font/DPI changes, including selection and copy before and after resize.

Source of scope: [issue #153](https://github.com/wangjq4214/harbor/issues/153) and [N02 planning scope](../../docs/next-stage-plan.md). The clarified width, orphan-mark, and fallback decisions are recorded in [ADR-0044](../adr/0044-unicode-text-unit-and-presentation-width-policy.md).

## Solution

Extend the retained screen text unit beyond a single `char` while retaining its original Unicode scalar sequence, assigned width, style, hyperlink, and meaningful-blank semantics. A combining mark attached to its base extends that unit without a cell advance; zero-width format and presentation scalars must be handled by the sequence policy rather than discarded wholesale or independently advanced. The same unit projects into one or two grid cells. No particular storage container, Unicode segmentation library, or shaping engine is mandated by this spec.

The streaming parser already decodes UTF-8 across reads and emits individual characters; the screen writer must preserve the needed sequence state across those callbacks and PTY read boundaries. An isolated line-start mark has a one-cell projection with an auxiliary dotted-circle display cue but copies only its source mark. Width is derived consistently for the retained unit, including the agreed emoji and ambiguous-width cases, rather than recalculated differently for writing, selection, reflow, and rendering.

N01's logical-line identity, atom-offset anchors, meaningful blanks, wide-cell pairing, and primary/alternate resize policies remain authoritative. Evolve a glyph atom into a text-bearing unit without treating continuation cells or generated right-edge padding as separate content. Appending a mark, completing a variation/ZWJ sequence, overwriting or erasing a unit, and changing its physical width must keep content anchors, pending wrap, cell invariants, and selection projections coherent. Dirty-range updates include the displayed base cell when its text or presentation changes without cursor motion. Alternate-screen rectangular resize and primary/history reflow retain their established distinctions.

The renderer presents the retained unit in its assigned cell bounds, including font fallback where available. When complete presentation is unsupported, fallback must not corrupt underlying text or shift neighboring cells; the supported and unsupported visual outcomes must be stated in evidence. Existing IME preedit rendering is a transient input overlay, not a substitute for ordinary output storage.

### Necessary seams

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Streaming print | `harbor-parser` → `harbor-terminal` screen writer | Ordered Unicode scalar callbacks across arbitrary PTY read splits | Retained text units and consistent cursor/cell advance |
| Logical content and geometry | Screen cells ↔ `logical_content`, `content_anchor`, `primary_reflow`, selection and copy | Original unit text, width, metadata, logical-line identity and offsets | Stable content meaning and physical projections through edits, wraps and resize |
| Presentation and damage | Screen text units → `render/text`, `damage`, `harbor-text` glyph/fallback backend | Text, assigned cell bounds and invalidated grid range | Rendered text or documented fallback without altering stored content/width |

## End-to-End Tests

- **Combining text:** Given a PTY emits `e` followed by U+0301 in the same or separate reads, when displayed and selected, then the accent appears on the base, the cursor advances only for the base, and copy returns the original `e` + U+0301 sequence.
- **Orphan mark:** Given a line begins with U+0301, when displayed and copied, then it occupies one cell with a dotted-circle cue, while copy contains only U+0301.
- **Emoji presentation:** Given `♥`, `♥️`, and `👩‍💻` in adjacent content, when printed, selected, and copied, then ordinary ambiguous-width text follows the one-cell policy, explicitly emoji-presented units the two-cell policy, and copied text retains each input sequence. A missing complete glyph uses the documented visual fallback without changing those widths.
- **Edit and boundary:** Given mixed narrow, combining, and wide text at a right margin, when an application overwrites, inserts, or erases affected cells, then wide-cell pairing, cursor/pending-wrap, selection projection, copied text, and redraw agree on surviving units.
- **Reflow and rendering changes:** Given retained primary/history text with a selected combined or emoji unit, when the window narrows and widens or the font/DPI changes, then retained text and copy results survive except for documented capacity eviction, no unit splits across wide-cell boundaries, and the affected cells repaint. Alternate-screen content follows its existing rectangular resize policy.

## Decisions and boundaries

- [ADR-0044](../adr/0044-unicode-text-unit-and-presentation-width-policy.md) records the shared text/width policy, orphan-mark cue, ambiguous-width default, explicit emoji width, and unsupported-font boundary; it is a proposed implementation decision, not a completed behavior claim.
- [ADR-0042](../adr/0042-content-anchors-and-buffer-specific-resize.md) defines N01 logical anchors, atoms, blank provenance and buffer-specific resize. N02 evolves text-bearing atoms without reverting to physical-row identity; [N01 spec](0014-content-preserving-resize-reflow.md) explicitly defers Unicode cluster semantics to N02. N01 runtime and performance acceptance remains open.
- A font's exact substitute glyph, a specific shaping library, and an internal storage layout are not settled project decisions; this spec does not choose them. Any additional text/coordinate semantics needed by implementation must return to clarification and recording before being added as requirements.

## Verification

Add deterministic parser/screen tests for PTY fragmentation, zero-width retention, cluster width and cursor/pending-wrap, line-start isolation, wide edges, insert/overwrite/erase and protected/styled/hyperlinked cells. Verify logical-stream decode, selected text, content-anchor remapping, eviction, primary reflow, alternate resize, and dirty-range uploads against the same original sequences. Check font fallback and font/DPI rerendering in Windows runtime, not only model tests; include a named terminal application workload and clipboard copy across resize.

For each delivered increment, record focused tests and reproducible Windows end-to-end steps, revision and dirty-tree scope, OS/ConPTY/application versions, expected versus observed outcome, artifacts, known exclusions, and honest PASS / FAIL / NOT RUN / BLOCKED statuses under `docs/verification/` or retrievable CI evidence. Run or account for `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `python scripts/check_docs.py`, and `python scripts/checklist_summary.py`; update status/protocol claims only for evidenced behavior. See [Validation](../../docs/validation.md). Parser/model tests alone do not prove Windows runtime compatibility.

## Out of scope

Full complex-script shaping and optional font ligatures are not prerequisites for basic combining correctness. IME preedit is not this work. Search UI, a wholesale screen-buffer rewrite, and blanket claims of emoji/font interoperability are not authorized by #153. N01's outstanding Windows/performance acceptance is tracked separately, not declared complete here.

## Future evolution

Further shaping and font coverage can be considered when named workloads require them. N08 search can build on the established text and anchor contract without redefining its width or copy semantics. Deliver #153 in bounded increments and close it only when the accepted scope has evidence or explicit scope decisions link all deferred work.
