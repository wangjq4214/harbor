# Buffer-local alternate-screen ownership

**Status:** Completed
**Date:** 2026-10-31
**Supersedes in part:** [ADR 0019](./0019-alternate-screen-buffer-isolation.md), only the whole-Screen swap implementation primitive; per-buffer cursor/pen/modes and observable alternate-screen semantics remain unchanged.

## Context

The interface audit found that swapping complete Screen values requires manually transferring session colors, focus reporting, synchronized-output state and replies at every transition. New session properties could be accidentally parked or restored with a buffer. The user authorized implementing the audit findings, including public API tightening and repository-consumer migration (option A).

## Decision

Screen retains session state and the primary/alternate lifecycle. Active, saved-primary and parked-alternate values are buffer-local states containing storage, cursor/pen, hyperlinks and anchor evidence. Saved and parked buffers are leaves; they cannot carry another screen tree or session properties.

A single Screen transition produces an authoritative lifecycle result, including whether an alternate was restored from parking. PointerInteraction consumes that result to switch its independently owned selections. Screen does not own selections or query the pointer.

Detached preparation follows the same boundary: local prepared buffers carry no session state, while Screen coordinates active/saved/parked preparation and ConPTY's deferred primary geometry. Commit remains an allocation-free ownership transfer.

## Consequences

- Session properties do not need individual transfer logic on buffer switches.
- The ?47 parking, ?1047/?1049 clearing, per-buffer cursor/pen/modes, reply ordering, selection anchors and ConPTY deferred-primary policies remain regression contracts.
- This is not a shared cursor/pen model and does not adopt xterm behavior where Harbor deliberately differs.
- The original whole-Screen primitive is replaced, not its observable isolation contract.
- Current integrated verification and limitations are recorded in the [implementation report](../refactor/terminal-7575ad9-interface-implementation/report.md).
