# ADR-0019: Alternate-screen isolation via whole-screen swap with persistent alt buffer

**Status:** Completed
**Date:** 2026-08-14

## Context

Issue #92 completes the alternate-screen mode family (`?47`, `?1047`, `?1048`) on top of the existing `?1049` path. The current `enter_alt`/`exit_alt` recreate a fresh `Screen` on every entry, which is equivalent to always clearing on entry and cannot express `?47`'s "no clear" semantics. Alternatives were: (a) fine-grained buffer-only swap that shares cursor/pen/modes between primary and alternate screens, or (b) whole-screen swap that isolates every engine. A sub-decision was how `?1048` and `?1049` cursor save/restore should be modeled.

## Decision

Adopt a whole-screen swap as the alternate-screen isolation primitive, with a persistent alternate `Screen` slot so `?47` can re-enter without clearing. `?1048` maps to explicit DECSC/DECRC cursor save/restore, while `?1049` relies on the whole-screen swap (a superset of DECSC) for its cursor save/restore. RIS drops the persistent alternate buffer and exits; DECSTR does not switch screens.

## Consequences

- Enables `?47` (no clear), `?1047` (clear on entry), and `?1049` (clear + cursor save) to share one alternate buffer while keeping primary scrollback isolated.
- Avoids the re-entry edge case where an explicit `save_cursor` inside the alternate screen would overwrite the primary's saved snapshot on a repeated `?1049h`.
- Constrains the model to per-buffer cursor/pen/mode state rather than xterm's shared-pen model; documented as the Harbor contract. One observable divergence: `?1048$p` (DECRQM) reports Reset inside a `?1049` alternate screen because the fresh alt screen has no DECSC snapshot, whereas xterm (which defines `?1049` as `?1048`+`?1047`) reports Set there.
- Requires updating `AltScreenAction` to carry a clear flag, adding the persistent alt-buffer slot, and tests for RIS/DECSTR interaction plus each mode's enter/exit semantics.
