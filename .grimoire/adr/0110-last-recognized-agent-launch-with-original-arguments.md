# Last Recognized Agent Launch with Original Arguments

**Status:** Proposed
**Date:** 2026-11-04
**Sources:** Owner refinement correction: copy original agent arguments, remember the last agent rather than require it to remain running, and defer virtual-environment support.
**Supersedes:** [ADR 0109](./0109-environment-aware-project-presets-and-recognized-agents.md) for running-at-save-time eligibility, first-version virtual-environment scope and mandatory fresh-agent-session semantics. Original arguments govern resume behavior; Harbor adds no resume options.

## Context

Limiting recognition to an agent still running when Save is opened loses the useful launch recipe after that agent exits. Starting only a bare default agent command also loses the owner's selected launch options. The owner explicitly requests the most recent agent invocation with its original arguments and narrows the first version by deferring virtual environments.

## Decision

Remember the most recent recognized invocation of a supported agent (pi, Codex, Claude Code or OpenCode), including its original launch arguments, for reuse by project preset saving. The agent does not need to remain running at save time. This is a bounded agent-launch record, not general command-history persistence or arbitrary last-command replay.

The owner confirms that the last-agent record is scoped to the current terminal and execution environment, not shared across tabs or SSH hosts. Ordinary commands do not overwrite it. The project directory captured for saving is the current directory at save initiation, not implicitly the directory of the earlier agent invocation.

Preserve the meaning of original agent arguments: existing conversation-resume options remain effective, rather than forcing a fresh session; Harbor does not add resume options. Credentials are not directly persisted. On detecting suspected password/token arguments, require user handling rather than silently dropping them and claiming the complete invocation was saved. This policy does not claim that every possible secret can be detected.

Defer virtual-environment detection and activation from the current delivery scope. Execution-context recognition for local/WSL/SSH project launches remains the selected direction. Guided shell integration, explicit save confirmation and non-destructive selected-file persistence remain applicable.

### Coverage and unknown-state handling confirmed 2026-11-04

The owner selects local pwsh, WSL bash and direct-SSH bash for the first version, excluding multi-layer nesting. Missing or unusable execution-context/current-directory recognition must be explicit and correctable; the old silent fallback to the initial directory is not acceptable for project preset capture. This supersedes that capture fallback in [ADR 0108](./0108-explicit-session-preset-snapshots-and-safe-configuration-writes.md), not ADR 0106's separate local named-launch directory precedence.

## Consequences

- Recognition must retain relevant launch information beyond process exit. The concrete observation mechanism is not selected here, and arbitrary commands or wrappers are not assumed reliably recognizable.
- Last-agent selection is isolated by terminal and execution environment; arbitrary command history is neither persisted nor replayed.
- Original resume arguments supersede the earlier mandatory-fresh-session rule. Sensitive argument handling must be visible and must not turn a modified invocation into a claimed exact copy.
- First-version support is local pwsh, bash inside WSL, and bash reached through a direct SSH connection. Multi-layer nested execution environments are deferred.
- If the execution environment or current directory cannot be recognized, show that limitation and allow correction; do not silently substitute the source session's initial directory.
- This is a requirements-direction change, not a claim that capture or relaunch has been implemented or verified.
