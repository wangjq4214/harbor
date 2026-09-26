# Unicode terminal text correctness (N02 / #153)

**Source:** [Spec 0015](../../spec/0015-unicode-terminal-text-correctness.md), [ADR-0044](../../adr/0044-unicode-text-unit-and-presentation-width-policy.md), [ADR-0042](../../adr/0042-content-anchors-and-buffer-specific-resize.md), [issue #153](https://github.com/wangjq4214/harbor/issues/153)
**Ticket folder:** `.grimoire/ticket/0013-unicode-terminal-text-correctness/`

## Overview

Deliver #153 in bounded increments: preserve and display combining marks through editing, copy, and N01 reflow; extend the same text unit to variation-selector and ZWJ emoji sequences with agreed width and documented font fallback; then record integrated Windows runtime evidence and update only supported status claims. Existing IME preedit and N01 automated reflow tests do not establish ordinary-output Unicode or Windows acceptance. These tickets do not authorize full complex-script shaping, ligatures, search UI, or a wholesale buffer rewrite.

## Delivery surfaces

- **Terminal model and write/edit paths:** `model.rs`, `screen.rs`, `screen/edit/cell_writer.rs`, `screen/edit/cell_ops.rs`, `normal_buf.rs`, and incremental `harbor-parser` callbacks.
- **Content and coordinates:** `logical_content.rs`, `content_anchor.rs`, `primary_reflow.rs`, `selection_model.rs`, copy paths, wide-cell invariants and buffer-specific resize.
- **Presentation:** `render/text.rs`, `render/layout.rs`, `damage.rs`, `harbor-text` atlas/DirectWrite fallback, font/DPI update handling.
- **Acceptance:** focused tests, Windows shell/clipboard/app sessions, revision and version records, standard quality gates, and documentation under `docs/verification/`.

## Dependency graph

| Ticket | Blocks | Concrete reason |
| --- | --- | --- |
| T0001 | T0002 | VS/ZWJ text and width cannot be retained or verified end-to-end while the single-`char` screen/logical representation drops zero-width sequence members; T0002 consumes T0001's text-unit contract. |
| T0001, T0002 | T0003 | Final scope and runtime evidence must exercise the integrated behavior at an identified revision; evidence from a partial build cannot close #153. |

## Coordination risks

| Tickets | Risk | Strategy |
| --- | --- | --- |
| T0001, T0002 | Both touch screen text storage, width projection, rendering, copy/reflow and damage. | T0001 establishes one source-of-truth text unit and tests; T0002 extends it without adding a second width or copy path. |
| T0003, implementation tickets | Runtime records can become stale after behavioral changes. | Prepare fixtures early, but capture final evidence against the integrated revision and record dirty-tree scope. |
| N01 acceptance, T0003 | N01 Windows/performance work is still open and shares resize scenarios. | Reuse reproducible scenarios where useful, but report N01 and N02 results separately; do not imply one closes the other. |

## Parallel candidates and recommended order

The final evidence capture is blocked by both implementation outcomes, though fixture preparation can proceed alongside either. Recommended sequence: T0001 (combining correctness), T0002 (VS/ZWJ and fallback), T0003 (integrated Windows and documentation acceptance). File overlap is coordination risk, not an additional dependency. These are reversible draft execution boundaries, not a new product policy.

## Requirement coverage

| Spec 0015 requirement | Tickets |
| --- | --- |
| R1 combining and isolated mark with display-only cue | T0001; T0003 runtime evidence |
| R2 VS/ZWJ, ambiguous/emoji width and unsupported-font fallback | T0002; T0003 runtime evidence |
| R3 shared editing/selection/copy/reflow/anchors/damage and future-search compatibility | T0001 establishes the shared unit; T0002 extends it; T0003 verifies the integrated contract |
| R4 fragmented reads, wide edges, edits, fallback and font/DPI changes | T0001 and T0002 focused tests; T0003 Windows runtime and clipboard evidence |

## Ticket index

| Ticket | File | Outcome |
| --- | --- | --- |
| T0001 | [T0001-combining-text-end-to-end.md](./T0001-combining-text-end-to-end.md) | Combining text, including isolated marks, survives writing, edits, copy, reflow and rendering without extra advance. |
| T0002 | [T0002-variation-and-zwj-presentation.md](./T0002-variation-and-zwj-presentation.md) | VS/ZWJ sequences share a stable width/text unit with coherent rendering, edits, fallback and resize. |
| T0003 | [T0003-windows-runtime-and-evidence.md](./T0003-windows-runtime-and-evidence.md) | Integrated quality gates, Windows runtime/clipboard/font evidence and accurate status documentation for #153. |
