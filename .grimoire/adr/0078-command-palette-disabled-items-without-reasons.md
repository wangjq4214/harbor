# Command Palette Disabled Items Without Reasons

**Status:** Proposed
**Date:** 2026-10-23
**Sources:** The user's command-palette refinement: unavailable commands must be distinguishable from available commands, followed by the explicit correction “do not display reasons”.
**Supersedes:** [ADR 0069](./0069-command-palette-source-aware-availability-and-focus.md), only the requirement in decision 2 to display an unavailability reason. Its other availability, execution, input-routing and focus decisions remain applicable.

## Context

ADR 0069 and spec 0030 R4 require unavailable palette entries to be visibly disabled with a reason. The current refinement rejects displaying those reasons. This changes presentation policy, not the shared availability or execution contracts.

## Decision

Unavailable palette entries remain visibly disabled, but do not display an unavailability reason. Do not replace the removed reason text with a reason tooltip on hover. The disabled state itself must be recognizable without hovering.

Keep authoritative availability revalidation, rejection without PTY input leakage, source-specific shortcut semantics and command-result focus behavior unchanged. This decision does not remove execution-failure feedback or change underlying command availability.

## Consequences

- The reason-display clauses in spec 0030 R4/E4 and related older artifacts are superseded by this decision; they must not be used to require reason text or a reason tooltip in the palette.
- This record does not select new keyboard-navigation behavior or exact colors/opacity values.
- This records the user's presentation constraint, not implementation or runtime acceptance. No spec/ticket rewrite or code execution is included in this refinement turn.
