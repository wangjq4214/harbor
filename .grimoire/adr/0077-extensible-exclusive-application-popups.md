# Extensible Exclusive Application Popups

**Status:** Testing
**Date:** 2026-10-22
**Sources:** User request to keep at most one command/search popup open, centrally manage future popup types without concrete-kind enumeration; approval on 2026-10-22 of replacement/cleanup/focus behavior, clearing displaced search, notification and confirmation-window exclusions, and the spec-only refinement endpoint.
**Related:** [ADR 0064](./0064-widget-and-application-component-ownership.md), [ADR 0069](./0069-command-palette-source-aware-availability-and-focus.md).

## Context

The workspace independently retains search visibility and optional command-palette state, and terminal-panel composition can render both. The user expects further popup types and rejects managing exclusivity by enumerating the current popup kinds.

## Decision

Centrally manage exclusivity for the command palette, search panel and future popups participating in the same application-popup contract: at most one participating popup may be open at a time. The management contract must support extension without a central enumeration of concrete popup kinds or pairwise special-case exclusion between them.

Opening another participating popup closes the old one through its lifecycle cleanup and gives focus to the new popup. There is no waiting queue, suspended-popup stack or automatic restoration of the displaced popup. Displacing search is a real close: clear its query and results rather than preserve a hidden search for later reopening.

Notifications and independent native paste-confirmation windows do not participate in this exclusivity contract; preserve their existing visibility, focus and cross-window input-gating behavior.

**Refinement clarification (2026-10-22):** The user's approval settles the initially open membership, replacement and search-retention questions. The central exclusivity/extension decision is unchanged.

## Consequences

- Adding another participating popup must not require adding it to a closed list of command-palette/search variants or updating every existing popup's exclusion logic.
- The exact extension API, identifiers/handles, storage representation and module/type names remain implementation choices. Existing notification timing and independent confirmation-window security/focus policy are not redesigned.
- Existing ownership, command execution and focus constraints remain relevant. This records the settled architectural constraint only, not implementation or acceptance.
