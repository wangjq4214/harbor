# Independent Session Profiles and Explicit OSC 7 Launch Policy

**Status:** Proposed
**Date:** 2026-11-02
**Sources:** [Issue #157](https://github.com/wangjq4214/harbor/issues/157) and its refinement conversation: the owner approved the recommended independent-profile, legacy-launch compatibility and fail-visible direction, explicitly included OSC 7 inheritance, and requested a quick-save-current-preset mechanism.
**Extends:** [ADR 0062](./0062-watched-reload-and-new-session-launch-settings.md) for named launches and [ADR 0033](./0033-osc7-metadata-without-filesystem-side-effects.md) with explicit user-action-driven Host policy. Existing shell configuration and parser metadata behavior remain unchanged.

## Context

Harbor already has child launch settings and a shared application-command path. Named profiles should reuse these boundaries without changing existing ordinary launches. Silently falling back to a different executable after choosing a named profile would misrepresent the user's selection.

OSC 7 describes an active session's directory but remains untrusted terminal metadata. Directory inheritance for a new user-requested session requires explicit application Host policy, not parser-driven file-system effects.

## Decision

Named session profiles are independent launch presets with stable identity, a display name, shell executable, argument array, initial directory and environment overrides. They do not inherit another profile or the legacy `[shell]` settings. Ordinary application startup and ordinary New tab/Ctrl+T retain the existing `[shell]` behavior.

Named profile creation reuses centralized configuration validation and platform process quoting/creation. Profile changes affect future sessions only; existing child identity, environment and directory remain unchanged. A named profile's executable failing to start is a visible failed creation, not permission to launch a different shell. The legacy `[shell]` fallback remains unchanged.

For named launches, directory selection is ordered: explicit launch directory, configured profile directory, usable local OSC 7 directory from the source session, then Harbor's inherited process directory. OSC 7 inheritance is included in this scope. The Host must safely interpret the URI and determine local usability; raw URI paths are not native paths, and inheritance must not implicitly access remote locations. This policy selects only a new child's initial directory and never changes Harbor's global directory or an existing child. It does not change ordinary legacy launch-directory semantics.

Configured relative directories remain anchored at the configuration file. Environment semantics, protected TERM, literal values, confidentiality and coherent configuration acceptance retain ADR 0062's rules. A selected configured directory becoming unavailable before spawn still fails visibly rather than falling back.

Profile selection through new-session UI and the command palette uses the established application-command/Host path, not a second direct UI-to-PTY execution path. No credentials or automatic secret persistence are authorized by this feature.
The selected profile interaction reuses the command palette for second-level selection and input-only quick saving, followed by an in-window confirmation overlay rather than a new native window. See [ADR 0107](./0107-shared-palette-profile-flow-and-in-window-confirmation.md).

## Consequences

- Existing users retain ordinary launch behavior while explicitly selected profiles have predictable independent launch policy.
- OSC 7 remains parser/model metadata; only the explicit launch action's Host policy may use it to select a new child's directory.
- UI and accepted future launch policy must remain coherent when profiles change; stale selections cannot bypass execution-time checks.
- Actual child argument, environment and directory observations plus Windows runtime evidence are required; configuration-only tests do not establish delivery.
