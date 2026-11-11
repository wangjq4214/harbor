# Environment-Aware Project Presets and Recognized Agents

**Status:** Proposed
**Date:** 2026-11-04
**Sources:** Owner refinement conversation: the desired workflow is to return to a project, restore its activated virtual environment and optionally start a recognized agent; the owner uses pwsh and explicitly requests automatic WSL/SSH/virtual-environment recognition rather than manual reconstruction.
**Supersedes:** [ADR 0108](./0108-explicit-session-preset-snapshots-and-safe-configuration-writes.md) only in its launch-provenance-only capture boundary and blanket exclusion of recognized runtime environment/program information. Its selected-file, explicit-confirmation, conflict-safety and credential-protection decisions remain in force.
**Superseded by:** [ADR 0110](./0110-last-recognized-agent-launch-with-original-arguments.md) for running-at-save-time eligibility, first-version virtual-environment scope and mandatory fresh-agent-session semantics. The owner now requests the last recognized agent with original arguments and defers virtual environments; original resume arguments are honored, without adding resume options.

## Context

The owner reports that the current quick-save captures too little to be useful and saves the wrong directory after changing directories in pwsh. The current capture uses usable local OSC 7 metadata or the initial launch directory; this does not establish the shell's actual current directory when metadata is absent or stale. The reported failure's specific cause has not been diagnosed.

The desired reusable object is a project development environment, not merely the executable and arguments originally used to start the terminal.

## Decision

Move project preset capture toward automatic recognition of the active execution environment, including WSL or SSH context and activated virtual environments such as Python venv. The intended reopening workflow returns to the project in that environment and restores its activation rather than requiring the user to manually reconstruct these facts.

Do not generally record/replay the last command or command history. Recognized agents are the selected exception: the initial requested set is pi, Codex, Claude Code and OpenCode, which may be recorded for launch when reopening the preset. This does not authorize arbitrary command replay or runtime process cloning.

Recognizing an activated environment is distinct from dumping its environment variables or persisting credentials. Existing secret-protection and explicit-save boundaries remain applicable.

### Refinement confirmed 2026-11-04

The owner accepts guided shell integration to supply directory and activated-environment information, rather than requiring zero-configuration inference from terminal text. WSL/SSH-side cooperation may be necessary; accepting this approach does not authorize unsolicited remote installation or select a concrete integration mechanism.

For the first version, recognize supported agents only when they are still running at save time; do not infer an agent from earlier command history. Reopening starts a fresh agent session, not automatic resumption of a previous conversation.

## Consequences

- The previous launch-only snapshot is no longer the complete intended product model; current implementation and spec 0052 do not yet establish the new behavior.
- Directory identity must be interpreted in its execution environment; identifying SSH/WSL cannot be treated as evidence of a local Windows directory.
- Guided shell integration is selected, but its concrete mechanism, supported shell/nesting combinations and installation procedure remain unsettled. No unsolicited local or remote configuration changes are authorized by this record.
- The precise virtual-environment support matrix beyond the named venv example remains unsettled.
- Agent argument preservation and behavior when detection is unavailable remain unsettled. Recognition timing and fresh-session semantics are settled above; no implementation or acceptance claim is made.
