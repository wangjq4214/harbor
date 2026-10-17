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

Protocol support is scoped, not complete. See the [VT reference](docs/terminal/vt.md) and [compatibility guide](docs/terminal/compatibility.md) for supported behavior and limitations.

Split panes, named profiles, a command palette and Kitty protocols are not implemented. Unicode width, configuration reload and application compatibility have limitations; consult the relevant guides before depending on them.

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

Build unsigned x64 MSI/portable assets with `./scripts/packaging/package_windows.ps1`. See the [release guide](docs/app/releases.md) for prerequisites, payload, install/portable paths, upgrades and publication policy.

## Library Crates

The reusable subsystems have crate-level usage guides:

- [`harbor-widget`](crates/harbor-widget/README.md) — declarative components, state, layout, input, rendering, and optional `winit` hosting.
- [`harbor-terminal`](crates/harbor-terminal/README.md) — ANSI/VT parsing, screen state, PTY I/O, input encoding, scheduling, and wgpu rendering.

Both crates are currently documented as pre-1.0 workspace libraries. Their READMEs show path-based setup, minimal examples, host integration, feature flags, and ownership boundaries.

### Startup Configuration

Installed/unmarked builds use `~/.harbor/config.toml`; portable builds use the file beside the marked executable. Copy [`config.example.toml`](config.example.toml) if needed. Settings are watched by default; **Ctrl+Shift+R** reloads manually. Read the [configuration reference](docs/app/configuration.md) for defaults, rejection/recovery and live/new-session/restart timing, and [commands](docs/app/commands.md) for rebinding.

## Documentation

Use the [documentation index](docs/README.md) for application usage, widget development and terminal embedding guides. Contributors can use the [developer guide](docs/developer/README.md) for architecture, progress and the roadmap.
