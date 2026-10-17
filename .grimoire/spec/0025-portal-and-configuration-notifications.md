# Portal and Quiet Configuration Notifications

**Spec ID:** 0025
**Status:** Implementation exists; native interaction acceptance is not established by source review.
**Sources:** [ADR 0063](../adr/0063-root-composable-escape-and-quiet-configuration-notices.md), [0023](./0023-atomic-manual-configuration-reload.md), [0024](./0024-watched-reload-and-launch-settings.md). Supersedes only their routine-success/absent-file user notices; strict acceptance, OSC, session identity, restart clipboard policy, confidentiality and compensation-failure reporting remain.

## Requirements
### R1 — Reusable Root-Level Escape Capability
Provide a general declarative Portal, directly root-composable or nested: content renders in a root/window-bounded overlay outside declaring-parent allocation/clipping, not a separate native window. It accepts arbitrary component content, not a configuration-specific drawing branch or full React API.
Adding/updating/removing content must consume no normal-layout space or change terminal allocation/grid/PTY size. Removing content/owner removes stale paint and interactive regions; remount works.

### R2 — Notification Component and Application Integration
Compose Portal-backed reusable notification presentation at the main-window root: floating top-right of current application content bounds above workspace. Remove bottom configuration TextLabel and reserved/duplicate row. Placement/hit testing must follow resize/DPI. Exact inset/size/color/radius/icons/animation are not fixed. Presentation receives content; application policy decides eligibility.

### R3 — Non-Disruptive Interaction
Manual dismissal, non-modal, no terminal-focus theft, input gate or interference with ordinary terminal/tab interaction outside the notice. Empty overlay space must not block input; controls hit-test where painted, not at old tree coordinates.
Actionable errors never auto-expire. Dismissal affects visibility, not accepted state/truth. Identical watcher failures cannot immediately resurrect dismissed notices; distinguish different failures and recurrence after recovery. No selected auto-dismiss interval/stack capacity or required auto-dismiss behavior.

### R4 — Selective Configuration Notification Policy
Apply consistently at startup and manual/watched reload:
| Outcome | Presentation and effective state |
| --- | --- |
| Absent startup file | Silent; established defaults |
| Absent live candidate | Silent; retain accepted state, no defaults/false acceptance |
| Ordinary success/no change | Silent; valid transaction still applies |
| Ordinary automatic recovery/system fallback needing no awareness | Silent; retain established recovery |
| Requested settings unapplied due to error | Actionable top-right cause/effective-state notice; live rejection retains accepted state |
| Restart-required change | Notice explicitly says pending restart, not already effective |
| Observation unavailable | Notice explains limitation/recovery/manual reload; manual remains usable |
| Unrecovered resource/operation failure | Persistent actionable error with affected operations/sessions; no false success; existing reader/compensation obligations |
Missing is not permission denial, malformed TOML or unresolved path; absence suppression must not hide other errors. Silence cannot accept invalid live candidates. Actionable ignored-setting/font-selection warnings remain reportable. A successful transaction with independent restart/actionable warning is not ordinary success-only; no appended success feed. Whenever reported, distinguish live/new-session/restart effects; ordinary new-session acceptance alone needs no toast.
Suppress identical-problem floods without hiding different failures or recovery. Clear/update obsolete failure as current state without a recovery success toast.

### R5 — Preserve Policy and Ownership Boundaries
Generic escape/presentation owns no TOML validation, acceptance, terminal state or watcher recovery. Application classifies notices and owns dismissal intent under [Host state/one-shot FIFO actions](../adr/0029-widget-store-state-action-boundary.md), existing event-turn effect ordering and runtime invalidation. No new global bus/state container.
Feedback identifies available config path/parse location/setting/failed operation and retained effective state on rejection. Never inject into PTY or disclose environment values in UI/log/error chains. UI silence does not require discarding diagnostic logs.

## End-to-End Acceptance
| ID | Scenario → required outcome |
| --- | --- |
| E1 | Root Portal and arbitrary content under small clipping parent → root overlay, no parent clipping/sibling/terminal allocation change. |
| E2 | Update/remove owner/remount → correct content, no stale paint/hit regions, normal remount. |
| E3 | Show/update/dismiss, resize and change scale → top-right paint/hit agreement; no bottom row or notice-induced terminal/grid/PTY resize. |
| E4 | Notice appears while terminal typing; outside click and dismiss control → no focus theft, input block or empty-overlay interception; visible control works. |
| E5 | Absent startup then absent manual/watched file after non-default acceptance → silent defaults only at startup; accepted live/factory state unchanged, no false acceptance. |
| E6 | Manual/watch success, no-change and ordinary recoverable fallback → no success/recovery/bottom feed; valid changes/recovery still happen. |
| E7 | Invalid TOML/recognized field or permission denial mixed with valid edits → actionable truthful top-right error; no partial/default installation or unrelated absence suppression. |
| E8 | Restart change versus ordinary new-session-only change → explicit pending restart versus silent routine success. |
| E9 | Watch failure/repeated invalid edit, dismiss/repeat/change/recover → no flooding/resurrection, different errors visible, obsolete failure cleared, manual works. |
| E10 | Resource failure or resize-compensation double failure; wait → persistent truthful error, accepted state/affected sessions and resume/no-false-success obligations. |
| E11 | Actionable ignored setting and synthetic env-value diagnostics → necessary notice, no secret disclosure or PTY bytes. |

## Verification and Limits
Use [validation policy](../../docs/validation.md). [Portal tests](../../crates/harbor-widget/src/runtime/portal_tests.rs), [workspace tests](../../crates/harbor-app/src/workspace_tests.rs) and [startup/manual/watch tests](../../src/configuration_notice_tests.rs) are source/test evidence, not a new execution pass. Verify clipping escape, displayed-position hit testing, removal and layout invariance with arbitrary content, plus real startup/manual/watch paths rather than classifier-only checks. Native focus/appearance/resize/DPI checks must explicitly distinguish executed from NOT RUN; screenshots of config text alone prove no general escape component.
No converted paste/clipboard confirmation windows/security gates, changed acceptance/watcher ownership, global bus, prescribed styling/timeout/capacity or separate notification window.
