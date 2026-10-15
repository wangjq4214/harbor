# Root-Composable Escape and Quiet Configuration Notices

**Status:** Proposed
**Date:** 2026-10-15
**Sources:** User refinement request and subsequent explicit approval of the proposed Portal semantics, notification classification, interaction rules, and spec-only route.
**Supersedes in part:** [ADR 0059](./0059-atomic-manual-reload-and-terminal-only-fonts.md), only mandatory user-facing feedback for missing configuration files and routine successful/no-change reloads. Strict candidate acceptance and actionable ignored-setting, failure and restart-required reports remain unchanged.
**Related:** [ADR 0062](./0062-watched-reload-and-new-session-launch-settings.md).

## Context

The main-window UI currently renders `configuration_result` as a `TextLabel` beneath the terminal in `crates/harbor-app/src/ui.rs`. The application composes reload summaries, warnings and failures in `src/shell.rs`. The user rejects the bottom presentation and wants a reusable escape capability rather than a configuration-specific placement workaround.

## Decision

Provide a reusable Portal-like escape component that can be composed at the application root. Content is declared in the component tree but presented in a root-level overlay, outside its original parent's layout and clipping, without consuming terminal space. Build the notification component on this capability rather than a configuration-specific placement workaround.

Configuration notifications use a non-modal floating presentation at the top right, replacing the bottom notice. An absent configuration file must not produce a user-facing notice: startup uses defaults, while a missing file during live reload retains the last accepted configuration. Routine successful/no-change reloads and ordinary automatic recovery/system fallback remain silent. Notify when requested configuration did not take effect, a change requires restart, observation is unavailable, or an unrecovered failure needs attention; repeated identical problems must not flood notifications.

Silence is a presentation policy, not permission to change strict acceptance, install defaults during a rejected live reload, or disguise failure as success. Existing explicit ignored-setting and failure reporting remains actionable under this boundary; ordinary fallback does not mean every configuration mistake is silently repaired.

## Consequences

- Escape capability and notification presentation are separate reusable concepts; configuration-specific reporting is a consumer.
- Removing the bottom notice must not merely relocate every existing diagnostic into a noisy floating feed.
- Portal semantics and the notification policy are confirmed; API spelling, visual styling, and internal overlay machinery are not selected by this decision. Existing strict acceptance, confidentiality, watcher availability and compensation-failure obligations remain applicable.
- This records the settled direction only. It is not a spec, implementation plan, implementation or runtime acceptance claim.
