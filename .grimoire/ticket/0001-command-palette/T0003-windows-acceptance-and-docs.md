# Integrated Windows Acceptance and Truthful Documentation

**Ticket ID:** T0003
**Source:** [Spec 0030](../../spec/0030-command-palette.md), R8 and E1–E9; [issue #156](https://github.com/wangjq4214/harbor/issues/156); [relationship and coordination contract](./README.md).
**Status:** Todo

## Goal

Establish retrievable evidence for the actual first-delivery palette and accurately document implemented behavior, limits and deferred #156 scope. This ticket is future verification work, not a claim that any native checks have already run.

## Affected Surfaces

- Integrated Windows application/ConPTY/input/focus/IME/clipboard/window-lifecycle scenarios.
- Quality-gate commands and focused test results from T0001/T0002.
- `docs/app/commands.md`, configuration/workflow/troubleshooting pages as affected, and `docs/developer/progress.md`/`roadmap.md` for evidenced status only.
- Durable evidence in `docs/verification/` or retrievable CI artifacts; local evidence directory does not exist at refinement time and may be created during this ticket.

## Approach

Verify the integrated revision, not a stale development binary or a predecessor's generic editor smoke. Use controlled synthetic terminal/clipboard data. Exercise the real palette-to-command-to-result path; capture expected versus observed behavior and evidence without recording unrelated payloads.

Correctly distinguish product failures from environment blockers; return defects to the relevant code slice and rerun affected scenarios after fixes. Do not weaken the specification or infer compatibility from parser/model tests. Update claims only when supported by the recorded scope.

## Dependencies and Coordination

- **Blocked by:** T0001 and T0002 for final integrated runs and delivered-scope documentation. Scenario preparation may proceed before integration.
- **Blocks:** Claiming spec 0030's integrated acceptance, not unrelated profile/pane/appearance implementation.
- **Coordination risks:** Any implementation fix invalidates affected evidence; record revision and dirty scope for every final run. A shared file is a coordination concern, not a new semantic blocker.

## Acceptance

- [ ] Record exact revision/dirty-tree scope, Windows/ConPTY/application versions, configuration, reproducible steps, expected/observed outcomes, artifacts and exclusions with PASS / FAIL / NOT RUN / BLOCKED.
- [ ] Run spec E1–E9 for the delivered scope on the integrated Windows binary, including actual Chinese candidate confirm/cancel behavior, focus restoration/destinations, field clipboard editing, alternate-screen rejection, accepted/rejected binding reload, tab/window-close behavior, search and multiline paste confirmation. Capture controlled PTY evidence for consumed-input isolation and resumed terminal pass-through.
- [ ] Record supported root-replacement/focus-loss/stale clipboard/IME scenarios with actual observations. Unsupported/unavailable environments remain explicitly NOT RUN/BLOCKED; do not relabel headless coverage as native acceptance.
- [ ] Run and record `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --workspace` and `python scripts/docs/check_docs.py`, plus focused tests. Record commands not run and why.
- [ ] Explain #156's obsolete commands: documentation checker is now `scripts/docs/check_docs.py`; `scripts/checklist_summary.py` was removed, so its old command is NOT RUN with the reason from the current developer guide, not recreated or marked PASS.
- [ ] Evidence is retrievable under `docs/verification/` or linked CI artifacts and contains no secrets/unrelated terminal or clipboard content. Keep test reports out of concise developer status pages.
- [ ] User docs state the delivered opener/override grammar, filtering, actual shortcuts, disabled reasons, keyboard/IME behavior, feedback and focus rules with observed limitations. Remove obsolete 'palette unavailable' statements only for the evidenced delivered scope.
- [ ] Progress/roadmap and the implementation handoff retain profile/pane/appearance deferrals. Link bounded implementation sub-issues and remaining scope when authorized to update GitHub; do not treat local ticket completion as automatic #156 closure.
- [ ] Final handoff separates implemented/tested outcomes, native failures/blockers and deferred package work. No full acceptance claim while required checks are unresolved; no broad terminal/application interoperability claim from this palette evidence.

## Out of Scope

Expanding command scope to unimplemented features, broad platform/application certification, implementation-plan generation, inventing performance thresholds, or closing #156 without its remaining-scope/evidence conditions and appropriate authorization.
