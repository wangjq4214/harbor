# Reload Automatically Across Saves and File Replacement

**Ticket ID:** T0003
**Source:** [Spec 0024 R4/R5, E8/E9/E11](../../spec/0024-watched-reload-and-launch-settings.md), [ADR 0062](../../adr/0062-watched-reload-and-new-session-launch-settings.md).
**Status:** Todo

## Goal

With watching enabled by default, ordinary saves, editor atomic replacements and save bursts reach the same strict transaction as manual reload; invalid edits and watcher faults remain recoverable and visible.

## Affected Surfaces

Application file-observation lifecycle, event routing, coordinator transaction invocation, UI/log feedback, filesystem/event-order tests and user documentation. Do not extend widget-library HMR.

## Approach

Treat file observation as a source of reload requests, not a configuration owner. Coalesce saves and maintain coherent request/acceptance ordering through the established application thread. Keep manual invocation working independently. Choose and document watcher backend/timing without inventing a product latency guarantee.

## Dependencies and Coordination

- **Blocked by:** No local ticket; uses existing manual transaction baseline.
- **Blocks:** T0004 combined acceptance.
- T0002 is parallel with shared-file coordination. Verify initial watcher behavior using existing colors/bindings/fonts; T0004 covers environment/directory integration.

## Acceptance

- [ ] Watching is active by default without widget-HMR or palette prerequisites.
- [ ] Real Windows saves, replacement/rename saves, subsequent saves and bursts ending in valid content apply final stable settings, without detaching observation.
- [ ] Intermediate missing/malformed files never publish defaults; a stable invalid file reports failure and retains the prior accepted state; later correction succeeds.
- [ ] Repeated identical failures avoid notification flooding without hiding changed errors or recovery.
- [ ] Manual/save interleaving does not let stale work overwrite a later accepted configuration; current applicable sessions and future factories agree.
- [ ] Watcher setup/runtime failures are visible and manual reload remains usable.
- [ ] No PTY restart, session recreation, second acceptance policy or value-leaking diagnostics are introduced.
- [ ] E8/E9/E11 and interleaving with existing live fields have controlled and real-filesystem evidence; applicable gates and limitations are recorded.

## Out of Scope

New launch-field implementation, changing startup fallback, widget HMR, command palette, arbitrary latency promises or a new configuration ownership architecture.
