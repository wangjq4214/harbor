# Launch a Split in the Source Environment and Directory

**Ticket ID:** T0002
**Source:** [Spec 0056](../../spec/0056-pane-workspace.md), R2–R3, startup lifetime portions of R1/R7, E1–E4/E12 and C5's final correction.
**Status:** Todo

## Goal

Provide the identity-qualified Host launch path needed by split commands: enter the source session's same environment/current directory, or automatically launch the accepted default `[shell]` policy after inherited startup fails.

## Affected Surfaces

- Host/factory and existing default-shell launch pipeline.
- Session-scoped project/context recognition and existing environment-aware launch boundaries.
- Local/WSL/SSH process creation, startup-readiness/failure observation and partial-launch cleanup.

## Approach

Reuse existing opt-in project integration and environment launch validation/quoting where suitable. Capture the intended source environment/directory for the split operation; do not read whichever pane is active at delayed completion. Current directory is not initial directory or an earlier agent's directory.

The split action authorizes the required connection/directory entry. Receiving terminal metadata alone does not. Preserve normal authentication, credentials confidentiality, and the absence of unsolicited integration installation.

Inherited-launch success must mean the intended environment/directory was reached, not just that `ssh.exe` or `wsl.exe` spawned. Detection/signaling mechanisms are implementation details constrained by that observable contract. Keep this independently testable before the production UI command is admitted in T0004.

Current implementation facts to account for: the bash integration receives its environment target from the user rather than discovering/verifying it, and the existing project launch templates provide no Host-observable, launch-attempt-correlated readiness result. Local pwsh can remain alive under `-NoExit` even after a directory-entry error; WSL/SSH launcher creation likewise does not prove connection/cwd success. Deliver explicit observable startup success/failure under R3 rather than treating these existing paths as an already complete contract.

## Dependencies and Coordination

- **Blocked by:** T0001, because source/session/operation identity must survive delay and replacement.
- **Blocks:** T0004's production split command admission.
- **Parallel with:** T0003 after T0001; agree on creation/completion and resource attachment contracts. Shared Host files are merge coordination, not another blocking edge.

## Acceptance

- [ ] Local pwsh, WSL bash and direct-SSH bash fixtures use the actual current environment/directory at the source operation, not a native-local reinterpretation or stale launch cwd.
- [ ] A successful inherited launch creates an independent shell/session without copying the foreground process or automatically relaunching a recognized agent.
- [ ] Unknown/unusable recognition, failed connection, unavailable WSL target and failed directory entry automatically choose the accepted default `[shell]` launch policy.
- [ ] Fallback uses default executable/arguments/directory/environment semantics without source remote paths or stale source-specific launch fields.
- [ ] Connection/cwd startup failure is distinguishable from a successful session's later exit; only startup failure invokes this fallback. Preserve ordinary default-launch recovery/failure behavior rather than recursively relaunching.
- [ ] Failed partial attempts release their child/reader/PTY resources before a replacement is published; a delayed result cannot replace the fallback or attach to an unrelated/closed workspace.
- [ ] Default launch failure remains an honest failure with no falsely live pane, orphan process or damage to survivors.
- [ ] Existing named-profile fail-visible behavior, save-time unknown/correction behavior, normal New tab and configuration validation/reload are unchanged.
- [ ] Tests exercise argument/path boundaries, current versus initial cwd, source switching/closure, partial failure and both-stage failure. Real connection/cwd observations are distinguished from mocked launch assertions; missing hosts/integration are recorded as BLOCKED/NOT RUN.

## Out of Scope

Production split widgets/shortcuts, arbitrary shell-command replay, agent/preset saving changes, runtime environment cloning, credentials persistence and automatic remote installation.
