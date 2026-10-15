# Atomic Manual Reload and Terminal-Only Fonts

**Status:** Proposed
**Date:** 2026-10-14
**Sources:** [Issue #172](https://github.com/wangjq4214/harbor/issues/172) and user clarification during its refinement: terminal font changes must apply without restarting the application, must not affect external UI fonts, and must not lose terminal content; other recommended reload policies were accepted.
**Supersedes in part:** [ADR 0036](./0036-preserve-harbor-alpha-for-osc-default-colors.md) and [ADR 0053](./0053-bounded-session-osc-palette-policy.md), only their immutable startup reset baseline after a successful configuration reload. Other OSC wire, isolation, reset, and alpha-preserving set rules remain applicable.
**Superseded in part by:** [ADR 0060](./0060-font-reload-uses-normal-bounded-resize.md), clarifying that font-driven grid changes use ordinary bounded resize retention rather than preserving every old history row or out-of-bounds cell. No restart, session recreation, or buffer reset is permitted; other decisions remain applicable.
**Superseded in part by:** [ADR 0061](./0061-manual-reload-compensation-failure.md), permitting an explicitly reported recovery-failed result only when a compensating native PTY resize itself fails; partial configuration acceptance remains prohibited.

## Context

Harbor's startup configuration intentionally recovers from malformed settings through documented field, palette, and keybinding fallback. Reusing that recovery as live-reload acceptance would silently replace previously accepted settings with defaults. The existing command dispatcher provides the application-owned invocation boundary; widget-library HMR is a separate development capability.

The original issue excludes font/resource transitions. The user explicitly expanded the discussed scope to terminal font family and size, rejecting an application restart as their application policy. `harbor-text` already distinguishes configured terminal font loading from independent system UI font loading. The GPU-independent terminal boundary also separates retained terminal content and PTY ownership from replaceable rendering resources.

OSC default colors and indexed colors currently reset to startup configuration. Reload requires a choice between clearing application overrides and preserving them above an updated configuration baseline. The user accepted preservation of OSC RGB overrides and resets to the latest accepted baseline.

## Decision

Manual configuration reload uses the existing application command registry and dispatcher. Parse and validate the complete candidate, including application-owned keybinding semantics, before committing the supported changes on the application thread. Startup fallback remains a separate policy. Malformed or unreadable candidates, invalid recognized fields, invalid bindings, and failed resource preparation reject the reload rather than partially applying settings or substituting startup defaults. Unknown ordinary fields retain visible warning behavior.

Applicable existing sessions, including inactive and alternate-screen sessions, and future-instance factories must observe a coherent accepted configuration. Replacing font/render resources must not restart the application or existing PTYs, recreate terminal sessions, clear terminal buffers, or lose retained terminal content. Terminal font family and size apply only to terminal rendering and its associated geometry; application chrome, widgets, and independent dialog UI retain their own font policy. This preserves the analogous terminal-theme/application-appearance separation in [ADR 0058](./0058-terminal-themes-separate-from-application-appearance.md).

Configuration reload changes each session's palette baseline while preserving its active OSC RGB overrides. This applies consistently to OSC 10/11/12 and OSC 4 indexed entries. Alpha follows the latest accepted configuration baseline, not a retained OSC RGBA copy. Explicit OSC overrides remain overrides even when their RGB equals the baseline. OSC 110/111/112 and OSC 104 clear the relevant overrides and restore the complete latest accepted baseline RGBA. Before any successful reload, that baseline is the startup palette. Newly created sessions start from the latest accepted baseline without inheriting another session's overrides. Failed reloads do not advance the baseline or clear overrides.

Changing terminal colors or fonts does not select application appearance, recolor application chrome, or change the independent whole-window compositor tint. Existing explicit truecolor terminal content is not rewritten as theme colors.

Shell program and arguments accepted through reload affect newly created sessions only; running processes retain their launch configuration. Clipboard policy is not live-updated in this discussed scope and requires an explicit restart-required report. Consumed application/reload chords remain consumed even if execution fails or the accepted binding table changes.

## Consequences

- Live acceptance needs a strict candidate/resource-preparation boundary, not reuse of startup fallback as a successful result.
- Terminal font resources and geometry may change while terminal/session identity and retained content survive; the widget font path must remain independent.
- Per-renderer font replacement and per-session resize primitives do not by themselves prove application-wide atomicity or content preservation.
- OSC reset-baseline changes require explicit override tracking and consistent default/indexed palette behavior, without expanding supported OSC syntax.
- This is an approved policy, not an implementation or runtime acceptance claim. The expanded font scope has not been written back to the GitHub issue, and concrete resize/content-preservation mechanics are not selected by this record.
