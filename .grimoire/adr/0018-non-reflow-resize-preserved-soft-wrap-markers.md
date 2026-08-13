# ADR-0018: Documented non-reflow resize with preserved soft-wrap markers

**Status:** Completed
**Date:** 2026-08-13

## Context

Issue #86 requires adding soft-wrap metadata to the screen model and defining deterministic resize/reset behavior for wrapped lines and pending-wrap state. Resize today copies rows without reflow and is generation-stable. The alternatives were: (a) implement full reflow that re-wraps logical lines to the new width, or (b) document non-reflow and keep rows at their pre-resize layout. A sub-decision was whether existing soft-wrap markers survive a resize.

## Decision

Adopt a documented non-reflow policy for issue #86: on resize, rows are copied as-is and their soft-wrap markers are preserved. Full reflow is deferred to a follow-up issue. Soft-wrap markers are stored as per-row `wrapped` flags in `NormalBuf` row metadata, not on `Cell`.

## Consequences

- Enables deterministic, easily tested resize semantics with minimal change to the existing generation-stable resize path.
- Enables soft-wrap evidence to unlock the pending checklist/autowrap conformance items.
- Constrains logical-line geometry to become stale after a width change; selection/copy cannot reconstruct logical lines post-resize until reflow is implemented.
- Requires documenting non-reflow in the roadmap and protocol checklist, tests proving markers survive resize, and a tracked follow-up issue for reflow.
