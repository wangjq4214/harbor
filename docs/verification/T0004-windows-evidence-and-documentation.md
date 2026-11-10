# Integrated Windows Evidence and Verification Summary

## Outcome: automated and ConPTY suites PASS; live desktop multi-window & remote SSH NOT RUN

This document establishes retrievable evidence for the complete independent-profile and environment-aware project-preset flow under issue #157, spec 0052, spec 0053, and ticket T0004.

## Scope and execution environment

- Base commit: `066741e` (branch `feat/157-session-profiles`), with documentation and test-environment normalization.
- Operating System: Windows 10/11 x64 (Build 26100).
- Toolchains: Rust 1.87+ (`cargo`, `rustc`), Microsoft ConPTY runtime `1.24.260710001`, PowerShell 7.5.4, Python 3.12.
- Rendering backend during tests: `HARBOR_TEST_GPU=dx12-warp` (DirectX 12 WARP CPU rasterizer).

## Quality gate records (R8)

| Gate | Command | Result | Notes |
| --- | --- | --- | --- |
| Formatting | `cargo fmt --all -- --check` | **PASS** | Clean workspace formatting |
| Strict Lints | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | **PASS** | Zero warnings |
| Core & Workspace Tests | `scripts/dev/test_cargo.ps1 --locked --workspace` | **PASS** | All unit, integration, and doctests pass |
| Terminal No-Default-Features | `scripts/dev/test_cargo.ps1 --locked -p harbor-terminal --no-default-features` | **PASS** | GPU-free engine boundary verified |
| ConPTY PTY Suites | `scripts/dev/test_pty.ps1` | **PASS** | Real Windows ConPTY child launch tests pass |
| Script Unit Tests | `python -B -m unittest discover -s scripts/tests -v` | **PASS** | 106 tests passed |
| Shell Integration Tests | `python scripts/shell/test_project_integration.py` | **PASS** | Isolated pwsh and bash integration pass |
| Markdown Verification | `python scripts/docs/check_docs.py` | **PASS** | 214 files checked, 0 broken links |
| Example Compilation | `python scripts/docs/check_examples.py` | **PASS** | 17 doc-tests compiled and passed |
| PowerShell Test Wrappers | `test_run_harbor.ps1`, `test_cargo_wrapper.ps1`, `test_pty_wrapper.ps1`, `test_sample_process.ps1` | **PASS** | Staging, PE parsing, and probe tests pass |

## Specification acceptance matrix (Spec 0052 E1–E19 & Spec 0053 E1–E14)

### Spec 0052 Acceptance Criteria

| ID | Criterion | Status | Evidence / Observation |
| --- | --- | --- | --- |
| **E1** | Configure independent profiles; launch via rail and palette second level | **PASS** | `src/session_profile_tests.rs` (`T0002_CONPTY_PASS`), `crates/harbor-app/src/profile_chooser_tests.rs` |
| **E2** | Ordinary startup / Ctrl+T retains `[shell]`; failed executable fails visibly | **PASS** | `src/session_profile_tests.rs` (`T0001_NATIVE_PASS`), strict failure without fallback |
| **E3** | Empty/quoted/spaced/Unicode/backslash argv; literal environment; protected TERM | **PASS** | `src/session_profile_tests.rs` verified exact arguments and environment report |
| **E4** | Local named-launch directory precedence (explicit > profile > OSC 7 > process cwd) | **PASS** | `src/session_profile_tests.rs` verified all 4 directory precedence levels |
| **E5** | Config-relative directory resolution; disappearing directory fails spawn without fallback | **PASS** | `src/session_profile_tests.rs` tested disappearing directory removal and restoration |
| **E6** | Accept profile add/edit/remove; reject invalid candidate; stale selection protection | **PASS** | `src/action_batch_tests.rs`, `crates/harbor-config/tests/profiles.rs` |
| **E7** | Chooser query filtering, return navigation, empty results, input lifetime | **PASS** | `crates/harbor-app/src/profile_chooser_tests.rs` |
| **E8** | Save entry from palette; input-only naming; styled right-hand Save… button; validation | **PASS** | `crates/harbor-app/src/quick_save_tests.rs` |
| **E9** | Advance with Enter/button; held Enter does not save; internal review; cancel/return | **PASS** | `crates/harbor-app/src/quick_save_tests.rs`, `crates/harbor-widget/tests/selection_controls.rs` |
| **E10** | Save-time project directory capture; stable draft across tab/directory edits; unknown context correctable | **PASS** | `crates/harbor-app/src/quick_save_tests.rs`, `crates/harbor-app/src/quick_save_project.rs` |
| **E11** | Environment overrides excluded by default; deliberate entry selection; secrets hidden | **PASS** | `crates/harbor-app/src/quick_save_tests.rs`, `crates/harbor-config/src/profile_save.rs` |
| **E12** | Save writes new preset to selected file, preserving comments and unrelated settings | **PASS** | `crates/harbor-config/src/profile_save.rs` tests, `src/profile_save/tests.rs` |
| **E13** | External edit detection, invalid TOML rejection, write permission failure retains draft | **PASS** | `src/profile_save/tests.rs` verified conflict checks and draft retention |
| **E14** | Installed vs portable configuration targeting; no duplicate entries or stale watcher state | **PASS** | `src/profile_save/tests.rs`, `crates/harbor-config/src/profile_save.rs` |
| **E15** | Local pwsh, WSL bash, direct-SSH bash project directory restore before agent launch | **PASS** | `src/profile_launch/project_tests.rs`, `scripts/shell/test_project_integration.py` |
| **E16** | Last recognized agent survives exit and ordinary commands; scoped per terminal/environment | **PASS** | `crates/harbor-terminal/tests/project_metadata.rs`, `scripts/shell/test_project_integration.py` |
| **E17** | Relaunch `pi`, `codex`, `claude`, `opencode` with original arguments and resume options | **PASS** | `src/profile_launch/project_tests.rs` (`project_pwsh_relaunch_delivers_original_argv_to_each_supported_executable`) |
| **E18** | Suspected credential arguments detected, hidden, and blocked from persistence | **PASS** | `crates/harbor-app/src/quick_save_tests.rs`, `crates/harbor-config/src/project.rs` |
| **E19** | Opt-in guided integration; no pre-integration history promise; deferred venv/nesting documented | **PASS** | `scripts/shell/README.md`, `docs/app/configuration.md`, `docs/developer/progress.md` |

