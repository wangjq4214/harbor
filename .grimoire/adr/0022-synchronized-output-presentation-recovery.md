# Synchronized Output Presentation Recovery

**Status:** Implementing
**Date:** 2026-08-17

## Context

DEC private mode `?2026` requires Harbor to batch terminal output without exposing intermediate frames, yet unmatched boundaries must not permanently freeze a visible terminal. Alternatives included binary mode state versus nesting, terminal-owned timers versus runtime scheduling, and resetting the mode on both hard and soft resets.

## Decision

Use a saturating Terminal/session-owned nesting counter, defer only ordinary presentation, and have the Widget Runtime Frame Scheduler force a present every 100 ms while the counter remains nonzero. RIS and PTY/session close clear the counter, while DECSTR preserves it; DECRQM reports Set for any nonzero count.

## Consequences

- Nested and mismatched boundaries recover without counter underflow or indefinitely suppressing UI updates.
- Terminal parsing and state mutation retain their existing ownership while Runtime retains deadline and presentation ownership.
- The implementation must verify recurring recovery, cancellation after the final disable, and both reset and session-close cleanup.
