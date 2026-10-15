# Manual Reload Compensation Failure

**Status:** Proposed
**Date:** 2026-10-14
**Sources:** User approval during spec 0023 implementation planning.
**Supersedes in part:** [ADR 0059](./0059-atomic-manual-reload-and-terminal-only-fonts.md), only unconditional restoration after a compensating native PTY resize itself fails.

## Context

The existing ConPTY control path resizes one PTY through a fallible native call. Preparing all models and renderers before acceptance prevents preparation failures from publishing partial settings. If a later PTY resize fails after an earlier PTY succeeds, restoring that earlier PTY is another fallible call; reader barriers do not guarantee its success. There is no measured failure rate for this combined scenario.

## Decision

The user permits an explicit recovery-failed outcome for this double-failure case. Keep the previous accepted configuration and resources, never report success or publish a partial generation, resume every paused reader, attempt geometry reconciliation, and identify unrecovered sessions. Do not falsely state that all prior geometry was restored. Ordinary validation/resource/barrier failures still reject the complete candidate; ordinary later-session resize failure requires compensation. The exception applies only when compensation also fails.

## Consequences

- Multi-session tests must inject later-session failure and compensating-resize failure separately, exercise retry, and verify reader resume and honest diagnostics.
- Recovery must use normal bounded model/geometry policies, not session recreation, child restart, hidden content backups, or silent acceptance of new font/palette/binding/factory settings.
- This approval is a policy decision, not implementation or runtime evidence, and does not establish a numerical likelihood of native failure.
