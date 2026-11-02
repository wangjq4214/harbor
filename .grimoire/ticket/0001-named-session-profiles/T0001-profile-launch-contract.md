# Validated Profile Launch Contract and Source Provenance

**Ticket ID:** T0001
**Source:** [Spec 0052 R1–R3, R6 capture seam; E2–E6/E10](../../spec/0052-named-session-profiles-and-quick-save.md); [ADR 0106](../../adr/0106-independent-session-profiles-and-explicit-osc7-launch-policy.md), [ADR 0108](../../adr/0108-explicit-session-preset-snapshots-and-safe-configuration-writes.md).
**Status:** Todo

## Goal

Make independent named profiles a validated, accepted future-launch policy with safe local OSC 7 directory inheritance and visible failure, retaining the actual source launch facts needed by quick save. Verify child outcomes without requiring a final production chooser.

## Affected Surfaces

Configuration schema/validators and reload acceptance, application profile identity/typed launch intent, Host/factory/session provenance, Windows PTY creation and existing launch tests. Keep existing application/widget ownership and centralized quoting.

## Direction and Boundaries

Reuse existing complete-candidate acceptance and per-child launch machinery. Distinguish named-profile strict failure from legacy `[shell]` executable fallback. Retain session launch program/arguments and initial directory so later profile reloads cannot rewrite source history. OSC 7 interpretation belongs to explicit Host launch/capture policy, not parser side effects.

This is the first concrete consumer of profile policy, not a standalone generic registry, popup framework or global launch refactor.

## Dependencies and Coordination

- **Blocked by:** None within this set; existing launch, OSC 7 metadata and reload foundations are present.
- **Blocks:** T0002 needs the accepted catalog/launch operation; T0003 needs schema/acceptance and source provenance.
- **Coordination:** Publish the shared catalog, validation and source-session contract before downstream integration. Coordinate command/session/factory files; do not freeze incidental signatures in these tickets.

## Acceptance

- [ ] Document and test independent profile fields/identity and consistent concrete configuration spelling; no inheritance from profiles or `[shell]`.
- [ ] Valid profile add/edit/remove publishes coherent future policy; invalid launch data mixed with valid live fields rejects the entire candidate. Existing child identities/env/cwd remain unchanged.
- [ ] Ordinary startup/New tab/Ctrl+T retain legacy `[shell]` behavior. Named executable failure is visible, starts no substitute shell and harms no existing session.
- [ ] Actual child launch tests observe exact empty/spaced/quoted/backslash/Unicode argv and literal environment semantics, including Windows case conflicts, TERM protection, empty/removal and value-free diagnostics.
- [ ] Named directory policy observes explicit > configured profile > usable local OSC 7 > Harbor process directory. Profile-relative directories remain config-relative, and configured-path failure is not bypassed.
- [ ] Test local URI/native conversion, percent encoding/drive form, absent/reset/malformed/nonlocal metadata and higher-priority directories without remote filesystem/network access.
- [ ] Remove an accepted selected directory before spawn: visible failure with old sessions intact; restore it and a later request can succeed.
- [ ] Per-session recorded launch program/arguments and initial directory remain available and unchanged across profile reloads and later terminal command execution; use real selected-launch facts, not merely an executable display title.
- [ ] Focused configuration, Host/PTY and provenance tests accompany delivery. Record exact commands/results and missing native cases under R8; tests alone do not close #157.

## Out of Scope

Production chooser/naming UI, configuration writing, credentials, runtime environment capture, parser-driven chdir, global cwd mutation and changing legacy fallback.
