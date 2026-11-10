# Confirmed Quick Save and Relaunch Verification

## Outcome: automated and unit/integration suites PASS; integrated desktop evidence tracked under T0004

This document records automated test and integration evidence for ticket T0003 under spec 0052 and spec 0053. It establishes that guided shell integration, stable draft capture, in-window review and correction, credential protection, conflict-safe TOML writing, and environment-aware relaunch templates are verified.

## Scope and execution environment

- Platform: Windows 10/11 x64, Microsoft ConPTY runtime 1.24.260710001, PowerShell 7.5.4, Git Bash 2.56.0.
- Target surfaces:
  - Guided shell integration: `scripts/shell/harbor-project.ps1`, `scripts/shell/harbor-project.bash`
  - Configuration schema & persistence: `crates/harbor-config/src/project.rs`, `crates/harbor-config/src/profile_save.rs`
  - Review UI & state: `crates/harbor-app/src/quick_save.rs`, `crates/harbor-app/src/quick_save_project.rs`
  - Host launch templates: `src/profile_launch/project.rs`, `src/profile_save.rs`
- Target contracts: Spec 0052 requirements R5, R6, R7, R9; Spec 0053 requirements R1–R7; acceptance criteria E8–E19.

## Verified contracts and observations

### 1. Opt-in guided shell integration (R6, R9, E15, E16, E19)

- `scripts/shell/harbor-project.ps1` and `scripts/shell/harbor-project.bash` provide inert OSC `777;harbor;1;...` metadata without self-installing or modifying disk profiles.
- Current project directory is reported on prompts; commands invalidate directory state (`busy`).
- Last recognized supported agent (`pi`, `codex`, `claude`, `opencode`) invocation and original argv are preserved across agent exit and ordinary commands in the same terminal/environment scope.
- Submitting a subsequent supported agent invocation updates the record in that scope only; tabs and SSH destinations do not leak records.
- Verified by:
  - `scripts/shell/test_project_integration.py`: isolated pwsh and bash integration tests with synthetic agents.
  - `crates/harbor-terminal/tests/project_metadata.rs`: `project_metadata_retains_exited_agents_but_not_stale_cwd_or_other_terminal_records`.

### 2. In-window review UI and reusable controls (R5, Spec 0053 R1–R7, E8, E9, E10)

- Naming uses single-line input with adjacent `Save…` button; Enter and button advance to review without writing. Empty or duplicate names disable advance with nearby feedback.
- Review is presented in an in-window overlay sharing translucent glass appearance with no separate native window.
- Organizes details into 6 distinct blocks: Header, Project & environment, Agent, Environment variables, Supporting details, and Final actions.
- `SegmentedControl` provides single-choice environment switching (`Local pwsh`, `WSL bash`, `Direct SSH bash`).
- `Checkbox` provides caller-driven state for agent inclusion, argument review acknowledgement, and configured override selection.
- `TextLabel::wrap()` and `Flex::wrap(true)` provide measurement-driven text and action button wrapping on constrained viewports.
- Unknown/edited context requires explicit user correction via "Apply context correction"; typing alone does not authorize saving.
- Suspected password/token arguments in agent invocations hide arguments and block saving that invocation.
- Verified by:
  - `crates/harbor-app/src/quick_save_tests.rs`: 849 lines covering naming, review, correction, acknowledgements, credentials, and cancel/return.
  - `crates/harbor-widget/tests/selection_controls.rs`: 432 lines covering Checkbox and SegmentedControl behavior.
  - `crates/harbor-widget/tests/flex_layout.rs`: flex wrapping layout tests.

### 3. Safe configuration persistence (R7, E12, E13, E14)

- Appends `[profiles.saved-N]` with lossless comment and setting preservation using `toml_edit`.
- Windows handle-guarded replacement verifies file identity before atomic move-and-replace, detecting external edits and preventing partial writes.
- Portable mode writes strictly to the executable-adjacent configuration file.
- Failed commits retain the editable draft and display honest error feedback.
- Verified by:
  - `crates/harbor-config/src/profile_save.rs`: `append_retains_comments_settings_and_literal_recipe_and_rejects_duplicates`
  - `src/profile_save/tests.rs`: `real_flow_to_writer_to_accepted_catalog_retains_disk_only_failure_and_retries_once`.

### 4. Environment-aware relaunch templates (R9, E15, E17)

- Generates fixed launch templates for `pwsh`, `wsl.exe`, and `ssh.exe`.
- Enters saved project directory before launching any selected agent.
- Preserves empty, spaced, quoted, backslash, and Unicode arguments, as well as original resume flags.
- Re-emits recognized agent metadata while running so subsequent saves in the relaunched session continue to recognize context.
- Verified by:
  - `src/profile_launch/project_tests.rs`: POSIX and PowerShell literal boundary quoting, argument preservation, and ConPTY pwsh probe test.

## Scoped exclusions and deferred items

- Automatic virtual-environment detection and activation (venv, Conda, Poetry) remain deferred.
- Multi-layer nested environments (e.g. SSH inside WSL) remain deferred.
- Arbitrary command replay, zero-configuration guessing, and remote auto-installation remain excluded.
