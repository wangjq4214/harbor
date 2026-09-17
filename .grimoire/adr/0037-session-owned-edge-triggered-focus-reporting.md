# Session-Owned Edge-Triggered Focus Reporting

**Status:** Proposed
**Date:** 2026-09-17

## Context

DEC private mode `?1004` must translate native window focus into terminal input without duplicate reports, synthetic reports when the mode is enabled, scrollback side effects, or state loss when primary and alternate screens are exchanged. Candidate ownership locations included the buffer-local cursor modes, PTY I/O, and session-owned screen protocol state.

## Decision

Store focus-reporting enablement and the latest observed host focus as session-owned terminal protocol state. Observe focus even while reporting is disabled, emit `CSI I` or `CSI O` only when an enabled event changes that observation, preserve the state across alternate-screen transitions, and reset both fields on RIS. DECSET `?1004` does not emit an immediate synthetic focus report.

## Consequences

- Repeated host events for the same state are suppressed while genuine focus transitions remain visible to applications.
- Disabling and re-enabling reporting does not turn an unchanged host state into a false transition.
- Alternate-screen swaps must explicitly carry focus-reporting state alongside other session-owned state.
- Focus input uses the PTY encoding/write boundary without applying keyboard-specific scroll-to-bottom behavior.
- RIS removes both reporting enablement and de-duplication history, restoring the initial state.
