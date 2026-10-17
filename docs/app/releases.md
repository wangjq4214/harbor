# Windows releases and updates

[Application guide](README.md) · [Getting started](getting-started.md) · [Configuration](configuration.md)

## Choose a format

Windows packages use these names:

- `harbor-<version>-windows-x64.msi`
- `harbor-<version>-windows-x64-portable.zip`

Check the project's release listing for available downloads; these names alone do not mean a release has been published. Packages contain an x64 application, not native ARM64 or x86 application builds. Packages are unsigned and may trigger unknown-publisher or SmartScreen warnings; verify their origin before running them.

### MSI installation and upgrades

The installer offers current-user installation (default, in a user-owned directory) or all-users installation (Program Files, requiring administrator authorization). It adds Start Menu and uninstall entries but does not change PATH by default. Settings use the launching user's `~/.harbor/config.toml`.

For an upgrade, use a newer stable MSI in the **same scope**. To switch scope, uninstall first, then install in the desired scope; an opposite-scope installation can block installation. Uninstall preserves user configuration. Back up your configuration before changing versions.

### Portable ZIP

Extract the complete ZIP into a directory you control and launch `harbor.exe`. The adjacent `harbor.portable` file selects adjacent `config.toml`, regardless of the working directory. To customize, copy the supplied `config.example.toml` to `config.toml` and edit it. Harbor does not create or overwrite this file. Missing or invalid portable settings use [startup recovery](configuration.md#startup-versus-reload), **never home-configuration fallback**.

Move the whole directory, including configuration, marker, and runtime, to carry settings. Restart Harbor after adding or removing the marker. To update, close Harbor, extract the new complete package, and preserve your chosen `config.toml`; do not mix runtime DLL or host versions.

Portable means **configuration portability**, not a no-trace sandbox: release logs still use `~/.harbor/harbor.log`, and child shells may keep histories or other data elsewhere.

## Required runtime payload

Keep `harbor.exe` and its complete adjacent `conpty/` directory together:

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

The ARM64 and x86 hosts allow native-host selection under emulation; they do not change the application's architecture. Both formats also include the Harbor license and example configuration; the ZIP adds the portable marker. Harbor loads the bundled runtime relative to the executable, not from PATH or the Windows inbox runtime. See the [runtime restoration guide](../../third_party/conpty/README.md).

## Build a package locally

From a Windows checkout:

```powershell
./scripts/packaging/package_windows.ps1
```

Prerequisites: PowerShell 5.1+, stable Rust with `x86_64-pc-windows-msvc`, Visual Studio C++ build tools, and network access for dependencies. MSI additionally needs the .NET 8+ SDK and NuGet access for WiX tooling. ZIP-only packaging does not need WiX or .NET:

```powershell
./scripts/packaging/package_windows.ps1 -Format Zip
```

Output goes to `target/dist/` by default. Use `-Format Msi` for MSI only or `-OutputDirectory <path>` to choose another output directory. Local packaging does not need GitHub credentials.
