# ⚓ Harbor

Harbor is a Windows-first, GPU-accelerated terminal emulator written in Rust with winit, wgpu, a custom VT parser, DirectWrite text support, and a declarative widget runtime.

The priority is a correct, stable daily-use Windows terminal. Native Unix PTY support remains deferred; WSL/SSH compatibility inside the Windows application is a separate concern.

## Current Capabilities

- 🧠 Incremental, bounded VT parsing; screen editing, colors, margins, alternate screen and scrollback.
- 🪟 Windows ConPTY sessions, multiple terminal tabs, selection/copy and scrollback controls.
- ⌨️ Traditional keyboard modes, bracketed paste/confirmation, SGR mouse reporting, focus reporting and IME commit/preedit integration.
- 🔗 Terminal replies and capability queries; OSC titles, working-directory metadata, hyperlinks, default colors and shell markers.
- 🎨 Damage-aware wgpu rendering, DirectWrite font fallback, synchronized-output scheduling and cursor/decorations.
- ⚙️ TOML settings with manual/default-on watched reload and configurable application keybindings.
- ✨ Acrylic/backdrop fallback, rounded widget decorations and a retained desktop widget runtime.

These are implemented scopes, **not a claim that every protocol or Windows release gate is complete**. See [Current Status](docs/current-status.md) for source links and limitations, and the [Protocol Checklist](docs/protocol/checklist.md) for exact coverage.

The main gaps are interactive Windows/performance acceptance for the implemented resize reflow, complete combining/grapheme text handling, combined configuration-reload package acceptance, split panes, search, profiles, a command palette, Kitty protocols, and selected terminal extensions. [The roadmap](docs/roadmap.md) orders the work; [the next-stage plan](docs/next-stage-plan.md) defines its scope, including UI polish and a liquid-glass investigation.

## Build and Run

An operational PTY session currently requires Windows.

```powershell
./scripts/fetch_conpty.ps1
cargo run
```

Run the fetch script once after cloning, and again when the pinned runtime changes.
Windows builds stage the pinned [Microsoft ConPTY runtime](third_party/conpty/README.md)
in `target/<profile>/conpty/`. When distributing or moving `harbor.exe`, include
the complete adjacent `conpty/` directory (DLL, hosts, version and license).
An incomplete bundle is reported as a PTY startup error rather than falling back
to an older Windows console host with incompatible resize behavior.

To regenerate the checked-in Windows App SDK bindings, restore the pinned build-time
[WASDK metadata](third_party/wasdk/README.md) with `./scripts/fetch_wasdk.ps1`.
Normal builds do not require this download.

### Windows Release Packages

Build unsigned x64 MSI and portable ZIP packages from one release build:

```powershell
./scripts/package_windows.ps1
```

Prerequisites: Windows PowerShell 5.1 or newer, stable Rust with the
`x86_64-pc-windows-msvc` target and Visual Studio C++ build tools, the **.NET 8+
SDK** (a runtime alone is insufficient), and access to NuGet. The script restores
the hash-checked ConPTY package, builds with `--locked --release` and static CRT
linkage for the x64 target, restores pinned WiX/UI 5.0.2 tooling under `target/wix/`,
and validates the complete common payload before returning outputs under `target/dist/`:

- `harbor-<version>-windows-x64.msi`
- `harbor-<version>-windows-x64-portable.zip`

Use `-Format Zip` or `-Format Msi` for one format, and `-OutputDirectory <path>`
to choose the output directory. ZIP-only packaging needs no .NET/WiX installation.
Local packaging needs no GitHub credentials.
`-SkipBuild -ReleaseDirectory <path>` reuses an explicit release directory only
after checking its x64 binary ProductVersion, absence of direct external versioned
VC runtime imports, and matching pinned ConPTY runtime. A normal dynamic-CRT build
is rejected; rebuild through the packager rather than relabeling an old binary.
Compiler rustflags are scoped to the packaging invocation and restored afterward;
ordinary development builds keep their existing linkage.

