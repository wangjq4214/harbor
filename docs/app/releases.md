# Windows releases and updates

[Application guide](README.md) · [Getting started](getting-started.md) · [Configuration](configuration.md)

## Choose a format

The repository's [packager](../../scripts/packaging/package_windows.ps1) constructs unsigned **x64 application** assets from one release payload:

- `harbor-<version>-windows-x64.msi`
- `harbor-<version>-windows-x64-portable.zip`

These names describe build outputs, not proof that a public release with those assets exists. Check the project's actual release record before downloading. Native ARM64/x86 application packages are not part of this packaging contract; their OpenConsole hosts are still bundled for native-host selection under emulation.

### MSI installation and upgrades

The [installer definition](../../installer/windows/Harbor.wxs) offers current-user (default, user-owned directory) or all-users (Program Files, administrator authorization) installation, with scoped Start Menu and uninstall entries. It does not change PATH by default. Settings use the launching user's `~/.harbor/config.toml`; MSI does not include the portable marker.

Use a newer stable MSI in the **same scope** for an upgrade. Uninstall preserves user configuration. To switch scope, explicitly uninstall first, then install in the desired scope. A visible opposite-scope installation is blocked rather than automatically removed/migrated; this does not enumerate all other accounts' private installations. Back up your configuration before changing versions. Installer-version mechanics alone do not establish application-data compatibility between arbitrary versions.

### Portable ZIP

Extract the complete ZIP into a directory you control and launch `harbor.exe` directly. Its adjacent `harbor.portable` file selects adjacent `config.toml`, regardless of launch cwd. If you want customization, copy the supplied `config.example.toml` to `config.toml` and edit it. Startup does not create or overwrite configuration; missing/invalid portable settings use startup recovery, **never home configuration fallback**.

Move the whole directory, including configuration, marker, and runtime, to carry settings. Readable read-only configuration can load without writes. Restart after changing marker mode. For a manual portable update, close Harbor and use the new complete payload while preserving your chosen `config.toml`; do not mix runtime DLL/host versions or assume overwriting a live executable is supported. This guide provides a manual workflow, not a verified automatic-updater contract.

Portable means **configuration portability**, not a no-trace sandbox: release logs still use `~/.harbor/harbor.log`, and child shells may keep histories or other data elsewhere.

## Required runtime payload

Both formats carry the application and matching ConPTY runtime:

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

The common packaged files also include the Harbor license and example configuration; the ZIP adds the portable marker. The runtime is loaded executable-relative, not from cwd/PATH/inbox fallback. See [ConPTY restoration, pinning, and layout](../../third_party/conpty/README.md). Keep the entire runtime when moving the application.

## Build locally

From a Windows checkout:

```powershell
./scripts/packaging/package_windows.ps1
```

Prerequisites: PowerShell 5.1+, stable Rust with `x86_64-pc-windows-msvc`, Visual Studio C++ build tools, and package-network access. MSI additionally needs the .NET 8+ **SDK** and NuGet access for pinned WiX/UI tooling. ZIP-only packaging (`-Format Zip`) does not need WiX/.NET. No GitHub credentials are needed for local packaging.

The script restores pinned ConPTY, builds `--locked --release` with x64 static CRT, validates the common payload/PE imports, and emits under `target/dist/` by default. `-Format Msi` selects MSI only; `-OutputDirectory <path>` changes output. `-SkipBuild -ReleaseDirectory <path>` requires a matching versioned x64 static-CRT build and pinned payload; it does not relabel an arbitrary executable. Compiler flags are scoped/restored by the packager; ordinary development linkage is unchanged. See [packaging regression tests](../../scripts/tests/test_package_windows.ps1).

## Version and publication policy

[Version validation](../../scripts/packaging/windows_package_version.ps1) accepts stable `MAJOR.MINOR.PATCH`, bounded by MSI fields `255.255.65535`, without truncation/remapping. A supplied release tag must be exactly `v` plus the root [Cargo package version](../../Cargo.toml); prerelease mappings are not supported.

The [tag-triggered workflow](../../.github/workflows/windows-release.yml) uses the shared [Windows quality gates](../../.github/workflows/windows-quality-gates.yml), then downloads checked assets, uploads both to a draft Release, and publishes it. It refuses to replace an already-published Release. Configured automation is not evidence of an actual successful publication or downloaded build.

Initial artifacts are unsigned and may trigger unknown-publisher/SmartScreen prompts. Verify provenance under your organization's policy; no reputation, signing, or prompt-bypass guarantee is offered.

## Verification boundary

See [Current Status](../current-status.md) and [Windows release validation](../validation.md#windows-release-packages) for historical checks and outstanding acceptance. Fresh ordinary-user installation/launch, authorized machine installation followed by ordinary-user launch, applicable upgrades/uninstall/scope/custom-directory scenarios, live stable-tag publication, and downloaded MSI/ZIP smoke are separate gates. A successful local archive build does not prove them. This documentation task builds, installs, publishes, and updates no runtime package.
