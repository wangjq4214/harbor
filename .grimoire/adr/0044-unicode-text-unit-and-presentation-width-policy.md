# Unicode text unit and presentation-width policy

**Status:** Implementing
**Date:** 2026-09-26

## Context

Issue #153 requires ordinary terminal output to preserve combining marks, variation selectors, and ZWJ sequences across rendering, editing, selection, copy, reflow, and damage tracking. The current cell and N01 logical-atom model store one character per glyph; dropping zero-width input or deciding width independently in each subsystem would break the shared text and coordinate contract. The alternative of treating unsupported presentation as lost text is unacceptable.

## Decision

Represent retained text and its cell width with one shared text-unit contract across consumers, compatible with N01 logical-line identities and content anchors. Combining marks attached to a base do not advance an extra cell. An isolated combining mark at a line start is retained as raw text, occupies one cell, and is shown using a synthetic dotted-circle cue; copying emits only the original mark, not the cue. Treat ambiguous-width ordinary characters as one cell and sequences explicitly requesting emoji presentation (for example `♥️` or `👩‍💻`) as two cells. If a font cannot render a complete sequence, preserve the original text and assigned cell width while permitting a documented visual fallback; do not claim correct presentation merely because copy succeeds.

## Consequences

- Parsing across PTY reads, cell edits, selection, copy, reflow, rendering, and dirty-range invalidation must agree on text-unit boundaries and width; no separate scalar-width calculation may silently override the contract.
- The dotted-circle cue is display-only and is never inserted into retained content or copied text.
- Unsupported-font presentation must be identifiable in verification records; text preservation and visual presentation are separate acceptance claims.
- N01 logical-line identity and anchor meaning remain content-based rather than reverting to physical-row coordinates as the representation evolves.