The first MSI page selects current-user or all-users installation and defaults
to the current user. Current-user installation uses a user-owned directory;
all-users installation uses native Program Files and requires administrator
authorization. Both provide scoped Start Menu and uninstall entries. Upgrade in
the same scope; uninstall before switching scopes. A visible opposite-scope
installation is blocked, but this does not enumerate every other account's
private per-user installations. Uninstall preserves `~/.harbor/config.toml`.
PATH is not changed by default.

The ZIP needs no installer or elevation. Extract the entire archive, then launch
`harbor.exe` directly. It includes `harbor.portable` beside the executable; that
marker selects adjacent `config.toml`, independently of the working directory.
Startup, automatic watching and **Ctrl+Shift+R** all use that same path for the
session. Restart Harbor after adding or removing the marker to switch modes.
Copy the supplied `config.example.toml` to `config.toml` and edit it if desired.
Without a config file, Harbor uses defaults; startup never creates or overwrites
it, and portable mode never falls back to home-directory settings. Move the
entire directory—including the marker, config and complete `conpty/` bundle—to
carry the configuration. This is configuration portability, not a no-trace
sandbox: existing release logs still go to `~/.harbor/harbor.log`, and launched
shells may keep their own histories.

Pull requests to `main` run the shared Windows quality workflow, including a
static-CRT release build, both package formats and packaging regression checks.
The tag-triggered release workflow reuses those same gates and accepts only stable
`vMAJOR.MINOR.PATCH` tags matching the root Cargo version. MSI version fields must
fit `255.255.65535`; unrepresentable or mismatched versions fail rather than being
remapped. Only the release workflow uploads both assets to a draft GitHub Release
and publishes it after the checks succeed; PR checks do not publish releases.
An already published Release is not replaced. Initial packages are **unsigned**
and may trigger unknown-publisher or SmartScreen prompts. Native ARM64/x86
application packages and prerelease version mappings are not included; their
OpenConsole hosts still remain bundled.

