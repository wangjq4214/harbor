# Diagnostics, Windows Acceptance, and Release Readiness

**Source:** [Spec 0059](../../spec/0059-diagnostics-windows-acceptance-and-release-readiness.md), [ADR 0113](../../adr/0113-diagnostics-crash-reporting-and-release-readiness.md), and [Spec 0023](../../spec/0023-windows-msi-and-portable-release.md)
**Ticket folder:** `.grimoire/ticket/0002-diagnostics-windows-acceptance-and-release-readiness/`

## Overview

Implement bounded application logging, panic/fatal crash retention and interactive feedback, shell exit-code diagnostics, a shared redacted diagnostic bundle (`--diagnostics` CLI and Command Palette), failure-mode resilience tests, and the tiered Windows Preview/Daily-use release, migration, parser-fuzz, and verification gates for [#167](https://github.com/wangjq4214/harbor/issues/167).

## Delivery Surfaces

- Root binary host (`src/main.rs`, `src/shell.rs`, `src/tab_coordinator.rs`, and host diagnostics module)
- PTY and terminal session boundary (`crates/harbor-pty/`, `crates/harbor-terminal/`)
- Application command registry and notifications (`crates/harbor-app/`)
- Documentation and verification artifacts (`docs/app/releases.md`, `docs/app/configuration.md`, `docs/app/troubleshooting.md`, `docs/verification/`, `.github/workflows/parser-fuzz.yml`)

## Dependencies

| Producer | Consumer | Why consumer cannot proceed first |
| --- | --- | --- |
| T0001 | T0003 | Diagnostic bundle export collects bounded rolling logs and `crash-*.log` records defined by T0001 |
| T0001 | T0004 | Resilience suite exercises bounded log rotation, crash retention, and fatal-feedback suppression from T0001 |
| T0002 | T0004 | Resilience suite verifies shell non-zero exit-code propagation and notice behavior from T0002 |
| T0003 | T0004 | Resilience and redaction suite validates CLI/Command Palette diagnostic bundles from T0003 |
| T0001, T0002, T0003, T0004 | T0005 | Windows Preview/Daily-use acceptance and dogfood evidence require delivered diagnostics, exit reporting, and resilience verification |

## Coordination

| Tickets | Risk | Strategy |
| --- | --- | --- |
| T0001, T0003 | Shared startup/logging/diagnostics files (`src/main.rs`, host diagnostics module) | Land T0001's log/crash layout and path helpers first so T0003 consumes them cleanly |
| T0001, T0002 | Shared error/notice touchpoints in `src/shell.rs` | Keep T0001 focused on fatal startup/GPU/panic handling and T0002 on `tab_coordinator` shell exit events |

## Recommended Order

1. Start **T0001** and **T0002** in parallel (T0001 owns logging/panic/fatal feedback; T0002 owns `harbor-pty` exit-code observation and `tab_coordinator` notices).
2. Execute **T0003** after **T0001** lands so the shared collector consumes the final rolling-log and crash-file contracts.
3. Execute **T0004** once **T0001–T0003** are in place to verify all failure modes, shutdown stress, and redaction invariants.
4. Complete **T0005** last to record parser-fuzz runs, [Spec 0023](../../spec/0023-windows-msi-and-portable-release.md) ordinary-user/UAC and live-release checks, migration docs, and Preview/Daily-use dogfood evidence.

## Ticket Index

| Ticket | File | Outcome |
| --- | --- | --- |
| T0001 | [T0001-bounded-logging-panic-hook-and-fatal-feedback.md](./T0001-bounded-logging-panic-hook-and-fatal-feedback.md) | Size-bounded rolling logs, count-bounded `crash-*.log` persistence, panic hook, and gated Windows fatal `MessageBoxW` |
| T0002 | [T0002-shell-exit-code-and-actionable-process-diagnostics.md](./T0002-shell-exit-code-and-actionable-process-diagnostics.md) | Child process exit-code observation (`0` quiet close vs non-zero structured log + 3-second floating notice) and launch diagnostics |
| T0003 | [T0003-redacted-diagnostic-bundle-cli-and-palette.md](./T0003-redacted-diagnostic-bundle-cli-and-palette.md) | Shared redacted diagnostic bundle collector exposed via `harbor --diagnostics [--output <dir>]` and Command Palette |
| T0004 | [T0004-failure-mode-and-resilience-test-suite.md](./T0004-failure-mode-and-resilience-test-suite.md) | Automated failure-mode, resource-exhaustion, GPU-fault, shutdown, and redaction tests |
| T0005 | [T0005-release-tiers-migration-policy-and-fuzz-acceptance-evidence.md](./T0005-release-tiers-migration-policy-and-fuzz-acceptance-evidence.md) | Non-mutating config migration docs, executed `parser-fuzz.yml` evidence, and Preview/Daily-use Windows acceptance records |
