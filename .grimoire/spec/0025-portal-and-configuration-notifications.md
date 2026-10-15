# Portal and Quiet Configuration Notifications

**Spec ID:** 0025
**Status:** Draft
**Date:** 2026-10-15
**Sources:** The conversation contract below; [ADR 0063](../adr/0063-root-composable-escape-and-quiet-configuration-notices.md); the existing configuration contracts in [spec 0023](./0023-atomic-manual-configuration-reload.md) and [spec 0024](./0024-watched-reload-and-launch-settings.md).

## Authority and Conversation Contract

- **C1 — Original request:** Define a reusable escape component composable at the root; build a notification component using it; show necessary configuration notices as top-right floating UI; remove the bottom notice. An absent configuration file and silently resolvable conditions must not notify.
- **C2 — Approved meaning:** The user accepted Portal semantics: content declared in the component tree is presented in a root-level overlay, outside the original parent's layout and clipping, without consuming terminal space.
- **C3 — Approved classification and interaction:** The user accepted silent missing-file, successful/no-change reload and ordinary automatic recovery/system-fallback cases; actionable notification for unapplied configuration changes, restart requirements, unavailable observation and unrecovered failures; no repeated identical-message flooding. Notifications are manually dismissible, do not steal terminal focus, and errors requiring action do not automatically disappear.
- **C4 — Selected endpoint:** The user accepted a single spec, without ticket decomposition or implementation in this refinement.

This spec changes presentation and notification eligibility, not configuration acceptance. It takes precedence over spec 0023 R7/E1/E2 and spec 0024 R4/R5 only where they would require a routine success or absent-file user-facing notice. Missing live candidates still do not commit defaults. Preserve strict complete-candidate rejection, coherent publication, existing session identity, OSC policy, restart-required clipboard policy, confidentiality and explicit compensation-failure reporting. [ADR 0059](../adr/0059-atomic-manual-reload-and-terminal-only-fonts.md) links the limited notification-policy supersession in both directions; [ADR 0062](../adr/0062-watched-reload-and-new-session-launch-settings.md) still requires actionable watcher-failure feedback and duplicate suppression.

## Requirements

### R1 — Reusable Root-Level Escape Capability

Provide a general Portal-like component usable in the declarative component tree, including direct composition at the application root. It is not a configuration-specific drawing branch.

Content is declared at its component-tree location but displayed in a root-level overlay. Its original parent's layout allocation and clipping must not confine the escaped presentation. Root/window bounds remain the presentation boundary; this is not a separate native notification window.

Adding, updating or removing Portal content must not consume space in the normal layout. In particular, notification visibility must not shrink the terminal allocation, change its grid or trigger a PTY resize merely to make room for a notice. Removing content or its owner must remove the corresponding overlay rather than leave stale visible or interactive content.

The capability must be reusable for arbitrary component content rather than only notification text. C2 defines the required escape behavior; it does not require compatibility with the complete React Portal API.

### R2 — Notification Component and Application Integration

Build the notification component using R1 and compose its presentation at the main-window root. Display configuration notices as floating, non-modal content anchored at the top right of the application content area, above ordinary workspace content rather than inside the terminal's vertical layout.

Remove the existing bottom configuration-result `TextLabel` presentation. Do not retain a duplicate bottom message or reserve a blank result row. The notification component consumes presentation content; application configuration policy determines whether an outcome deserves notification.

Notification placement must remain top-right relative to the current root size after resizing or scale changes. Exact inset, width, colors, corner radius, iconography and animation are not fixed by this contract.

### R3 — Non-Disruptive Interaction

Notifications can be manually dismissed. Showing or updating one must not steal terminal focus, open a modal input gate or prevent ordinary terminal input and tab interaction outside the notice. Empty overlay space must not behave as a full-window input blocker. Notice controls must be hit-testable at their displayed location, not their former component-tree layout position.

Errors requiring action do not automatically disappear. Dismissing a notice changes its visibility, not the accepted configuration or the truth of the reported outcome. Repeated identical failures must not immediately recreate a dismissed notice merely because another identical watcher event arrived; existing duplicate-suppression semantics continue to distinguish a different failure or a new failure after recovery.

No numeric auto-dismiss interval or notification-stack capacity is selected. This spec does not require auto-dismiss for any class.

### R4 — Selective Configuration Notification Policy

