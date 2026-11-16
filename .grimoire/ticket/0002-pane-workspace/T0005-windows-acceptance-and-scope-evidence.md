# Verify the Integrated Windows Pane Workspace

**Ticket ID:** T0005
**Source:** [Spec 0056](../../spec/0056-pane-workspace.md), R8 and all E1–E12; [#158](https://github.com/wangjq4214/harbor/issues/158).
**Status:** Todo

## Goal

Produce reproducible integrated evidence for the delivered pane workspace and its environment-aware split/fallback behavior, and reconcile documentation to actual results without claiming that component tests prove interoperability.

## Affected Surfaces

- Focused existing model, command, mounted widget, offscreen GPU and Windows/ConPTY test suites.
- Local pwsh, WSL bash and controlled direct-SSH bash runtime fixtures.
- Resource/idle/output observations, user/developer documentation and scoped acceptance records.

## Approach

Use [current testing guidance](../../../docs/developer/testing.md) and each test's resource requirements. Prepare fixtures and controlled hosts early, but execute final acceptance against the integrated candidate revision. Use the current Windows test wrapper/recipes rather than unfiltered workspace `--ignored` runs.

The issue's old `scripts/check_docs.py`, `scripts/checklist_summary.py` and `docs/validation.md` references do not match the current checkout. The docs checker is `scripts/docs/check_docs.py`; record current equivalents and unavailable commands honestly. This is not permission to recreate retired tooling or modify quality policy.

## Dependencies and Coordination

- **Blocked by:** T0004 for full integrated behavior. Evidence setup and independently applicable component checks may occur earlier.
- **Blocks:** No further ticket in this set; complete acceptance informs a separately authorized #158 disposition.
- **Coordination risks:** Real WSL/SSH prerequisites, authentication, integration setup and version drift must be recorded. A successful default-shell fallback is not evidence that a same-environment launch succeeded.

## Acceptance

- [ ] Every spec scenario E1–E12 has an explicit PASS / FAIL / NOT RUN / BLOCKED disposition, reproduction steps and actual expected/observed results.
- [ ] Actual new-child observations prove split-time environment and cwd for local pwsh, WSL bash and direct-SSH bash; distinguish these from fixture-only launcher assertions.
- [ ] Missing/unusable recognition, failed connection, directory-entry race, failed partial startup, default fallback and failure of the default launch are observed without orphan processes or stale attachment.
- [ ] Two and several visible mixed-direction panes remain responsive under simultaneous output, drag, resize and DPI changes; maximize/inactive-tab hidden grids and restoration match the contract.
- [ ] Native keyboard, IME, mouse/capture, copy/paste and confirmation-gate behavior are observed in the correct panes, including allowed focus/close/exit races and UI-chord byte non-leakage.
- [ ] Explicit pane/tab close, natural shell exit, foreground-program completion and last-window behavior are distinct and correct, with survivor/resource checks over repeated cycles.
- [ ] Coherent live configuration acceptance/rejection/recovery is exercised with unequal visible/hidden pane sizes and pending output/resize.
- [ ] Report idle scheduling, visible/hidden work, sustained-output progress and repeated lifecycle resource behavior with workload/environment and measurement limitations. Do not invent new numeric performance thresholds or treat unrelated hosted performance probes as native pane latency evidence.
- [ ] Evidence identifies exact revision/dirty paths, Windows/ConPTY and shell/WSL/SSH versions, commands/features, integration/host prerequisites, retrievable artifacts and exclusions without secrets or unrelated terminal captures.
- [ ] Run applicable format, strict Clippy, workspace/core, documentation and selected resource gates; record omitted/obsolete commands and why. Configured automation and source inspection alone are not successful execution.
- [ ] User/status/protocol documentation describes only implemented and evidenced behavior. Keep unverified outcomes explicit; do not close #158 or mutate GitHub without separate permission and full-scope evidence or an explicit remaining-scope decision.

## Out of Scope

New feature scope, unsupported success claims, release packaging work beyond pane acceptance, remote issue mutation, staging/committing artifacts and automatic installation on test hosts.
