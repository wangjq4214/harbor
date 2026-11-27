# Diagnostics, Crash Reporting, and Windows Release Readiness

**Status:** Proposed
**Date:** 2026-11-27

## Context

GitHub Issue [#167](https://github.com/wangjq4214/harbor/issues/167) (`[N16] Diagnostics, Windows acceptance, and release packaging`) requires actionable configuration, shell-launch, and process-exit diagnostics, bounded crash/fatal log retention, a reproducible diagnostic capture path that excludes terminal contents and secrets by default, resilience coverage across failure modes, configuration migration policy, parser fuzz evidence, and scoped Windows Preview versus Daily-use release acceptance.

Currently in the repository:

1. `src/main.rs` initializes `tracing_appender::rolling::never` into `harbor.log` (launch working directory in debug builds, `~/.harbor/` in release builds) without size rotation, retention bounds, or graceful fallback if the log directory cannot be created, and release builds set `windows_subsystem = "windows"` without a panic hook or fatal user-visible feedback when startup or rendering fails before or without an active UI window.
2. `harbor-pty` exposes only a boolean `process_exited` check rather than capturing the shell exit code, and `src/main.rs` has no headless diagnostic CLI entry point.
3. [Spec 0023](../spec/0023-windows-msi-and-portable-release.md) and `.github/workflows/parser-fuzz.yml` establish the MSI/portable ZIP packaging contract and parser-fuzz CI workflow, while [ADR 0057](./0057-windows-installation-and-portable-isolation.md) defines `harbor.portable` configuration isolation (`docs/app/releases.md` and `docs/app/troubleshooting.md` keep release logs in `~/.harbor/`).

Four material design choices were evaluated and settled with the owner:

- **Non-zero shell exit UX:** Keeping crashed panes open in a frozen state was rejected in favor of preserving the existing automatic pane/tab close lifecycle while logging structured exit details and emitting an actionable 3-second floating notice (`NotificationCard`, [ADR 0071](./0071-three-second-notifications-including-errors.md)) when the main window remains open.
- **Fatal/panic/startup feedback:** Log-only silent termination in GUI release builds was rejected because users see nothing when Harbor fails early or on fatal GPU faults; persisting bounded `crash-*.log` files plus showing an interactive native Windows message box (suppressible in automated tests/non-interactive runs) was chosen.
- **Portable log location:** Relocating release logs and crash records beside `harbor.exe` under `harbor.portable` was rejected to preserve the existing contract in `docs/app/releases.md` and `docs/app/troubleshooting.md` (`harbor.portable` isolates `config.toml` only).
- **Diagnostic bundle and config migration:** A single-file-only report was rejected in favor of a timestamped redacted diagnostic directory (`summary.json` plus bounded tail copies of logs/crash records) shared between a Command Palette command and `harbor --diagnostics`. Automatic `schema_version` rewriting of user `config.toml` was rejected in favor of non-mutating backward compatibility, startup recovery with unknown-key warnings, and explicit migration notes.

## Decision

1. **Bounded logging and crash retention:** Replace unbounded `harbor.log` appending with size-bounded rolling log rotation and a bounded count of retained `crash-*.log` files in the existing log directory (debug: launch cwd; release: `~/.harbor/`, unchanged by `harbor.portable`), handling log directory/file creation failures without panicking.
2. **Interactive fatal feedback with test suppression:** Install a panic hook and fatal startup/GPU fault reporter that persist a redacted crash record before termination and, when no usable Harbor UI window is available in an interactive Windows session, display a native Windows `MessageBoxW` naming the failure category, crash/log file path, and noting that terminal contents are excluded. Provide an explicit suppression mechanism for automated tests and non-interactive sessions.
3. **Actionable shell launch and non-zero exit diagnostics:** Extend the PTY boundary to surface the shell exit code. Keep Harbor's existing automatic pane/tab close behavior on exit: exit code `0` closes quietly, while non-zero exit codes record a structured redacted log event and emit an actionable 3-second floating notice ([ADR 0071](./0071-three-second-notifications-including-errors.md)) if the main window remains open.
4. **Shared redacted diagnostic bundle (`CLI` + `Command Palette`):** Provide one host-owned collector invoked by both `harbor --diagnostics [--output <dir>]` (running headlessly without creating a window, GPU device, or PTY) and a Command Palette `Export Diagnostics` action. Emit a timestamped directory containing `summary.json` (Harbor version/build profile, Windows build/arch, bundled ConPTY `VERSION` availability, GPU adapter/backend summary, selected config path/`portable` flag and sanitized validation diagnostics, and log/crash file inventory) plus bounded tail copies of recent logs and crash reports. Exclude terminal screen/scrollback text, clipboard contents, environment variable values, raw `config.toml` contents, and full custom shell arguments by default.
5. **Non-mutating configuration migration and tiered Windows release gates:** Never automatically rewrite user `config.toml`; retain backward-compatible parsing, startup recovery with unknown-key warnings, and explicit upgrade/migration documentation for breaking changes. Separate Windows release readiness into **Preview** (delivered core, diagnostics, packaging, and explicit exclusions) and **Daily-use** (Preview plus [Spec 0023](../spec/0023-windows-msi-and-portable-release.md) ordinary-user/UAC install/lifecycle/portable checks, live stable-tag publication/download smoke, executed `parser-fuzz.yml` corpus/campaign evidence, and Windows dogfood records under `docs/verification/`).

## Consequences

- Support reports and fatal failures become diagnosable in GUI release builds without leaking terminal contents, clipboard data, or secrets by default.
- Existing `harbor.portable` configuration isolation and automatic pane/tab exit cleanup semantics remain intact without breaking changes.
- Achieving Windows Daily-use sign-off requires executing and archiving real Windows installer/portable, parser fuzz, failure-mode resilience, and dogfood verification artifacts rather than relying on CI workflow definitions alone.