Apply the policy to startup, manual reload and watched reload as applicable, rather than moving every existing result string into the overlay.

| Outcome | Required user-facing behavior | Required configuration behavior |
| --- | --- | --- |
| Configuration file absent at startup | No notice | Start with the established defaults |
| Configuration file absent during manual or watched reload | No notice | Retain the last accepted configuration; do not publish defaults or claim acceptance |
| Ordinary successful reload or no effective change | No success/no-change notice | Apply valid changes under the existing transaction; preserve truthful diagnostics |
| Ordinary automatic recovery or system fallback that requires no user awareness | No notice | Retain the established recovery/fallback behavior |
| Configuration error leaves the requested changes unapplied | Actionable top-right notice | Explain the cause and actual effective state; reload rejection retains the accepted configuration |
| A changed setting requires restart | Top-right notice stating the restart requirement | Do not imply the setting is already effective |
| Configuration observation unavailable | Top-right notice explaining the limitation and available recovery/manual reload | Do not pretend observation is working; preserve manual reload |
| Unrecovered resource/operation failure | Actionable top-right error, identifying affected operations/sessions when relevant | Preserve existing rejection and compensation-failure obligations; never claim success |

Silence for ordinary fallback does not authorize silently accepting invalid live candidates. Existing explicit ignored-setting and font-selection warnings remain subject to the actionable/unapplied-request boundary, not an indiscriminate suppression of every warning. Likewise, file absence is distinct from permission denial, malformed TOML or inability to resolve the configuration path: the missing-file exception must not swallow unrelated actionable errors.

A successful reload with an independent restart requirement or actionable ignored-setting warning is not an ordinary success-only outcome; retain the necessary notice without appending a routine success feed. Accurate distinctions between live, new-session-only and restart-required settings still apply whenever those distinctions are reported; normal new-session-only changes do not by themselves require a success notification.

Repeated identical problems must not flood the UI. Recovery must not leave an obsolete failure represented as the current state; clearing or updating a stale failure does not require a separate success toast. This retains spec 0024 R5's distinction between recovery, a different failure and repeated identical events.

### R5 — Preserve Policy and Ownership Boundaries

The reusable escape/presentation capability must not own TOML validation, configuration acceptance, terminal state or watcher recovery. Keep configuration classification with application policy and preserve the existing Host-published UI state / one-shot action boundary in [ADR 0029](../adr/0029-widget-store-state-action-boundary.md).

Dismissal is presentation intent, not a configuration command or a stored false success outcome. Preserve existing event-turn effects/action ordering and runtime invalidation when feedback changes. This spec does not select a new state container or global notification bus.

Required notices must remain truthful and actionable: identify the relevant configuration path, parse location, setting or failed operation where available; state retained effective configuration on reload failure. Feedback remains application UI, never text injected into the PTY. Preserve spec 0024's prohibition on revealing environment-variable values in UI, logs or error chains. User-facing silence does not require discarding existing diagnostic logging.

## Solution and Seams

Compose a general root-level escape capability with a separate notification component. Route application-classified configuration outcomes to that component instead of the bottom terminal-panel label. This is the selected solution boundary, not an internal rendering or implementation plan.

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Escape presentation | Declarative content/owner → widget root overlay | Arbitrary component content and its mounted lifetime | Root-level visual and hit-test placement outside original-parent layout/clipping, with no normal-layout reservation and no stale overlay after removal |
| Notification presentation | Application presentation state → notification component → Portal | Eligible message, relevant outcome meaning and dismissal intent | Top-right non-modal notices with R3 interaction and lifetime rules |
| Configuration classification | Startup/reload/watch outcomes → application UI state | Actual typed/error outcomes, accepted-state information, and the distinction between absent and otherwise unreadable files | R4 eligibility and truthful content; no routine-success notification feed |
| Dismissal and publication | Notification controls / Host → declarative state and runtime | One-shot dismissal actions, published presentation state, existing invalidation/effect ordering | Dismissal without accepting settings or stealing terminal focus |
| Terminal allocation | Main-window root / notification layer → terminal workspace | Unchanged ordinary workspace constraints when only visibility changes | Stable terminal allocation/grid, with no notice-induced PTY resize |

