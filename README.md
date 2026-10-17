# ⚓ Harbor

Harbor is a Windows-first, GPU-accelerated terminal emulator written in Rust with winit, wgpu, a custom VT parser, DirectWrite text support, and a declarative widget runtime.

The priority is a correct, stable daily-use Windows terminal. Native Unix PTY support remains deferred; WSL/SSH compatibility inside the Windows application is a separate concern.

[Use Harbor](docs/app/README.md) · [Build widgets](docs/widget/README.md) · [Embed the terminal](docs/terminal/README.md) · [Documentation index](docs/README.md)

## Current Capabilities

- 🧠 Incremental, bounded VT parsing; screen editing, colors, margins, alternate screen and scrollback.
- 🪟 Windows ConPTY sessions, multiple terminal tabs, selection/copy and scrollback controls.
- ⌨️ Traditional keyboard modes, bracketed paste/confirmation, multi-encoding mouse reporting, focus reporting and IME integration.
- 🔗 Terminal replies and capability queries; OSC titles, working-directory metadata, hyperlinks, default colors and shell markers.
- 🎨 Damage-aware wgpu rendering, DirectWrite font fallback, synchronized-output scheduling and cursor/decorations.
- ⚙️ TOML settings with manual/default-on watched reload and configurable application keybindings.
- ✨ Acrylic/backdrop fallback, reusable text fields, Portal overlays, gradients and local blur/edge refraction.
- 🔎 Bounded current-session literal search with highlights and previous/next navigation.

These are implemented scopes, **not a claim that every protocol or Windows release gate is complete**. See [Current Status](docs/current-status.md) for source links and limitations, and the [Protocol Checklist](docs/protocol/checklist.md) for exact coverage.

The main gaps are remaining Windows/performance acceptance, the documented Unicode no-wrap width discrepancy, combined configuration-reload acceptance, split panes, profiles, command palette and Kitty protocols. Delivered search, text editing and local glass mechanisms have their own remaining acceptance boundaries; they are not all-feature or release-completion claims. [The roadmap](docs/roadmap.md) orders the work; [targets and boundaries](docs/targets.md) explains the WezTerm, React/Flutter and Ghostty comparisons.

## Build and Run

An operational PTY session currently requires Windows.

```powershell
./scripts/setup/fetch_conpty.ps1
cargo run
```

Run the fetch script once after cloning, and again when the pinned runtime changes.
Windows builds stage the pinned [Microsoft ConPTY runtime](third_party/conpty/README.md)
in `target/<profile>/conpty/`. When distributing or moving `harbor.exe`, include
the complete adjacent `conpty/` directory (DLL, hosts, version and license).
An incomplete bundle is reported as a PTY startup error rather than falling back
to an older Windows console host with incompatible resize behavior.

To regenerate the checked-in Windows App SDK bindings, restore the pinned build-time
[WASDK metadata](third_party/wasdk/README.md) with `./scripts/setup/fetch_wasdk.ps1`.
Normal builds do not require this download.

### Windows Release Packages

Build unsigned x64 MSI/portable assets with `./scripts/packaging/package_windows.ps1`. See the [release guide](docs/app/releases.md) for prerequisites, payload, install/portable paths, upgrades and publication policy; [release validation](docs/validation.md#windows-release-packages) distinguishes package construction from installer/download acceptance.

## Library Crates

The reusable subsystems have crate-level usage guides:

- [`harbor-widget`](crates/harbor-widget/README.md) — declarative components, state, layout, input, rendering, and optional `winit` hosting.
- [`harbor-terminal`](crates/harbor-terminal/README.md) — ANSI/VT parsing, screen state, PTY I/O, input encoding, scheduling, and wgpu rendering.

Both crates are currently documented as pre-1.0 workspace libraries. Their READMEs show path-based setup, minimal examples, host integration, feature flags, and ownership boundaries.

### Startup Configuration

Installed/unmarked builds use `~/.harbor/config.toml`; portable builds use the file beside the marked executable. Copy [`config.example.toml`](config.example.toml) if needed. Settings are watched by default; **Ctrl+Shift+R** reloads manually. Read the [configuration reference](docs/app/configuration.md) for defaults, rejection/recovery and live/new-session/restart timing, and [commands](docs/app/commands.md) for rebinding.

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

Use the [standard quality gates](docs/validation.md#standard-quality-gates). [Scripts](scripts/README.md) lists helper commands and probe safety; [profiling](docs/performance/profiling-guide.md) explains comparable captures.

## Architecture

See [System Architecture](docs/architecture/system.md) for ownership, dependency direction and input/output/frame flows, and [Widget Runtime Architecture](docs/architecture/widget-runtime.md) for runtime-specific contracts.

## Documentation

Use the [documentation index](docs/README.md) to choose a reader path. [Targets](docs/targets.md) defines comparison boundaries; [Current Status](docs/current-status.md) records delivered scope and acceptance gaps; [Roadmap](docs/roadmap.md) orders future work. Durable contracts remain in the read-only, skill-maintained [Grimoire](.grimoire/README.md).
