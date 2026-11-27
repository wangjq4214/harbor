# Redacted Diagnostic Bundle Export via CLI and Command Palette

**Ticket ID:** T0003
**Source:** [Spec 0059](../../spec/0059-diagnostics-windows-acceptance-and-release-readiness.md) (R3, E4, E5) and [ADR 0113](../../adr/0113-diagnostics-crash-reporting-and-release-readiness.md)
**Status:** Open

## Goal

Provide one host-owned, strictly redacted diagnostic bundle collector reachable both headlessly via `harbor --diagnostics [--output <dir>]` and interactively from the Command Palette (`Export Diagnostics`).

## Affected Surfaces

- Host diagnostics module (bundle collector, `summary.json` serialization, bounded log/crash tail copying, redaction guarantees)
- `src/main.rs` (early CLI argument parsing for `--diagnostics` and optional `--output <dir>` before creating `EventLoop`, GPU, or PTY)
- `crates/harbor-app/src/command.rs` and palette command catalog (`AppCommand::ExportDiagnostics`)
- `src/tab_coordinator.rs` / `src/shell.rs` (dispatch `ExportDiagnostics` without blocking PTY I/O, collect optional initialized GPU summary, and emit 3-second notice with output path or error)
- [Commands](../../../docs/app/commands.md) and [Troubleshooting](../../../docs/app/troubleshooting.md) (document `--diagnostics` and `Export Diagnostics`)

## Approach

- Parse `--diagnostics` (with optional `--output <dir>`) in `src/main.rs` prior to `EventLoop` / `Shell::new` so bundle generation works even when window creation, DirectWrite, GPU, or ConPTY fails.
- Build a timestamped output directory (`<output>/harbor-diagnostics-<utc-timestamp>/`, defaulting to `<log_dir>/diagnostics/`) containing:
  - `summary.json`: Harbor version/profile, Windows OS build/arch, bundled `conpty/VERSION` availability and version string, GPU adapter/backend info (when available or safely probed), selected `config.toml` path, `portable` flag, sanitized startup/reload config diagnostics, and log/crash file inventory.
  - Tail copies (capped at `256 KiB` per file) of recent `harbor.log*` and `crash-*.log` files.
- Enforce strict default redaction: never include terminal buffers, clipboard contents, environment variable values, raw `config.toml` contents, or full custom shell arguments.
- Register `AppCommand::ExportDiagnostics` in `harbor-app` so Command Palette triggers the exact same collector and posts a 3-second notification showing the created bundle path.

## Dependencies and Coordination

- **Blocked by:** T0001 (requires rolling `harbor.log*` and `crash-*.log` directory/file contracts).
- **Blocks:** T0004 (redaction and export resilience tests), T0005 (release acceptance).
- **Coordination risks:** Shared files `src/main.rs` and host diagnostics module with T0001; execute after T0001 establishes log/crash paths.

## Acceptance

- [ ] `harbor --diagnostics --output <dir>` runs headlessly without opening a window or spawning a PTY and writes a valid `summary.json` plus bounded log/crash tail copies.
- [ ] Invoking `Export Diagnostics` from the Command Palette writes the same bundle structure without freezing terminal output and shows a 3-second floating notice with the destination path.
- [ ] Exported `summary.json` and bundle files exclude terminal text, clipboard data, environment variable values, and raw `config.toml` text by default.
- [ ] `python scripts/docs/check_docs.py`, `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`, and focused CLI/palette/redaction tests pass.

## Out of Scope

- Remote crash/telemetry upload or user-configurable full-content unredacted dumps.
- Exporting screenshots or live terminal scrollback captures.
