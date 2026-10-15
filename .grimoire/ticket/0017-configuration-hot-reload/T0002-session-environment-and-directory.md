# Apply Environment and Initial Directory to New Sessions

**Ticket ID:** T0002
**Source:** [Spec 0024 R2–R4 and E1–E7/E12](../../spec/0024-watched-reload-and-launch-settings.md), [ADR 0062](../../adr/0062-watched-reload-and-new-session-launch-settings.md).
**Status:** Todo

## Goal

A user configures environment overrides and initial directory; the first session and sessions created after a valid manual reload receive them, while existing children and rejected candidates remain unchanged.

## Affected Surfaces

`harbor-config` launch settings/diagnostics, application acceptance and tab factories, `ShellCommand`, Windows environment block and CreateProcess boundary, native launch tests, configuration examples and user documentation.

## Approach

Carry validated launch settings through the existing acceptance/factory path to per-child native launch inputs. Keep environment and directory together because they share validation, publication and child-creation acceptance. Retain native shell fallback order without allowing directory failure to become a launch in an unintended directory. Do not mutate global environment or cwd.

## Dependencies and Coordination

- **Blocked by:** No local ticket; uses existing manual transaction baseline.
- **Blocks:** T0004 combined all-field acceptance.
- Can proceed alongside T0003. Coordinate shared acceptance/feedback code and test fixtures; watcher integration is tested in T0004.

## Acceptance

- [ ] One documented TOML schema supplies literal string environment overrides and optional directory to startup and manual reload.
- [ ] Inheritance, overriding, empty values, override removal, Windows case-insensitive collisions and protected TERM match R2; no variable interpolation or inherited-variable deletion syntax.
- [ ] Absolute/config-relative/omitted directory semantics match R3, independently of Harbor's launch location, active terminal cwd or OSC 7.
- [ ] Invalid environment/directory candidates reject all live and factory changes; resource/resize failure also prevents launch-policy publication.
- [ ] Real new Windows children report expected synthetic environment values and cwd, including the selected fallback shell path where exercised; prior children retain identity and their environment/cwd.
- [ ] A directory lost after acceptance produces an explicit failed new-session attempt without fallback-directory launch or damage to existing sessions; subsequent valid creation works.
- [ ] UI/log/error chains do not reveal configured environment values, including malformed-input diagnostics.
- [ ] E1–E7/E12 results, startup fallback regressions, accurate new-session-only feedback and examples are recorded with applicable quality gates and runtime evidence.

## Out of Scope

Watcher implementation, changing existing child environment/cwd, process-global mutation, shell-expression expansion, profiles, OSC 7 side effects or unrelated executable-resolution redesign.
