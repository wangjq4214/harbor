# Accept the Existing Manual and Font Reload Slice

**Ticket ID:** T0001
**Source:** [Spec 0024 R1/R6](../../spec/0024-watched-reload-and-launch-settings.md), [spec 0023](../../spec/0023-atomic-manual-configuration-reload.md), [#172](https://github.com/wangjq4214/harbor/issues/172), [PR #201](https://github.com/wangjq4214/harbor/pull/201).
**Status:** Todo

## Goal

Establish the actual delivered scope of the existing manual transaction, repair confirmed in-scope failures, and record retrievable Windows acceptance rather than treating source/tests as a pass.

## Affected Surfaces

Existing coordinator/factory, strict config loader, input dispatch, terminal palette/font/resize resources, manual transaction tests, and user/status/evidence documentation. ADR 0059–0061 and spec 0023 remain authoritative.

## Approach

Use the current manual implementation instead of reconstructing the original colors-only slice. Locate prior evidence or produce the missing cases. Verify actual application behavior in addition to failure-injection and parser/model tests. Record scope discrepancies for authorized remote reconciliation.

## Dependencies and Coordination

- **Blocked by:** No local ticket; requires the existing PR #201 transaction code or its successor.
- **Blocks:** T0004 final baseline acceptance.
- Coordinate any fixes in shared files with T0002/T0003; evidence work alone is not their blocker.

## Acceptance

- [ ] Spec 0023 E1–E11 have explicit results, including active/inactive/alternate sessions, OSC reset precedence, consumed chords, startup fallback, new-session shell and restart-required clipboard reporting.
- [ ] Real Windows cmd/PowerShell, nvim, font changes, continued prompt/input, selection/copy and supported IME paths have reproducible results; exclusions remain explicit.
- [ ] Preparation failure, later-session resize failure and failed compensation retain prior accepted settings, resume readers and report honest outcomes; retry is covered.
- [ ] Applicable five quality gates from spec 0024 are executed or marked NOT RUN/BLOCKED with reasons.
- [ ] Evidence records revision/environment, expected/observed behavior and retrievable artifacts. Link only to existing reload evidence using permitted persistence; do not silently change ignore policy.
- [ ] #172's font exclusion, PR scope and local status discrepancies are reconciled through authorized updates, or explicitly reported pending without claiming issue closure.
- [ ] No broad compatibility or #155-complete claim is made from this slice.

## Out of Scope

Implementing watcher or environment/directory support; closing the full package; unrelated runtime features.
