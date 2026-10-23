# Command Palette Source-Aware Availability and Focus

**Status:** Proposed
**Date:** 2026-10-18
**Sources:** [Issue #156](https://github.com/wangjq4214/harbor/issues/156); the user's approval of the proposed first-delivery palette direction and Spec → Slice endpoint on 2026-10-18.
**Related:** [ADR 0040](./0040-application-command-dispatch-and-keybinding-precedence.md), [ADR 0029](./0029-widget-store-state-action-boundary.md), [ADR 0064](./0064-widget-and-application-component-ownership.md), [ADR 0065](./0065-layered-widget-text-input.md).
**Superseded by:** [ADR 0078](./0078-command-palette-disabled-items-without-reasons.md), only the reason-display requirement in decision 2; all other decisions remain applicable.

## Context

The command registry and shared typed execution path already exist. Shortcut routing deliberately permits some commands to pass input through: copy-or-interrupt without selection and primary-buffer scrolling in an alternate screen. A palette introduces a different source: an Enter used to select an unavailable action must not become terminal input. Execution can also change the appropriate focus target, including opening search, activating a new tab or opening paste confirmation.

## Decision

1. Keep the palette application-owned and reuse the existing command registry, typed dispatcher and Host execution. Reuse the widget text editor rather than add a palette-specific editor. These apply the existing ownership decisions, not a second command system.
2. Share command availability facts across palette presentation and command invocation, while retaining source-specific routing semantics. A palette-visible but currently unavailable command is disabled with a reason and is rechecked before execution. Palette rejection never falls back to sending its confirmation/navigation input to the PTY. Existing shortcut pass-through behavior remains unchanged; copy-or-interrupt remains palette-hidden.
3. Closing/cancelling the palette restores terminal focus and eligible terminal IME behavior. Executing an action respects that action's resulting focus destination rather than unconditionally refocusing the previously active terminal. Search enters the search field, tab creation/activation enters the resulting terminal, and paste confirmation retains its existing cross-window input gate and focus policy.
4. Palette query edits, navigation, IME confirmation/cancellation and execution input must remain separate from terminal input. IME composition uses the reusable editor contract; confirming a candidate must not also execute an action.

## Consequences

- Availability is not equivalent to a shortcut's `Consumed`/`PassThrough` result; presentation, invocation revalidation and input routing must agree without conflating these concepts.
- Palette commands do not bypass terminal clipboard authorization, confirmation windows or Host-owned effects.
- Tests must observe actual queue/execution counts, focus destinations and PTY writes, with Windows runtime evidence for native focus/IME behavior.
- This records approved constraints, not implementation or acceptance. Opening chord, matching rules, first-delivery command scope and detailed verification belong in the requirements contract, not a second architectural policy record.
