# Prioritized SGR-Only Mouse Reporting

**Status:** Proposed
**Date:** 2026-09-17

## Context

DEC tracking modes `?1000`, `?1002`, and `?1003` can be enabled together, while Issue #102 limits encoding support to SGR mode `?1006`. Alternatives were last-command-wins tracking, mutually clearing modes, or independent mode state with deterministic priority; when `?1006` is off, Harbor could add a legacy fallback or suppress protocol output.

## Decision

Track `?1000`, `?1002`, and `?1003` independently and select the effective mode by `?1003 > ?1002 > ?1000`, so clearing a higher-priority mode reveals the next enabled mode and DECRQM reports each mode's own state. Emit mouse reports only when `?1006` is enabled; otherwise tracking continues to own and consume pointer input without producing legacy mouse bytes.

## Consequences

- Conflicting tracking modes behave deterministically and retain lower-priority enablement across higher-priority mode resets.
- X10, UTF-8, urxvt, and pixel mouse encodings remain outside the implementation boundary.
- Tests must cover independent mode queries, priority fallback, SGR press/release/motion/modifiers, silent consumption without `?1006`, and RIS clearing all mouse modes.
