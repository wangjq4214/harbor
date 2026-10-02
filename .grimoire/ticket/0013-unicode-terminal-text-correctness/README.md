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

## Dependencies and retained acceptance

[T0003 — Windows runtime and evidence](./T0003-windows-runtime-and-evidence.md) remains retained with its existing Done status and acceptance markers unchanged. Final evidence must exercise the integrated combining and VS/ZWJ text-unit/width contract in [Spec 0015 Requirements](../../spec/0015-unicode-terminal-text-correctness.md#requirements) and [Solution](../../spec/0015-unicode-terminal-text-correctness.md#solution), not a partial build. Combining storage precedes VS/ZWJ extension; both share one writer, renderer, copy/reflow and damage interpretation rather than parallel width paths. Fixture preparation can proceed separately, but final records require an identified integrated revision and dirty-tree scope; later changes require affected captures to be rerun.

N01 Windows/performance acceptance remains separate. Reuse reproducible resize scenarios where useful without implying N02 closes N01.

## Historical implementation IDs and destinations

Historical T0001 and T0002 contracts survive in the destinations below. Their historical Done statuses do not supply missing per-scenario runtime evidence. Stable IDs map to surviving requirements and evidence:

| Historical ID | Surviving contract | Historical evidence |
| --- | --- | --- |
| T0001 — combining | [Spec 0015 Requirements](../../spec/0015-unicode-terminal-text-correctness.md#requirements) R1/R3/R4 and [Solution](../../spec/0015-unicode-terminal-text-correctness.md#solution): shared text unit, isolated display-only cue, edits/copy/reflow/anchors/damage | [Combining verification](../../../docs/verification/combining-text-t0001.md): automated checks and user acceptance; extended runtime matrix not independently documented. |
| T0002 — variation/ZWJ | [Spec 0015 Requirements](../../spec/0015-unicode-terminal-text-correctness.md#requirements) R2/R3/R4 and [Solution](../../spec/0015-unicode-terminal-text-correctness.md#solution): retained scalars, assigned width, shared geometry and documented visual fallback | [Variation/ZWJ verification](../../../docs/verification/variation-zwj-t0002.md): automated checks, scope-unconfirmed user smoke report and unresolved no-wrap final-column width. |

## Evidence boundaries

[Integrated Windows evidence](../../../docs/verification/unicode-terminal-text-t0003.md) records its documented subset, not every obligation in [Spec 0015 Verification](../../spec/0015-unicode-terminal-text-correctness.md#verification). The spec's [unresolved acceptance and discrepancy](../../spec/0015-unicode-terminal-text-correctness.md#unresolved-acceptance-and-discrepancy) preserves the no-wrap one-cell versus R2 two-cell discrepancy and missing font/DPI, resize, named-application/provenance and integrated-gate evidence. Retirement does not reconcile these gaps, change requirements, or establish whole-sequence emoji rendering.
