# Watched Reload and New-Session Launch Settings

**Status:** Proposed
**Date:** 2026-10-15
**Sources:** [Issue #155](https://github.com/wangjq4214/harbor/issues/155) and its refinement conversation: the user explicitly included environment variables and launch directory, then approved the proposed rules, default-on watching, and spec-to-ticket endpoint.
**Extends:** [ADR 0059](./0059-atomic-manual-reload-and-terminal-only-fonts.md), with its existing [bounded-resize](./0060-font-reload-uses-normal-bounded-resize.md) and [compensation-failure](./0061-manual-reload-compensation-failure.md) qualifications unchanged.

## Context

Manual reload already has a strict application-owned acceptance boundary. Automatic file saves must not introduce a second acceptance policy or bypass resource preparation. The original package also includes environment and launch-directory settings, which are absent from the current settings schema and excluded from the initial manual-reload spec.

Windows currently constructs a child environment from inherited variables and forces `TERM=xterm-256color`; process creation inherits Harbor's working directory. Configuration needs explicit child-only semantics rather than changing Harbor's global environment or directory. OSC 7 remains descriptive metadata under [ADR 0033](./0033-osc7-metadata-without-filesystem-side-effects.md), not permission to change directories.

## Decision

Automatic configuration watching is enabled by default and remains separate from widget-library HMR. Manual and watched requests share strict complete-candidate validation and application-thread acceptance. Debounce and editor file replacement must not produce mixed accepted generations. Stable invalid edits retain the previous accepted state and produce actionable feedback; repeated identical failures avoid notification flooding. Watcher failure is visible, and manual reload remains available.

Accepted shell environment and launch-directory changes apply to newly created sessions only. Existing children keep their environment and current directory; no PTY is restarted. These settings participate in complete-candidate rejection and coherent future-factory publication alongside colors, bindings and terminal fonts.

Environment configuration adds or overrides inherited variables. Windows names are case-insensitive and conflicting configured names are rejected. Empty string is an empty value, not deletion. Removing an override restores inheritance; there is no dedicated inherited-variable deletion syntax in this scope. Values are literal, without variable interpolation or shell evaluation. `TERM=xterm-256color` remains protected: a configured different value is rejected, not silently overridden. Diagnostics may identify variable names but must not expose their values.

Launch directories support absolute paths and resolve relative paths against the configuration file's directory. Omitting the setting retains inherited-directory behavior. Invalid, nonexistent or non-directory configured paths reject the whole reload candidate. If the accepted directory becomes unavailable before spawning, creating that session fails visibly rather than falling back to a different directory. Existing sessions remain unaffected.

## Consequences

- File events trigger the established transaction, not startup fallback or a second configuration owner.
- Launch settings are per-child inputs; no process-global environment mutation or `chdir` is needed to apply them.
- New-session tests must observe actual child environment and directory, not only parsed settings or a factory snapshot.
- A successful reload does not guarantee a directory remains usable until a later spawn; spawn-time failure must remain honest.
- This is an approved future contract, not implementation or runtime acceptance. Watch timing, concrete TOML field names and watcher-library selection are not fixed by this decision.
