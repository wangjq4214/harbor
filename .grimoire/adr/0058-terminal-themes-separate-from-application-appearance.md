# Terminal Themes Separate from Application Appearance

**Status:** Proposed
**Date:** 2026-10-14
**Sources:** User clarification during default Catppuccin Mocha refinement: themes should apply only to the terminal, while the overall application should have only day and night modes.

## Context

The default terminal-theme request exposed a scope ambiguity: the assistant proposed applying the same palette to terminal content and application controls. The user rejected that coupling. Terminal colors already have a separate configurable palette in `crates/harbor-config/src/color.rs`, while generic controls have style tokens in `crates/harbor-widget/src/theme.rs`.

## Decision

Terminal themes govern terminal appearance only. Application appearance is a separate day/night (light/dark) mode, not a terminal-theme selection. Application chrome, including the tab rail and buttons, must not adopt a terminal theme's palette merely because that theme is selected.

This separation applies to named terminal themes such as Catppuccin Mocha. It does not select application-mode defaults, system-mode following, switching controls, or persistence behavior; those choices remain unspecified.

The existing whole-window compositor backdrop policy in [ADR 0026](./0026-unified-window-backdrop-tint-chain.md) is distinct from the terminal background layer. This decision does not replace that compositor policy or the session-owned palette/reset contract in [ADR 0053](./0053-bounded-session-osc-palette-policy.md).

## Consequences

- Future terminal-theme work must preserve the distinction between terminal colors and application chrome colors.
- A default terminal-theme change does not authorize implementing or changing application day/night mode behavior.
- Terminal-theme selection cannot serve as the application appearance-mode selector.
- This record establishes the approved boundary, not a claim that day/night modes or theme-selection UI have been implemented.
