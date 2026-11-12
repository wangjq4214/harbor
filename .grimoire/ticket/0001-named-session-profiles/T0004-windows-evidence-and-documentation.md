# Integrated Windows Evidence and Truthful Project-Preset Documentation

**Ticket ID:** T0004
**Source:** [Spec 0052](../../spec/0052-named-session-profiles-and-quick-save.md), revised 2026-11-04: R8 and integrated R1–R9/E1–E19; [relationship README](./README.md); [ADR 0110](../../adr/0110-last-recognized-agent-launch-with-original-arguments.md); [#157](https://github.com/wangjq4214/harbor/issues/157).
**Status:** Todo

## Goal

Establish retrievable evidence for the complete independent-profile and environment-aware project-preset flow, document actual supported behavior and exclusions, and report an honest package disposition. Cover local pwsh, WSL bash and direct-SSH bash, including current-directory capture and relaunch of the last recognized supported agent with original arguments.

This is not permission to mark unrun cases passing or close #157 without its evidence/remaining-scope requirements. T0001/T0002 ticket files remain unchanged at the owner's request; their unchanged statuses or historical source references do not establish implementation acceptance. Current spec 0052 governs the integrated verification.

## Affected Surfaces

Integrated Windows/ConPTY, guided shell integration, WSL/direct-SSH and UI regression verification; actual environment/directory/agent argument observations; quality-gate records; durable evidence under `docs/verification/` or retrievable CI artifacts; applicable user configuration, setup, troubleshooting and developer/status/protocol documentation.

Use synthetic profiles, directories, agent arguments and suspected-secret fixtures. Do not disclose real credentials, unrelated terminal content or environment values.

## Dependencies and Coordination

- **Blocked by:** T0002's actual shared chooser/entry paths and T0003's complete recognition/capture/correction/confirmation/persistence/publication/relaunch flow. Both are needed for combined launch → capture → save → chooser → environment/project/agent behavior.
- **Coordination:** Use the exact integrated revision/dirty scope, including T0001's baseline launch contract and T0003's schema/Host extensions. Reuse applicable focused evidence with its scope and limitations, not historical acceptance as a substitute for expanded runtime results. T0003 can verify its Host/catalog contract independently; this ticket verifies the combined chooser interaction.
- **Defect ownership:** Return delivery defects to the relevant delivery scope, including T0003's expanded behavior; this evidence ticket is not authorization to invent requirements or silently widen frozen T0001/T0002 files.
- **Remote tracking:** Obtain explicit authorization before creating/updating implementation sub-issues, issue links or closure. Local ticket completion is not an implicit remote mutation request.

## Acceptance

### Existing contracts and complete user flow

- [ ] **E1–E7:** Exercise independent profile launches through both selection entry paths, actual-child argv/env/cwd, ordinary `[shell]` compatibility, strict named-launch failure, local directory precedence, configuration-relative local paths, coherent reload and stale-selection rejection. Existing children and process-global cwd/environment remain unchanged.
- [ ] **E4/E5/E15/E19:** Verify safe local OSC 7 interpretation, including remote/malformed/reset/missing metadata and percent-encoded local paths. Distinguish R3's permitted local named-launch fallback from R6's prohibition on presenting initial cwd as current during Save. Arbitrary remote metadata causes no implicit network probe; explicitly selected WSL/SSH presets are a separate operation.
- [ ] **E8–E10:** Exercise both save entry paths, input-only naming and styled right-hand Save… button, equivalent Enter/click advance, empty/duplicate feedback, internal review, return/cancel and correction. Chinese IME, held/repeated Enter, popup replacement/stale events and consumed-input isolation behave correctly; save confirmation creates no new native window.
- [ ] **E10/E11:** Verify stable source environment, Save-time directory, agent/argument snapshot and user corrections through later tab/directory/agent-record/profile changes. Default/opt-in configured environment-variable overrides retain their confidentiality rules, separate from deferred virtual-environment capture.
- [ ] **E12/E14:** Demonstrate save → accepted chooser selection → relaunch and restart reload, with unrelated settings/comments preserved, installed/portable selected-file isolation and no duplicate/stale watcher/manual publication.
- [ ] **E13/E14:** Record external-edit, invalid-file, name-race, permission and replacement-fault outcomes, including draft retention and honest incomplete disk/in-memory outcomes. Unexercised faults are NOT RUN/BLOCKED, not inferred from happy-path success.

### Expanded project and agent behavior

- [ ] **E10/E15:** In local pwsh, WSL bash and direct-SSH bash with guided integration, change directory before saving and observe the saved execution environment/current project directory. Reopen into that environment and directory before any selected agent launches. WSL/SSH paths are not interpreted as native-local Windows paths.
- [ ] **E10/E15/E19:** Exercise absent, stale or unusable environment/current-directory recognition. The limitation is explicit and user-correctable; no silent initial-directory fallback is shown as confirmed current data. Record setup prerequisites and omissions without claiming zero-configuration recognition or performing unsolicited local/remote installation.
- [ ] **E16:** Save a running supported agent and an already-exited one; ordinary commands retain the last recognized invocation, while a later supported agent replaces it only in the same terminal/execution-environment scope. Verify separate tabs, execution environments and SSH hosts do not share the wrong record. Changing cwd after agent exit saves the current directory, not the earlier invocation's directory.
- [ ] **E15/E17:** Exercise pi, Codex, Claude Code and OpenCode with original arguments across the supported environments. Observe actual argument boundaries/meaning, including empty/spaced/quoted/backslash/Unicode arguments where exercised. Existing resume options remain effective; Harbor neither forces a fresh session nor adds resume options. Report coverage per agent/environment, with explicit unrun combinations rather than extrapolated support.
- [ ] **E18:** Use synthetic suspected password/token arguments to verify visible handling before persistence or cancellation. Credentials are not directly persisted or disclosed; silent removal is not represented as exact copying, and confirmed changes remain apparent. Do not claim perfect secret detection.
- [ ] **E19:** Verify documentation/results do not promise pre-integration history recovery or arbitrary alias/wrapper recognition. Ordinary command history is not replayed. Multi-layer nesting and venv/Conda/Poetry recognition/activation remain deferred, not silently claimed as restored.

### Evidence and documentation

- [ ] **R8/E1–E19:** Record revision, dirty-tree scope, OS/ConPTY/application versions and relevant shell, WSL/SSH and agent versions; integration/setup prerequisites; reproducible steps; expected/observed outcomes; retrievable artifact locations; exclusions; and PASS / FAIL / NOT RUN / BLOCKED. Distinguish real runtime observations from helper tests and document-only review.
- [ ] **R8:** Record `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --workspace` and `python scripts/docs/check_docs.py`, including commands not run and reasons. Before delivering staged changes, also record `python scripts/docs/check_docs.py --staged`; working-tree checks include nonignored new files and do not establish the staged snapshot's validity. The former `scripts/checklist_summary.py` was retired as documented in [Development Scripts](../../../scripts/README.md); retain criterion-to-evidence results directly rather than treating that retired command as a missing gate or a passed check. Focused tests belong in delivery slices as well; this ticket does not defer their correctness obligations.
- [ ] **R1–R9:** Update user documentation only for implemented/evidenced configuration spelling, commands, guided setup and supported shells/environments/agents. Explain local named-launch precedence versus Save-time current directory, unknown-state correction, last-agent retention/scope, original arguments/resume behavior, suspected-secret handling, stable confirmation, configured-override opt-in, safe selected-file writes and portable targeting.
- [ ] **R6/R9:** Document actual recovery limitations, unrecognized invocations and required cooperation in WSL/SSH. Explicitly retain deferred virtual-environment support/multi-layer nesting and absence of general history replay or automatic credential storage.
- [ ] **R8:** Status/protocol claims match retrievable evidence, not the existence of a spec or ticket. Keep deferred/missing scope explicit. #157 remains a work-package tracker until accepted scope is evidenced or an explicit linked scope disposition covers the rest.

## Out of Scope

Inventing new requirements or capture algorithms during verification; broad Windows/shell/agent compatibility claims beyond evidence; silently fixing unrelated packages; changing T0001/T0002 ticket files; forced issue closure; real credentials/user content in evidence; or marking document creation as runtime validation.

This ticket revision records future verification/documentation obligations only. It does not execute tests or QA, mutate remote issues, change completion statuses, or implement the feature.
