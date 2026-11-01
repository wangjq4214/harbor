# Two-cell sequence promotion wraps with DECAWM disabled

**Status:** Testing
**Date:** 2026-11-01
**Refines:** [ADR 0044](./0044-unicode-text-unit-and-presentation-width-policy.md), the right-edge placement of a sequence promoted from one cell to two.

## Context

[Spec 0015](../spec/0015-unicode-terminal-text-correctness.md#unresolved-acceptance-and-discrepancy) records a conflict between its two-cell emoji policy and the former no-wrap final-column behavior. In a four-column terminal, `abc♥` followed by VS16 with DECAWM disabled previously retained `♥️` in one cell. The earlier parser regression asserted that behavior; passing it did not satisfy the two-cell requirement.

The user selected option 2 during refinement: preserve the two-cell width even when this requires an exception to disabled autowrap. The alternatives discussed were retaining the one-cell fallback as an explicit policy exception, or moving left and overwriting preceding content. Neither alternative was selected.

## Decision

When a retained one-cell sequence at the right edge is promoted to two cells and cannot fit there, move the complete sequence to the next line even with DECAWM disabled. Preserve its source text and the two-cell lead/continuation invariant rather than retaining a one-cell projection or moving left over preceding content.

This is a scoped placement exception for sequence promotion, not permission to enable DECAWM globally or change ordinary no-wrap printing. It refines ADR 0044 without replacing its width, source-retention, anchor or presentation-fallback policy. Existing text-unit admission limits remain applicable.

## Consequences

- Disabled autowrap no longer promises that this particular sequence-promotion operation stays on the current line.
- Implementation must preserve the sequence's style, hyperlink and content identity under the existing shared text/anchor contract; merely setting a width field without a continuation cell is not sufficient.
- The old one-cell regression is replaced by byte-fragmentation, cursor/mode, metadata/anchor, damage, copy/reflow, margin, insert-mode and buffer/scroll tests. Oversized rejected extensions still do not promote or wrap.
- The explicit promotion wrap is implemented in `CellWriter::promote_unit` without changing DECAWM. Focused headless and terminal-crate checks pass; integrated Windows visual/application acceptance remains pending. `Testing` is not a blanket Unicode or ConPTY compatibility claim.
