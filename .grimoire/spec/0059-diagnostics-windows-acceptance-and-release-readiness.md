# Diagnostics, Windows Acceptance, and Release Readiness

**Spec ID:** 0059
**Status:** Proposed
**Date:** 2026-11-27
**Issue:** [#167](https://github.com/wangjq4214/harbor/issues/167)
**Sources:**
- GitHub issue [#167](https://github.com/wangjq4214/harbor/issues/167) (`[N16] Diagnostics, Windows acceptance, and release packaging`) and refinement decisions (2026-11-27)
- [ADR 0113](../adr/0113-diagnostics-crash-reporting-and-release-readiness.md)
- [Spec 0023](./0023-windows-msi-and-portable-release.md) (Windows MSI and Portable Release)
- [ADR 0057](../adr/0057-windows-installation-and-portable-isolation.md), [ADR 0062](../adr/0062-watched-reload-and-new-session-launch-settings.md), and [ADR 0071](../adr/0071-three-second-notifications-including-errors.md)
- [Releases](../../docs/app/releases.md), [Troubleshooting](../../docs/app/troubleshooting.md), [Testing](../../docs/developer/testing.md), and [Parser Fuzz Workflow](../../.github/workflows/parser-fuzz.yml)

## Requirements

### R1 — Bounded Rolling Logs, Crash Retention, and Fatal Feedback

- Replace unbounded `harbor.log` appending with size-bounded rolling log rotation in the existing log directory (`cwd` in debug builds; `~/.harbor/` in release builds, unaffected by `harbor.portable`).
- Handle log directory or file creation failures gracefully without panicking so Harbor can still start or report fatal errors.
- Install a process-wide panic hook and route fatal startup (`ShellError`, `HostStartupError`) and fatal frame/shared-GPU errors (`HostFrameOutcome::Fatal`) through a shared crash/fatal reporter that persists a bounded, redacted `crash-*.log` file and retains at most a bounded number of crash files (pruning oldest files first).
- When a panic, fatal startup error, or fatal GPU error terminates Harbor without a usable UI window in an interactive Windows session, present a native Windows `MessageBoxW` summarizing the failure category, the crash/log path, and stating that terminal contents are excluded by default. Provide an explicit suppression mechanism (environment variable or test mode) for automated tests and non-interactive sessions.

### R2 — Actionable Configuration, Shell-Launch, and Non-Zero Process-Exit Diagnostics

- Retain actionable startup/reload configuration diagnostics (selected configuration path, validation errors, and 3-second floating notices per [ADR 0071](../adr/0071-three-second-notifications-including-errors.md)) and shell-launch failure reporting (bundled `conpty/` integrity errors, missing `shell.directory`, configured program fallback vs named-profile failure).
- Extend the PTY boundary (`harbor-pty` -> `harbor-terminal` -> host coordination) to observe and surface the child process exit code when a shell process exits.
- Preserve Harbor's existing automatic pane/tab close lifecycle on shell exit:
  - Normal exit (`0`) closes the pane/tab quietly without emitting an error notice.
  - Non-zero exit logs a structured redacted event (session/tab metadata, shell label, exit code; no environment values, full custom arguments, or terminal contents) and, if the main window remains open after the pane/tab closes, emits an actionable 3-second floating error notice reporting the exit code and pointing to logs/troubleshooting.

### R3 — Redacted Diagnostic Bundle Export (CLI and Command Palette)

- Provide a single host-owned diagnostic collector accessible through two entry points:
  1. **Headless CLI:** `harbor --diagnostics` (with optional `--output <dir>`), generating the bundle directly without opening a window, initializing a GPU surface, or spawning a PTY.
  2. **Command Palette:** an `Export Diagnostics` `AppCommand` executing on the UI/host boundary without blocking PTY I/O, reporting completion or failure via a 3-second floating notice including the exported path on success.
- Output a timestamped diagnostic directory containing:
  - `summary.json`: Harbor version and build profile (`debug`/`release`), OS/Windows build and architecture, bundled `conpty/VERSION` presence/version status, GPU adapter/backend summary (when initialized or probed safely), selected configuration path, `portable` mode flag, sanitized configuration validation diagnostics, and an inventory of included log/crash files.
  - Bounded tail copies of recent `harbor.log` (and rotated siblings if needed) and recent `crash-*.log` files.
- **Strict default redaction:** Exclude terminal screen and scrollback contents, OS clipboard contents, environment variable values, raw `config.toml` contents, and full custom shell arguments from `summary.json` and exported log/crash records by default.

### R4 — Failure-Mode, Resilience, and Redaction Test Coverage

- Maintain focused automated and fault-injection tests covering:
  - Log rotation size bounds, crash file retention bounds, and unwritable log-directory fallback without panics.
  - Panic/fatal record generation and interactive-dialog suppression under automated test runs.
  - Child exit code propagation (`0` quiet close vs non-zero structured log and notice when window remains open; final tab exit behavior).
  - Sustained high-volume PTY output, bounded history/resource exhaustion safety, presentation/shared-GPU fault handling, and clean shutdown under active panes/tabs.
  - Headless `--diagnostics` and Command Palette bundle generation plus strict redaction assertions confirming that sentinel terminal output, clipboard text, environment secrets, and raw TOML secrets never appear in exported artifacts.

### R5 — Non-Mutating Configuration Migration, Parser Fuzz Evidence, and Windows Release Tiers

- **Non-mutating configuration migration policy:** Harbor never automatically rewrites user `config.toml` files on upgrade. Backward-compatible schema evolution, startup recovery with unknown-key warnings, and documented manual migration notes in `docs/app/configuration.md` / release notes govern breaking changes.
- **Parser fuzz evidence:** Execute and archive retrievable evidence for `.github/workflows/parser-fuzz.yml` (`corpus-replay` and bounded `scheduled-campaign` runs) alongside `windows-quality-gates.yml` passes, distinguishing configured workflow definitions from executed results under `docs/verification/`.
- **Tiered Windows release acceptance:** Define explicit release readiness criteria and exclusions in `docs/app/releases.md` and verification records under `docs/verification/`:
  - **Windows Preview:** Delivered core terminal/reflow/Unicode, live config reload, search, command palette, named profiles/quick save, panes, bounded diagnostics/export, and signed-off local/CI MSI and portable ZIP builds from [Spec 0023](./0023-windows-msi-and-portable-release.md); must explicitly document exclusions (Kitty keyboard/graphics protocols, full N09 shell-integration workflows, native Unix PTY, unsigned binaries/SmartScreen prompts).
  - **Windows Daily-use:** All Preview requirements plus completed [Spec 0023](./0023-windows-msi-and-portable-release.md) ordinary-user/UAC install, same-scope upgrade, uninstall, opposite-scope block, portable ZIP isolation checks, live stable-tag publication and downloaded-asset smoke, executed parser fuzz evidence, failure-mode resilience passes, and representative Windows dogfood records.

## Solution

A small host diagnostics module in the binary crate (`src/diagnostics.rs` or cohesive submodule) centralizes size-bounded rolling `harbor.log` initialization, `crash-*.log` persistence, panic hook registration, native Windows `MessageBoxW` fatal presentation (gated by interactive/non-suppressed checks), and the redacted diagnostic bundle collector. `src/main.rs` parses `--diagnostics` / `--output <dir>` before creating the event loop so diagnostic export succeeds even when window/GPU/PTY startup is broken.

In `crates/harbor-pty`, `PtyControl` queries `GetExitCodeProcess` (while retaining non-blocking exit observation) and exposes an exit status (`Option<u32>`) through `harbor-terminal` (`TerminalIo` / `Terminal`) to `src/tab_coordinator.rs`, which logs non-zero exits and enqueues a 3-second `NotificationCard` notice if the main window remains open. `crates/harbor-app` adds `AppCommand::ExportDiagnostics` to the shared command registry and palette so UI-triggered exports invoke the same host collector without duplicating redaction or bundle assembly. Release tier checklists, migration policy, and verification/fuzz/dogfood evidence live in `docs/app/` and `docs/verification/`.

### Seams

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Logging & crash reporter | `src/main.rs` / `src/shell.rs` -> host diagnostics | Log directory resolution (`cwd` vs `~/.harbor/`), interactive suppression flag | Size-bounded rolling `harbor.log`, panic hook, count-bounded `crash-*.log`, gated native `MessageBoxW` |
| Diagnostic bundle collector | `src/main.rs` CLI & `AppCommand::ExportDiagnostics` -> host diagnostics | Optional `--output <dir>`, config path/diagnostics, optional initialized GPU summary | Timestamped redacted directory (`summary.json` + bounded log/crash tails) |
| Exit code observation | `harbor-pty` -> `harbor-terminal` -> `src/tab_coordinator.rs` | Observed exited child handle | `Option<u32>` exit code driving quiet `0` close vs structured log + 3-second notice |
| Command & notification UI | `harbor-app` (`AppCommand`, `NotificationCard`) <-> `src/shell.rs` / `src/tab_coordinator.rs` | Palette command request / non-zero exit event | Command execution completion and auto-expiring floating feedback |
| Release & verification gates | `docs/app/` & `.github/workflows/` -> `docs/verification/` | Executed Windows MSI/ZIP, fuzz, resilience, and dogfood runs | Retrievable Preview / Daily-use release evidence and documented exclusions |

## End-to-End Tests

### E1 — Bounded Log Rotation and Unwritable Log Directory
- **Given:** Harbor starts with an active log target exceeding its single-file rotation cap, and separately with an unwritable or blocked log directory path.
- **When:** Tracing events are emitted continuously past the rotation threshold, or startup initializes logging on the unwritable path.
- **Then:** Rotated log files stay within the configured file-size and file-count caps, and an unwritable log path degrades safely without panicking or aborting diagnostic error reporting.

### E2 — Panic Hook, Crash Record Retention, and Suppressed vs Interactive Fatal Feedback
- **Given:** More than the maximum retained crash count of panic/fatal events occur, and separately a startup failure or `HostFrameOutcome::Fatal` occurs in automated test mode versus interactive Windows launch.
- **When:** The crash reporter records the failure.
- **Then:** Only the newest bounded `crash-*.log` files are retained, each excludes terminal/clipboard/env secrets, automated test mode suppresses native UI dialogs while persisting the record, and interactive failure without a usable UI window presents `MessageBoxW` naming the failure and log path.

### E3 — Shell Exit Code Observation (`0` vs Non-Zero)
- **Given:** An open Harbor window with two active panes/tabs, where one child shell exits with code `0` and another exits with a non-zero exit code (e.g., `42`).
- **When:** Each child process exits.
- **Then:** Both panes/tabs follow Harbor's automatic close lifecycle; exit `0` closes quietly without an error notice, while exit `42` writes a redacted structured log entry and displays a 3-second floating error notice showing exit code `42`.

### E4 — Headless CLI Diagnostic Export (`harbor --diagnostics`)
- **Given:** A broken or unstarted Harbor environment (no GPU or PTY required) containing config diagnostics, `harbor.log`, and a `crash-*.log` file.
- **When:** Running `harbor --diagnostics --output <temp_dir>` on Windows.
- **Then:** The command exits cleanly without creating a GUI window or spawning a shell, producing a timestamped directory with valid `summary.json` and bounded tail copies of logs/crash records.

### E5 — Command Palette Diagnostic Export and Strict Default Redaction
- **Given:** A running Harbor session containing distinctive sentinel text in terminal scrollback, sentinel text on the clipboard, a secret environment variable (`HARBOR_SECRET_TEST=top_secret_token`), and custom settings in `config.toml`.
- **When:** The user invokes `Export Diagnostics` from the Command Palette.
- **Then:** A 3-second floating notice reports the generated directory path; `summary.json` and copied log/crash tails contain version/OS/ConPTY/config-path metadata and none of the terminal, clipboard, environment-value, or raw `config.toml` sentinels.

### E6 — Failure-Mode and Resilience Suite
- **Given:** Automated fault/stress fixtures simulating sustained high-rate PTY output, bounded history capacity pressure, child crash/disconnect during active output or resize, and shared-GPU presentation faults.
- **When:** Executing the resilience suite via `scripts/dev/test_cargo.ps1` and `scripts/dev/test_pty.ps1`.
- **Then:** Bounded memory/history limits hold, no panics or hung shutdowns occur, and shared-GPU faults trigger clean exit/diagnostic persistence.

### E7 — Non-Mutating Configuration Upgrade Compatibility
- **Given:** User `config.toml` files (both home and `harbor.portable`) containing older valid settings alongside an unknown or deprecated key, with a recorded SHA-256 hash prior to launch.
- **When:** Harbor starts and validates configuration.
- **Then:** Harbor never modifies or rewrites `config.toml` (hash remains identical), emits unknown-key warnings, preserves valid settings, and exposes troubleshooting/migration guidance.

### E8 — Parser Fuzz Evidence and Tiered Windows Release Acceptance
- **Given:** Release-profile static-CRT x64 MSI and portable ZIP packages built per [Spec 0023](./0023-windows-msi-and-portable-release.md) and `.github/workflows/parser-fuzz.yml`.
- **When:** Running the Preview and Daily-use acceptance checklists (ordinary-user/UAC install, upgrade, uninstall, scope block, portable isolation, live stable-tag download smoke, parser fuzz replay/campaign, and dogfood verification).
- **Then:** `docs/verification/` stores dated, revision-stamped `PASS`/`FAIL`/`NOT RUN`/`BLOCKED` evidence for every gate, and `docs/app/releases.md` clearly distinguishes Preview vs Daily-use scope and exclusions.

## Decisions

### Settled Architectural and Behavioral Decisions
- **Keep automatic pane/tab close on non-zero exit:** Confirmed by owner (2026-11-27); exit `0` closes quietly, non-zero logs a redacted structured event and emits a 3-second floating notice if the window remains open ([ADR 0113](../adr/0113-diagnostics-crash-reporting-and-release-readiness.md)).
- **Persist crash records and show suppressible native `MessageBoxW` on interactive fatal failures:** Confirmed by owner (2026-11-27); prevents silent release-mode crashes when no UI window is usable.
- **Keep portable log location unchanged (`~/.harbor/` in release, `cwd` in debug):** Confirmed by owner (2026-11-27); `harbor.portable` continues to isolate `config.toml` only.
- **Shared redacted diagnostic directory via CLI (`harbor --diagnostics`) and Command Palette (`Export Diagnostics`):** Confirmed by owner (2026-11-27).
- **Non-mutating configuration migration and Preview vs Daily-use release tiers:** Confirmed by owner (2026-11-27).

### Explicit Reversible Tuning Assumptions (Implementation Defaults)
- **Log rotation thresholds:** Rotate `harbor.log` at `5 MiB` per file keeping up to `3` rotated files (`~15 MiB` upper bound), and retain at most `10` `crash-*.log` files.
- **Diagnostic bundle tail cap & default destination:** Include up to `256 KiB` tail per log/crash file in exported bundles; when `--output` is omitted (or invoked from the Command Palette), default to `<log_dir>/diagnostics/harbor-diagnostics-<utc-timestamp>/`.
- **Suppression switch:** Suppress native `MessageBoxW` dialogs when `HARBOR_SUPPRESS_FATAL_DIALOG=1` (or under `#[cfg(test)]`).

## Test Plan

- **Unit & integration tests (`scripts/dev/test_cargo.ps1`):**
  - Log rotation, retention pruning, unwritable-directory fallback, panic/crash payload formatting, redaction filters, CLI argument parsing (`--diagnostics`, `--output`), and `summary.json` schema validation.
  - `harbor-app` tests for `AppCommand::ExportDiagnostics` registration, palette visibility/availability, and completion notices.
- **ConPTY & GPU fault tests (`scripts/dev/test_pty.ps1`, `HARBOR_TEST_GPU=dx12-warp`):**
  - Real Windows child exit code `0` vs non-zero exit propagation through ConPTY, sustained output/shutdown stress, and `HostFrameOutcome::Fatal` handling.
- **CI & manual Windows verification (`docs/verification/`):**
  - Executed `parser-fuzz.yml` (`corpus-replay` and `scheduled-campaign`), `windows-quality-gates.yml`, [Spec 0023](./0023-windows-msi-and-portable-release.md) ordinary-user/UAC MSI & portable ZIP scenarios, live tag download smoke, and Windows daily-use dogfood logs.

## Out of Scope

- Automatic background telemetry or crash upload to external servers.
- Keeping crashed panes open in a frozen state or changing automatic shell-exit close semantics.
- Relocating release logs/crash dumps into `harbor.portable` application directories.
- Adding a `schema_version` field or automatically rewriting user `config.toml` files.
- Code signing / SmartScreen reputation acquisition or blocking scoped Windows releases on Kitty protocols, full N09 shell workflows, or native Unix PTY support.
