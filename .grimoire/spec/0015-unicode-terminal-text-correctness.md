# Unicode terminal text correctness (N02 / #153)

**Spec ID:** 0015
**Status:** Implemented
**Date:** 2026-09-26

**Stable execution aliases:** 0013/T0001 = combining, T0002 = VS/ZWJ; 0013/T0003 (also Spec0015/T0003) = Verification and its unresolved discrepancy below. Historical Done/checked acceptance does not supply missing runtime evidence or close #153; closure requires evidence for accepted scope or explicit decisions linking deferred work.

## Requirements

1. Ordinary terminal output preserves combining marks with their base in retained content and copied text, draws them together, and does not advance an extra cell. A combining mark received alone at the start of a line remains in the original text, occupies one cell, and uses a display-only dotted-circle cue; the cue is never copied.
2. Variation selectors and ZWJ emoji sequences use a shared retained-text and cell-width contract. Ordinary ambiguous-width characters occupy one cell; sequences explicitly requesting emoji presentation, including `♥️` and `👩‍💻`, occupy two. If the selected font cannot render a whole sequence, the original text and assigned width remain intact and a documented display fallback is permitted. Preserving copy text alone is not evidence of correct presentation.
3. Rendering, screen edits, selection, copy, logical-line reflow, content-anchor projection, and damage tracking must agree on the same text units and widths. Future search must consume this text representation rather than reconstructing a competing scalar/cell model; implementing a search UI is not required by N02.
4. Cover sequences divided across PTY reads, wide-cell/right-edge placement, overwrite and erase, font fallback, and font/DPI changes, including selection and copy before and after resize.

Source of scope: [issue #153](https://github.com/wangjq4214/harbor/issues/153) and N02 planning scope. The clarified width, orphan-mark, and fallback decisions are recorded in [ADR-0044](../adr/0044-unicode-text-unit-and-presentation-width-policy.md).

## Solution

Extend the retained screen text unit beyond a single `char` while retaining its original Unicode scalar sequence, assigned width, style, hyperlink, and meaningful-blank semantics. A combining mark attached to its base extends that unit without a cell advance; zero-width format and presentation scalars must be handled by the sequence policy rather than discarded wholesale or independently advanced. The same unit projects into one or two grid cells. No particular storage container, Unicode segmentation library, or shaping engine is mandated by this spec.

The streaming parser already decodes UTF-8 across reads and emits individual characters; the screen writer must preserve the needed sequence state across those callbacks and PTY read boundaries. An isolated line-start mark has a one-cell projection with an auxiliary dotted-circle display cue but copies only its source mark. Width is derived consistently for the retained unit, including the agreed emoji and ambiguous-width cases, rather than recalculated differently for writing, selection, reflow, and rendering.

N01's logical-line identity, atom-offset anchors, meaningful blanks, wide-cell pairing, and primary/alternate resize policies remain authoritative. Evolve a glyph atom into a text-bearing unit without treating continuation cells or generated right-edge padding as separate content. Appending a mark, completing a variation/ZWJ sequence, overwriting or erasing a unit, and changing its physical width must keep content anchors, pending wrap, cell invariants, and selection projections coherent. Dirty-range updates include the displayed base cell when its text or presentation changes without cursor motion. Alternate-screen rectangular resize and primary/history reflow retain their established distinctions.

The renderer presents the retained unit in its assigned cell bounds, including font fallback where available. When complete presentation is unsupported, fallback must not corrupt underlying text or shift neighboring cells; the supported and unsupported visual outcomes must be stated in evidence. Existing IME preedit rendering is a transient input overlay, not a substitute for ordinary output storage.

## Decisions and boundaries

- [ADR-0044](../adr/0044-unicode-text-unit-and-presentation-width-policy.md) records the shared text/width, orphan-mark and fallback policy; its historical implementation status is not full runtime acceptance.
- **Approved future exception, not implemented:** [ADR 0073](../adr/0073-session-history-accounting-and-text-unit-overflow.md), specified in [Spec 0031](0031-bounded-history-and-performance-budgets.md), qualifies only unbounded full retention for exceptionally oversized text units: a fixed protective limit retains the admitted prefix and discards further extensions until the next independent unit, without retained orphan/synthetic content. Ordinary Unicode text/width/fallback policy is unchanged. This exception does not resolve or authorize the no-wrap width discrepancy below.
- [ADR-0042](../adr/0042-content-anchors-and-buffer-specific-resize.md) defines N01 logical anchors, atoms, blank provenance and buffer-specific resize. N02 evolves text-bearing atoms without reverting to physical-row identity; [N01 spec](0014-content-preserving-resize-reflow.md) explicitly defers Unicode cluster semantics to N02. N01 runtime and performance acceptance remains open.
- A font's exact substitute glyph, a specific shaping library, and an internal storage layout are not settled project decisions; this spec does not choose them. Any additional text/coordinate semantics needed by implementation must return to clarification and recording before being added as requirements.

## Verification

Add deterministic parser/screen tests for PTY fragmentation, zero-width retention, cluster width and cursor/pending-wrap, line-start isolation, wide edges, insert/overwrite/erase and protected/styled/hyperlinked cells. Verify logical-stream decode, selected text, content-anchor remapping, eviction, primary reflow, alternate resize, and dirty-range uploads against the same original sequences. Check font fallback and font/DPI rerendering in Windows runtime, not only model tests; include a named terminal application workload and clipboard copy across resize.

For each delivered increment, record focused tests and reproducible Windows end-to-end steps, revision and dirty-tree scope, OS/ConPTY/application versions, expected versus observed outcome, artifacts, known exclusions, and honest PASS / FAIL / NOT RUN / BLOCKED statuses in self-contained committed summaries, with raw artifacts retained locally or in retrievable CI. Run or account for `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `python scripts/docs/check_docs.py`; update status/protocol claims only for evidenced behavior. See Validation. Parser/model tests alone do not prove Windows runtime compatibility.

### Unresolved acceptance and discrepancy

Historical T0001 (combining) and T0002 (VS/ZWJ) completion included focused verification; their completion does not establish every runtime matrix row. The variation/ZWJ verification documented a DECAWM-disabled final-column selector retaining raw text with a one-cell projection, in conflict with R2's two-cell emoji policy. This discrepancy remains unresolved; retiring the implementation tickets neither authorizes that exception nor changes R2.

Integrated Windows verification covered the documented combining, selector/ZWJ width, delayed-write, wrapping-edge, edit/erase and clipboard subset. It did not document font/DPI-change or resize/alternate/eviction executions, a named application workload/version, full runtime and dirty-tree provenance/artifacts, or the integrated `cargo test --workspace` gate (it lists package tests). Delayed writes alone do not establish distinct PTY reads. T0003's Done status and checked acceptance do not fill these evidence gaps or resolve the no-wrap discrepancy; the Verification obligations above remain applicable.

## Out of scope

Full complex-script shaping and optional font ligatures are not prerequisites for basic combining correctness. IME preedit is not this work. Search UI, a wholesale screen-buffer rewrite, and blanket claims of emoji/font interoperability are not authorized by #153. N01's outstanding Windows/performance acceptance is tracked separately, not declared complete here.