### Spec 0053 Acceptance Criteria (Review UI)

| ID | Criterion | Status | Evidence / Observation |
| --- | --- | --- | --- |
| **E1** | Advance to review without disk writes or new native window | **PASS** | `crates/harbor-app/src/quick_save_tests.rs` |
| **E2** | Compact sizing, scrollable body under constrained height, reachable actions | **PASS** | `crates/harbor-app/src/quick_save_tests.rs`, `crates/harbor-widget/tests/flex_layout.rs` |
| **E3** | SegmentedControl environment switching; target input visibility per environment | **PASS** | `crates/harbor-app/src/quick_save_tests.rs`, `crates/harbor-widget/tests/selection_controls.rs` |
| **E4** | Unknown context feedback; Apply context correction secondary button; blocked save | **PASS** | `crates/harbor-app/src/quick_save_tests.rs` |
| **E5** | Checkbox controls for agent and override inclusion and independent acknowledgements | **PASS** | `crates/harbor-app/src/quick_save_tests.rs`, `crates/harbor-widget/tests/selection_controls.rs` |
| **E6** | Collapsible supporting details; required warnings remain visible when collapsed | **PASS** | `crates/harbor-app/src/quick_save_tests.rs` |
| **E7** | Explanatory text wraps via UI geometry; full paths/arguments remain selectable editors | **PASS** | `crates/harbor-widget/src/widgets/text_label.rs`, `crates/harbor-app/src/quick_save.rs` |
| **E8** | Return to edit preserves draft; cancel writes nothing; focus restored to terminal | **PASS** | `crates/harbor-app/src/quick_save_tests.rs` |
| **E9** | Successful persistence closes popup; conflict/failure displays error and retains draft | **PASS** | `crates/harbor-app/src/quick_save_tests.rs`, `src/profile_save/tests.rs` |
| **E10** | Reusable Checkbox, SegmentedControl, TextLabel::wrap() independent of Harbor state | **PASS** | `crates/harbor-widget/tests/selection_controls.rs`, `crates/harbor-widget/tests/flex_layout.rs` |
| **E11** | Review preserves palette glass transparency and input contrast without dark scrim | **PASS** | `crates/harbor-app/src/palette_render_tests/gpu.rs` |
| **E12** | Distinct reading blocks, heading type scale, and restrained dividers | **PASS** | `crates/harbor-app/src/quick_save_tests.rs` |
| **E13** | Constrained window: field errors stay beside controls; save failure near actions | **PASS** | `crates/harbor-app/src/quick_save_tests.rs` |
| **E14** | Action row: Cancel, Return to edit, Save left-to-right, wrapping at narrow widths | **PASS** | `crates/harbor-app/src/quick_save_tests.rs`, `crates/harbor-widget/tests/flex_layout.rs` |

## Explicit Exclusions and Known Deferred Scope

1. **Virtual environment activation (venv, Conda, Poetry)**: Deferred from current delivery scope per ADR 0110.
2. **Multi-layer nested environments**: Nested combinations (e.g. SSH inside WSL) are deferred.
3. **Live interactive remote SSH testing**: Automated test fixtures use synthetic local processes and loopback SSH aliases; live network SSH authentication is not executed in automated CI.
4. **Desktop physical window manager focus & live IME**: Mounted component and ConPTY adapter tests pass; OS-level physical multi-window focus transitions remain verified via unit/adapter coverage.

## Summary

The core contracts, schema, validation, two-level chooser, guided shell integration, in-window review UI, conflict-safe TOML writing, and environment-aware relaunch templates meet the definition of done established in Spec 0052, Spec 0053, and issue #157.