Package-construction checks do not establish all installation or live publication
acceptance. See [Current Status](docs/current-status.md#windows-distribution) for
the verified scope and [Validation](docs/validation.md#windows-release-packages)
for the remaining Windows lifecycle and downloaded-artifact checks.

## Library Crates

The reusable subsystems have crate-level usage guides:

- [`harbor-widget`](crates/harbor-widget/README.md) — declarative components, state, layout, input, rendering, and optional `winit` hosting.
- [`harbor-terminal`](crates/harbor-terminal/README.md) — ANSI/VT parsing, screen state, PTY I/O, input encoding, scheduling, and wgpu rendering.

Both crates are currently documented as pre-1.0 workspace libraries. Their READMEs show path-based setup, minimal examples, host integration, feature flags, and ownership boundaries.

### Startup Configuration

For installed or unmarked builds, copy [`config.example.toml`](config.example.toml) to `~/.harbor/config.toml`. For a [portable ZIP](#windows-release-packages), use `config.toml` beside the marked executable instead. Harbor selects one configuration path at startup and uses it for automatic watching, manual reload and diagnostics throughout the session. **Ctrl+Shift+R** (`app.reload-configuration`) remains available independently of watching. Harbor does not create a missing file. The reload binding follows the same replacement/unbinding rules as other application commands.

Manual and automatic reload validate the complete document: unreadable files, malformed TOML, invalid recognized settings, or invalid/conflicting bindings reject the entire candidate and retain the last accepted configuration. Startup fallback below remains separate. Unknown ordinary settings produce ignored-setting warnings.

The application-owned watcher polls file contents every 200 ms and requests reload after an unchanged observation has lasted 300 ms. This coalesces save bursts without relying on file identity or modification timestamps; it is not a latency guarantee. Missing/malformed intermediate files never install startup defaults. Stable invalid edits retain accepted settings, repeated identical automatic failures are suppressed, and corrections can recover without restart. Observation I/O failures are visible in application feedback/logs and retry automatically; watcher setup failure leaves manual reload usable and can be retried by restarting Harbor. Byte-identical rewrites do not trigger a new automatic attempt; use manual reload to retry an unchanged candidate after a resource failure. Watching needs neither widget HMR nor a command palette.

Colors, application bindings, and terminal font family/size apply to existing and future terminals; fonts do not change application UI fonts or restart children. Font-driven grid changes use normal bounded primary reflow/alternate clipping, retaining newest available content rather than unlimited history. Shell program/arguments, environment overrides and initial directory change new tabs only. Clipboard policy remains effective until restart. Non-modal application feedback and logs distinguish these outcomes.

`[shell.environment]` is a string-to-string mapping of literal child-only additions/overrides over Harbor's inherited environment. Empty strings are explicit empty values; removing an override restores inheritance in future sessions. There is no inherited-variable deletion syntax, `$VAR`/`%VAR%` interpolation or shell-expression evaluation. Windows names are case-insensitive (including non-ASCII case pairs); conflicting configured spellings reject the whole reload. Names must be non-empty without `=` or NUL; values must not contain NUL. `TERM=xterm-256color` is protected: any different configured value is rejected regardless of casing. Diagnostics and Debug output omit environment values; malformed-TOML errors show only source location, not source snippets.

`[shell] directory = 'projects'` resolves against the directory containing `config.toml`, not Harbor's launch directory, the active shell's cwd or OSC 7 metadata. Absolute paths are supported. Omission/removal restores inherited-directory behavior. The directory must exist and be a directory when the settings are read. If it becomes unavailable later, new-session creation fails visibly without launching in another directory; existing sessions remain usable and creation can be retried after restoration. Neither setting changes Harbor's process-global environment/cwd. Both apply to the first startup child and to the actual shell selected by fallback. At startup an invalid environment mapping falls back as a unit to inheritance (with protected TERM), and an invalid directory falls back to inheritance; strict reload instead rejects the entire candidate.

OSC 10/11/12 and OSC 4 RGB overrides survive reload (even when equal to the old baseline), with alpha from the new baseline. OSC 110/111/112 and OSC 104 clear their respective overrides and restore the latest accepted configuration RGBA. New tabs do not inherit another session's overrides.

A later-session PTY resize failure triggers compensation before rejection. If compensation itself fails, reload still fails without accepting candidate settings; readers resume, geometry reconciliation is attempted, and affected sessions are explicitly reported as needing recovery. See [Current Status](docs/current-status.md) for implemented scope and [Validation Policy](docs/validation.md) for evidence requirements. [Package integration status](.grimoire/spec/0024-watched-reload-and-launch-settings.md#delivery-and-outstanding-acceptance) distinguishes executed automated/native Windows checks from pending interactive acceptance. Combined manual/watch/live/launch generations have real GPU/ConPTY coverage; full package acceptance, including fresh IME and physical-input checks, is not yet claimed. Evidence reports remain local and Git-ignored at the user's request, not distributed artifacts.

`[clipboard] osc52_write = "allow"` controls application-requested OSC 52 writes. Values are `allow` (default), `deny`, and `confirm`; invalid values fall back locally to `allow`. This setting applies at startup, including subsequently created tabs; restart Harbor to apply edits.

Only the active live tab in the foreground, non-minimized main window may request a write or clear. `confirm` opens an independent per-request window with the source tab, decoded byte size and a bounded preview. Its own focus may continue that admitted interaction; tab/source closure or switching applications cancels it. Further requests are rejected while pending, and approval is not a persistent grant. `allow` keeps only the latest unexecuted valid write per session.

Supported OSC 52 selections are `c` and empty (system clipboard), with standard padded/unpadded base64 encoding of UTF-8 text without NUL. Empty text clears under the same policy. Fixed limits: 4,194,304 decoded bytes and 5,592,408 encoded data bytes. Clipboard reads (`?`), other selections, binary text and malformed/over-limit requests are ignored. Other OSC/string limits are unchanged. This is not proof of application authenticity or WSL/SSH/tmux compatibility; runtime evidence is recorded separately.

For manual-assisted Windows checks inside an existing Harbor tab, run [`scripts/test_osc52_manual.ps1`](scripts/test_osc52_manual.ps1). The script requires explicit clipboard-replacement consent and does not edit startup configuration.

Supported settings include font family/size, shell program/arguments/environment/directory, default and ANSI terminal colors, OSC 52 write policy, and structured per-command keybindings.

The built-in default terminal palette is Catppuccin Mocha, with Harbor's existing background, cursor, and selection transparency. It affects only terminal colors, not application day/night appearance or the Windows Acrylic tint. Explicit `[colors]` settings still override these defaults; indexed slots 16–255 retain the standard color cube and grayscale.

- Invalid scalar font/shell fields fall back independently; an invalid supplied color resets the complete palette.
- Invalid keybinding overrides reset the complete binding table while retaining valid non-keybinding settings. Omitted commands keep defaults; `bindings = []` unbinds a command.
- At startup, missing files use complete defaults silently; unreadable files and invalid TOML use complete defaults with diagnostics. Portable mode never substitutes home-directory settings. Unknown keys are warned about and ignored.
- Missing font family uses DirectWrite system monospace selection. On Windows, a missing shell program tries `pwsh.exe`, then `powershell.exe`, then `COMSPEC`/`cmd.exe`, falling back when process creation fails. A configured executable takes precedence; if it cannot start, the same default sequence is retried without its configured arguments. Tab titles use the executable that actually started. This Windows policy does not apply to other platforms; Unix PTY support is not yet implemented.
- Terminal background alpha is preserved. It does not configure the Windows Acrylic backdrop tint.

Named profiles and user-selectable themes are planned, not additional current TOML options. Scoped Windows child-launch and transaction checks passed; interactive launch-setting UI acceptance is not implied by automated checks. The detailed local-only record is `docs/verification/session-launch-settings-evidence.md` (Git-ignored and not distributed with the repository).

### Debug Widget Hot Reload (Windows)

This is optional development-time UI-library reload, **not user-configuration hot reload**.

Build the reloadable library and keep it rebuilding in one terminal:

```bash
cargo install cargo-watch
cargo build -p harbor-app-ui
cargo watch -w crates/harbor-app/src/ui.rs -w crates/harbor-app-ui/src -x "build -p harbor-app-ui"
```

Run the persistent host in another:

```bash
cargo run --features widget-hot-reload
```

The adapter retains the native host and Host-owned tabs/terminals/PTYs while replacing the application root and resetting Widget/Fiber-local state. Changes to shared contracts, `harbor-widget`, dependencies, or exported signatures require a full rebuild/restart. Release and unsupported-target builds do not start the observer. See [runtime architecture](docs/architecture/widget-runtime.md) for ownership details.

## Checks

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
python scripts/check_docs.py
python scripts/checklist_summary.py
```

[Validation](docs/validation.md) distinguishes source tests, configured CI, runtime acceptance and release evidence. Heap profiling uses `cargo run --profile dhat --features dhat-heap`; follow the [profiling guide](docs/performance/profiling-guide.md) for comparable captures.

## Architecture

```text
Application host: sessions/tabs, commands, paste safety, platform policy
  -> harbor-widget native adapter: Window / Surface / Runtime / presentation
    -> terminal widget bridge: layout, input, scheduling, external draw
      -> harbor-terminal: screen, input, PTY I/O, rendering
        -> harbor-parser / harbor-text / harbor-pty
```

The generic native adapter does not own terminal business policy. Each OS window has its own widget runtime; session resources survive ordinary tab switching. See [architecture](docs/architecture/widget-runtime.md) and [ADR-0031](.grimoire/adr/0031-widget-winit-adapter-owns-native-host-infrastructure.md).

## Documentation

Start at [docs/README.md](docs/README.md). The shortest path is:

1. [Current Status](docs/current-status.md) — what exists and what remains unverified.
2. [Roadmap](docs/roadmap.md) — delivery order and release scope.
3. [Next-Stage Product Plan](docs/next-stage-plan.md) — concrete work packages and acceptance boundaries.
4. [Validation](docs/validation.md) — how to prove a change is ready.

Durable decisions and implementation records live under [`.grimoire/`](.grimoire/README.md).