Observed integration surfaces are [main-window root and bottom label](../../crates/harbor-app/src/workspace.rs), [Host-published UI state](../../crates/harbor-app/src/tab_view.rs), [startup/reload/watch reporting](../../src/shell.rs), [configuration I/O and candidate validation](../../crates/harbor-config/src/lib.rs), and the [widget component/runtime boundary](../../crates/harbor-widget/src/view.rs). These identify existing responsibilities, not mandated type or method names.

## End-to-End Acceptance

| ID | Given / When | Required outcome |
| --- | --- | --- |
| E1 | Compose arbitrary Portal content directly at the root, then declare escaped content beneath a small clipping parent | Both render at the root overlay as specified; the nested parent's clip does not cut off escaped content; ordinary sibling/terminal allocation is unchanged |
| E2 | Change Portal content, remove its declaring component, then remount it | Visible content updates; removal leaves no stale paint or interactive hit region; remounted content works normally |
| E3 | With a running terminal and an actionable configuration notice, show/update/dismiss the notice, resize the window and change display scale | Notice stays top-right; bottom result row is absent; visibility alone never changes terminal allocation/grid or causes PTY resize; positioning/hit testing agree after resize/scale changes |
| E4 | Terminal has focus; a notification appears while typing; click outside it and exercise its dismiss control | Appearance does not steal focus or block normal terminal/tab interaction; blank overlay space does not intercept input; the visible dismiss control works at its rendered position |
| E5 | Start without a configuration file; separately remove the file after accepting non-default live settings and invoke manual and watched reload | No missing-file notice in any case; startup uses defaults; live settings/factories remain on the accepted configuration, without a false successful acceptance |
| E6 | Manually reload, automatically reload, repeat a no-change reload, and exercise an ordinary silently recoverable condition | No routine success/no-change/recovery notice and no bottom message; valid settings and established recovery behavior still take effect |
| E7 | Introduce invalid TOML, an invalid recognized setting or a permission-denied read, including a candidate with otherwise valid changes | Actionable top-right error identifies the cause/effective state; no valid subset/default fallback is installed by a rejected live reload; missing-file suppression does not hide these errors |
| E8 | Change a restart-required setting; separately reload an ordinary valid new-session-only setting | Restart case explicitly states the pending effect rather than claiming live application; routine new-session-only acceptance does not generate a success notice |
| E9 | Trigger watcher failure and repeated identical invalid edits; dismiss a notice; repeat the same failure, then change the failure and later recover | No identical-message flood or immediate resurrection on repeated identical events; different failure remains reportable; obsolete failure is not represented as current after recovery; manual reload remains available during watcher failure |
| E10 | Trigger a resource failure or the established resize-compensation double-failure path and allow time to pass | Required error does not auto-disappear; retained accepted state and affected/unrecovered sessions are honestly reported; existing reader-resume and no-false-success guarantees remain intact |
| E11 | Produce actionable ignored-setting feedback and diagnostics involving a synthetic environment-variable value | Necessary feedback remains visible, no PTY bytes are injected, and UI/log/error-chain output does not expose the value |

Portal-specific verification must include clipping escape, displayed-position hit testing, content removal and layout invariance; a configuration-message screenshot alone does not prove a general escape component. Application verification must exercise startup, manual and watched paths, not only a classifier unit test. Interactive appearance/focus evidence should distinguish executed checks from NOT RUN; this spec is not test execution evidence.

## Decisions and Limits

- **Portal rather than a configuration-only overlay workaround:** C1/C2 and ADR 0063. Component/API spelling and root-overlay implementation remain implementation details; only the defined behavior is required, not full React semantics.
- **Separate presentation from eligibility:** C1/C3 and existing Host ownership in ADR 0029. No new command bus, watcher policy or acceptance owner is selected.
- **Selective rather than universal reporting:** C3 qualifies the older always-visible reload-result contract, with the limited supersession described above. Strict acceptance, required actionable failures, ignored-setting feedback, restart semantics and redaction remain applicable.
- **Manual dismissal, no focus theft, persistent actionable errors:** C3. No animation, numeric timeout, queue limit or visual theme has been assumed.
- **Scope:** One spec for the connected escape/presentation/configuration outcome; no tickets, implementation, product QA or remote issue changes are authorized by this refinement. Existing paste/clipboard confirmation windows and modal security gates are not converted into notifications by this request.

There are no additional product assumptions or unresolved blocking decisions. This draft defines delivery and acceptance, not proof that Portal or the notification component exists or has passed runtime validation.
