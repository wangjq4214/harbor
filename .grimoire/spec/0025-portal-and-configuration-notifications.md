# Portal and Quiet Configuration Notifications

**Spec ID:** 0025
**Status:** Accepted (owner-confirmed)
**Sources:** [ADR 0063](../adr/0063-root-composable-escape-and-quiet-configuration-notices.md), [0023](./0023-atomic-manual-configuration-reload.md), [0024](./0024-watched-reload-and-launch-settings.md). Supersedes only their routine-success/absent-file user notices; strict acceptance, OSC, session identity, restart clipboard policy, confidentiality and compensation-failure reporting remain.
**Notification refinement (2026-10-19):** The user selected the existing shared liquid-glass appearance, a larger top-right close control with search-button hover/press feedback, and a three-second timeout for every notice, explicitly including errors. [ADR 0071](../adr/0071-three-second-notifications-including-errors.md) overrides the former persistent-error rule.

## Requirements
### R1 — Reusable Root-Level Escape Capability
Provide a general declarative Portal, directly root-composable or nested: content renders in a root/window-bounded overlay outside declaring-parent allocation/clipping, not a separate native window. It accepts arbitrary component content, not a configuration-specific drawing branch or full React API.
Adding/updating/removing content must consume no normal-layout space or change terminal allocation/grid/PTY size. Removing content/owner removes stale paint and interactive regions; remount works.

### R2 — Notification Component and Application Integration
Compose Portal-backed reusable notification presentation at the main-window root: floating top-right of current application content bounds above workspace. Remove bottom configuration TextLabel and reserved/duplicate row. Placement/hit testing must follow resize/DPI. Exact inset/size/color/radius/icons/animation are not fixed. Presentation receives content; application policy decides eligibility.
Use the application's shared real local glass material (background blur, edge refraction and subtle highlights), not translucency alone. Keep notification text and controls sharp; preserve top-right placement and existing non-modal behavior. The larger right-hand vector close control stays at the top beside the heading, including tall notices, and shares the search close button's transparent background and visible hover/press feedback. Apart from its reserved lane and normal padding, use the full available text width, wrapping by the actual rendered UI-font advances rather than terminal-cell estimates. Show one ellipsized title line above at most four detail lines; ellipsize overflow and visually distinguish title from detail. The application owns source headings and this four-line policy; generic notification callers retain their existing defaults. This follow-up supersedes the earlier vertically-centered close placement; the user's initial left-corner wording was corrected to top-right.

### R3 — Non-Disruptive Interaction
Manual dismissal, non-modal, no terminal-focus theft, input gate or interference with ordinary terminal/tab interaction outside the notice. Empty overlay space must not block input; controls hit-test where painted, not at old tree coordinates.
All Harbor floating notifications, including actionable errors and unrecovered failures, automatically close after three seconds; no severity-based permanent exception. Manual dismissal remains available. Dismissal and expiry affect visibility, not accepted state/truth. Identical watcher failures cannot immediately resurrect dismissed or expired notices; distinguish different failures and recurrence after recovery. Stack capacity remains unselected.

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
| Unrecovered resource/operation failure | Actionable error, auto-closing after three seconds, with affected operations/sessions; no false success; existing reader/compensation obligations |
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
| E10 | Resource failure or resize-compensation double failure; wait → truthful error automatically closes after three seconds even without user input, accepted state/affected sessions and resume/no-false-success obligations. |
| E11 | Actionable ignored setting and synthetic env-value diagnostics → necessary notice, no secret disclosure or PTY bytes. |
| E12 | Short/very long and mixed-width Unicode messages, normal/narrow/fractional-DPI layouts → title stays one line, detail uses the available rendered width outside the close lane, at most four detail lines with overflow ellipsis, visually distinct title/detail, and close remains top-right beside the heading. |

## Verification and Limits
Use validation policy. [Portal tests](../../crates/harbor-widget/src/runtime/portal_tests.rs), [workspace tests](../../crates/harbor-app/src/workspace_tests.rs) and [startup/manual/watch tests](../../src/configuration_notice_tests.rs) are source/test evidence, not a new execution pass. Verify clipping escape, displayed-position hit testing, removal and layout invariance with arbitrary content, plus real startup/manual/watch paths rather than classifier-only checks. Native focus/appearance/resize/DPI checks must explicitly distinguish executed from NOT RUN; screenshots of config text alone prove no general escape component.
No converted paste/clipboard confirmation windows/security gates, changed acceptance/watcher ownership, global bus, stack-capacity requirement or separate notification window.
