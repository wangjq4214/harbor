# Release Tiers, Config Migration Policy, and Fuzz/Windows Acceptance Evidence

**Ticket ID:** T0005
**Source:** [Spec 0059](../../spec/0059-diagnostics-windows-acceptance-and-release-readiness.md) (R5, E7, E8), [Spec 0023](../../spec/0023-windows-msi-and-portable-release.md), [ADR 0113](../../adr/0113-diagnostics-crash-reporting-and-release-readiness.md), and [Parser Fuzz Workflow](../../../.github/workflows/parser-fuzz.yml)
**Status:** Open

## Goal

Establish documented non-mutating configuration migration guidance and Windows Preview vs Daily-use release tiers, and archive retrievable parser-fuzz, packaging, installer/portable, and dogfood verification evidence under `docs/verification/`.

## Affected Surfaces

- [Releases](../../../docs/app/releases.md) (Preview vs Daily-use scope, declared exclusions, upgrade/migration notes)
- [Configuration](../../../docs/app/configuration.md) (non-mutating configuration compatibility and migration policy)
- `docs/verification/` (new verification record(s) for parser fuzz runs, Spec 0023 A1–A11 installer/portable/live-release smoke, failure-mode resilience summary, and representative Windows dogfood observations)
- `crates/harbor-config` tests (characterization test verifying `config.toml` bytes/hash remain untouched on startup with unknown/legacy keys)

## Approach

- Document the non-mutating configuration migration contract in `docs/app/configuration.md` and `docs/app/releases.md`, and add an automated test confirming Harbor never rewrites user `config.toml` during startup recovery or reload.
- Document explicit **Windows Preview** and **Windows Daily-use** release checklists and declared exclusions in `docs/app/releases.md` so release notes never overclaim unshipped packages (Kitty protocols, N09 full shell workflows, native Unix, unsigned packages).
- Execute and record retrievable evidence under `docs/verification/` with revision SHA, Windows build, expected vs observed outcome, and honest `PASS`/`FAIL`/`NOT RUN`/`BLOCKED` tags:
  1. Executed `.github/workflows/parser-fuzz.yml` (`corpus-replay` and `scheduled-campaign`) and `windows-quality-gates.yml` run links/summaries.
  2. [Spec 0023](../../spec/0023-windows-msi-and-portable-release.md) A1–A11 checks: non-elevated current-user MSI, authorized machine-wide MSI + ordinary-user launch, same-scope upgrade with distinct two-version binaries, uninstall, opposite-scope block, portable ZIP isolation/relocation/malformed-config checks, and live stable-tag publication + downloaded-asset smoke.
  3. Representative Windows daily-use dogfood records covering terminal workloads, resize, panes, reload, search, palette, and diagnostic export.

## Dependencies and Coordination

- **Blocked by:** T0001, T0002, T0003, T0004 (release acceptance and dogfood records require completed diagnostic/exit-reporting implementation and passing resilience tests).
- **Blocks:** None (closes #167 acceptance scope once evidenced).
- **Coordination risks:** Follow [Validation](../../../docs/developer/testing.md) and [ADR 0112](../../adr/0112-standardized-repository-file-layout.md) when placing verification records under `docs/verification/`.

## Acceptance

- [ ] `docs/app/configuration.md` and `docs/app/releases.md` clearly state the non-mutating configuration migration policy, Preview vs Daily-use release scopes, and explicit exclusions.
- [ ] An automated test verifies `config.toml` remains byte-identical when loaded with unknown/legacy keys.
- [ ] `docs/verification/` contains linked, revision-stamped evidence covering executed `parser-fuzz.yml` replay/campaign runs, Spec 0023 ordinary-user/UAC MSI & portable ZIP checks, live release asset smoke, resilience suite results, and Windows dogfood records.
- [ ] `python scripts/docs/check_docs.py` and `python scripts/ci/check_layout.py` pass with zero broken links or layout errors.

## Out of Scope

- Code signing infrastructure or SmartScreen reputation claims.
- Automatic `config.toml` rewriting or introducing `schema_version` migrations.
- Implementing Kitty protocols, full N09 workflows, or native Unix PTY support.
