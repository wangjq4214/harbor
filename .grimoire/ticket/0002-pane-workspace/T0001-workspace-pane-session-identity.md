# Separate Workspace, Pane and Session Identity

**Ticket ID:** T0001
**Source:** [Spec 0056](../../spec/0056-pane-workspace.md), R1, identity portions of R2/R6–R7, E4/E8–E12; [#158](https://github.com/wangjq4214/harbor/issues/158).
**Status:** Todo

## Goal

Establish explicit workspace/pane/session ownership and routing while the existing single-pane tab remains usable. Several real consumers need this migration before split layout or launch can rely on it.

## Affected Surfaces

- Application tab/session model and Host factories/output/exit events.
- Terminal/bridge/draw identity relationships and lifetime.
- Existing command targets, search/read-model provenance, project capture, clipboard sources, title/cwd metadata and configuration transaction enumeration.

## Approach

Move the one-tab/one-session assumption behind explicit workspace, pane and session relationships, preserving existing terminal-engine session identities where applicable. The concrete storage/ID representation remains an implementation detail. A UI mount/visibility change must not own process destruction.

Do not replace all consumer behavior in a global rewrite: migrate actual source/session bindings and validate the existing single-pane operations. Keep product policy in the Host/application and generic widget/runtime contracts independent.

## Dependencies and Coordination

- **Blocked by:** None within this set.
- **Blocks:** T0002's stable launch source/operation binding and T0003's several-pane geometry/registration.
- **Coordination risks:** Existing consumers use `TabId` for several meanings. Inventory actual events and commands rather than merely renaming the type. Coordinate T0002/T0003 exchange contracts before parallel edits.
- Single-pane migration alone does not deliver the new close defaults or natural-exit policy; those are integrated in T0004.

## Acceptance

- [ ] Workspace membership, pane presentation identity and session lifetime have an explicit documented relationship; no pane position or draw ID is treated as session identity.
- [ ] Existing New tab, activation, whole-tab close and final-window behavior still operate in the migrated single-pane path without restarting surviving sessions.
- [ ] Output/exit wakes, draw callbacks, metadata and existing asynchronous consumers remain source-qualified; delayed events for removed/replaced sessions cannot affect a new one.
- [ ] Failed creation cannot reuse an identity in a way that accepts a stale event.
- [ ] Hiding/unmounting/replacing the UI root retains the intended session; explicit retirement releases its ownership and pending registrations.
- [ ] Search/revision checks, OSC 52 source checks, profile capture and configuration transaction enumeration refer to the actual session rather than the active tab. Expose the explicit identity needed for T0004's new original-session binding for user paste; do not assume pending paste already retains that target.
- [ ] Focused model/Host tests cover source removal, late events, resource-release ownership and single-pane regression. Record performed/omitted verification honestly.

## Out of Scope

Split UI, environment-aware connection/fallback, new shortcut defaults and production multi-pane command admission. No new global bus, crate hierarchy or universal renderer abstraction.
