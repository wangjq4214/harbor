# Windows MSI and Portable Release

**Spec ID:** 0023 (distribution; distinct from manual reload 0023)
**Status:** Accepted (owner-confirmed)
**Sources:** User-selected x64/selectable-scope MSI/full portable ZIP/local and tag publishing; [ADR 0057](../adr/0057-windows-installation-and-portable-isolation.md), [0058](../adr/0058-release-only-static-crt.md), [ConPTY runtime contract](../../third_party/conpty/README.md).

## Requirements

### R1. Common Release Payload and Local Packaging
Provide a documented Windows local entry point producing x64 MSI and portable ZIP for one version from the same release-build application/runtime payload, without GitHub credentials. Asset identity exposes version/architecture before installation. Both contain:
```text
harbor.exe
conpty/
  conpty.dll
  x64/OpenConsole.exe
  arm64/OpenConsole.exe
  x86/OpenConsole.exe
  LICENSE
  VERSION
```
Preserve pinned matching DLL/hosts, version/license and all host architectures (native-OS selection includes emulation). Validate payload availability before success; load executable-relative only, with explicit missing/incomplete errors, never inbox/cwd/PATH fallback. Receiving machines need neither Cargo, checkout nor third_party.
Use release-only x64 MSVC static CRT; preserve development linkage. Scope compiler environment changes to the build and restore caller flags, respecting encoded-rustflags precedence/separator. Inspect actual PE imports, including `-SkipBuild`, rejecting external versioned VC runtime imports; no app-local CRT or external runtime prerequisite. A dependency's Windows 10 build 17763 baseline is not a Harbor OS-support matrix.

### R2. MSI Installation and Lifecycle
One MSI offers current-user (user-owned directory, normally no elevation) or all-users (Program Files, administrator authorization) scope, matching Start Menu launch and registered uninstall entries. No default PATH modification; installed settings remain `~/.harbor/config.toml`; no portable marker.
Support same-scope stable upgrades preserving user settings and replacing executable/runtime together. Uninstall removes owned files/runtime/shortcuts/registration, not user configuration.
Block a visible opposite-scope installation with an actionable explicit-uninstall requirement; no automatic migration/removal/config copying/second registration. Document that detection covers installations visible to the current user, not every other account's private installation.

### R3. Full Portable ZIP
An unambiguous exe-adjacent marker selects portable mode even on direct launch; load exe-adjacent `config.toml`, never cwd/home settings. Startup must not create/overwrite config or read/write home configuration. Missing/unreadable/malformed documents use existing startup defaults/diagnostics without home fallback. Document placing/editing the repository example beside the exe.
Moving the complete marked directory, configuration and runtime carries operation/settings. Read-only readable config must load without writes; no MSI registration, installation or elevation required. This isolates configuration, not arbitrary shell histories/behavior, OS caches or all logging. Separate configuration specs govern reload; no editor is introduced.

### R4. Stable Version Identity and GitHub Release
Local packaging and tag-triggered Actions share one payload/file-list/check contract and pinned sources. Accept only stable `vMAJOR.MINOR.PATCH` tags matching root Cargo package numeric version. Reject mismatch or MSI-unrepresentable numbers; never truncate/remap/relabel. Build the tag's source, run required build/payload/package checks before publishing both verified matching assets to the corresponding Release; failed checks cannot publish an accepted artifact set.
Artifacts are initially unsigned. Document untrusted-publisher/SmartScreen prompts without bypass/reputation claims; no signing service/credentials prerequisite. No prerelease mapping.

### R5. Consumer Evidence and Documentation
Document local prerequisites/commands, x64 selection, scopes/custom directory/lifecycle/switching visibility, portable marker/config isolation/example, runtime layout, stable versions and unsigned limits. Distinguish requirements, implementation and verification. Real Windows installation/direct-extracted launch and shell command/response are required, not merely archives/MSI compilation.
Follow validation policy, preserving PR gates. Record revision/tag/version/profile, Windows, scope, steps, expected/observed results, artifacts and PASS/FAIL/NOT RUN/BLOCKED. Use two version fixtures and isolated scope environments; compare configuration and payload hashes before/after. Version-fixture installer mechanics do not prove historical application migration. YAML/local mocks are not live Release or downloaded-asset evidence.

## End-to-End Acceptance
| ID | Scenario → required outcome |
| --- | --- |
| A1 | Restore pinned runtime and package locally → both matching x64 formats/common bytes, no publishing credentials. |
| A2 | Non-elevated current-user install; Start Menu launch from unrelated cwd → scoped entries, real bundled-ConPTY shell command/response. |
| A3 | Authorized machine install; launch as ordinary user → Program Files/machine entries, launching user's home settings, interactive shell. |
| A4 | Older stable → newer MSI in each same scope with distinctive config → only intended upgrade remains, matching working runtime, unchanged settings. |
| A5 | Uninstall each scope then explicitly install opposite scope → owned files/entries removed, config preserved, selected replacement works. |
| A6 | Visible opposite-scope attempts in both directions → actionable block, no automatic removal/migration/second installation; visibility limit documented. |
| A7 | Extract/direct-launch ZIP from unrelated cwd with distinct home/portable settings → marked exe-adjacent settings/runtime, unchanged home config. |
| A8 | Move complete portable directory to spaces/non-ASCII path → settings/runtime resolve, actual shell round trip. |
| A9 | Missing, unreadable, malformed portable config with distinctive valid home config → startup defaults/diagnostics, no home substitution or config creation/overwrite; read-only readable config loads. |
| A10 | Matching stable tag and successful CI checks → corresponding Release has both common-build assets; downloaded MSI/ZIP pass installation/portable smoke (A2/A3/A7/A8). |
| A11 | Mismatch/prerelease/MSI overflow or required payload/runtime/package check failure → actionable rejection, no accepted publication or inbox runtime substitution. |

## Execution Addendum: Final Local Evidence and External Gates
Historical evidence limitations: elevated-token runs did not establish ordinary-user/UAC behavior; the 0.0.9 → 0.1.0 upgrade reused one application binary and proved installer-version mechanics only. Live publication/download smoke and a broad clean-machine matrix were not established; old artifacts have not been reverified.
The intermittent font-replacement GPU comparison remains unexplained, not repaired by reruns.

### Stable Execution Aliases
`0017-windows-msi-and-portable-release`:
| Slice | Surviving contract | Acceptance |
| --- | --- | --- |
| T0001 — portable configuration/ZIP | R1/R3/R5 | A1/A7–A9/A11 |
| T0002 — selectable-scope MSI | R1/R2/R5 | A1–A6/A11 |
| T0003 — stable-tag publication | R1/R4/R5 | A1/A10/A11 plus downloaded A2/A3/A7/A8 |
ZIP includes the portable marker; MSI excludes it. Both formats share one runtime/version/payload contract, and local/CI file lists cannot diverge.

## Exclusions
No native ARM64/x86 application assets, prerelease mapping, automatic scope migration, global private-install enumeration, signing, shell no-trace sandbox or new OS matrix.
