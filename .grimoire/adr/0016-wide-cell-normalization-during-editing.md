# Normalize Wide Cells at Editing Boundaries

**Status:** Completed
**Date:** 2026-08-05

## Context

Terminal editing operations such as erase, insert, delete, line movement, scrolling, and DEC rectangular operations can mutate only one half of a wide glyph and leave an orphan continuation cell. Alternatives were to fix each operation independently, defer normalization to rendering, or enforce a shared screen-editing invariant.

## Decision

Define a shared wide-cell normalization rule in the screen editing layer: when an editing operation touches either half of a wide glyph, it cleans the complete glyph unless the operation explicitly moves or copies the complete glyph. Apply this rule across all operations in issue #71, while preserving active margins, protection semantics, erase attributes, and damage tracking.

Place the shared boundary and normalization helpers in `VtEditEngine` within `screen/edit.rs`, rather than in `Cell` or the renderer. Attribute-only rectangular operations extend a partial wide-glyph selection to both cells; rectangular copy operations copy only complete source glyphs and normalize any truncated destination. Use a shared row invariant test helper, and update the protocol checklist and roadmap after implementation and verification.

## Consequences

- Every listed editing path must converge on the same cell invariant and cannot leave an orphan continuation cell.
- Boundary handling is centralized instead of duplicating wide-cell checks in each operation.
- Insert, delete, line, scroll, and rectangle operations must remain bounded by their active margins.
- Renderer logic does not need to repair malformed screen state.
- Focused tests are required for operations touching either half of a wide glyph, including protected cells and margin boundaries.
