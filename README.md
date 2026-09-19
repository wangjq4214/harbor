# ⚓ Harbor

Harbor is a Windows-first, GPU-accelerated terminal emulator written in Rust with winit, wgpu, a custom VT parser, DirectWrite text support, and a declarative widget runtime.

The priority is a correct, stable daily-use Windows terminal. Native Unix PTY support remains deferred; WSL/SSH compatibility inside the Windows application is a separate concern.

## Current Capabilities

- 🧠 Incremental, bounded VT parsing; screen editing, colors, margins, alternate screen and scrollback.
- 🪟 Windows ConPTY sessions, multiple terminal tabs, selection/copy and scrollback controls.
- ⌨️ Traditional keyboard modes, bracketed paste/confirmation, SGR mouse reporting, focus reporting and IME commit/preedit integration.
- 🔗 Terminal replies and capability queries; OSC titles, working-directory metadata, hyperlinks, default colors and shell markers.
- 🎨 Damage-aware wgpu rendering, DirectWrite font fallback, synchronized-output scheduling and cursor/decorations.
- ⚙️ Startup TOML settings and configurable application keybindings.
- ✨ Acrylic/backdrop fallback, rounded widget decorations and a retained desktop widget runtime.

These are implemented scopes, **not a claim that every protocol or Windows release gate is complete**. See [Current Status](docs/current-status.md) for source links and limitations, and the [Protocol Checklist](docs/protocol/checklist.md) for exact coverage.

The main gaps are resize reflow, complete combining/grapheme text handling, settings hot reload, split panes, search, profiles, a command palette, Kitty protocols, selected terminal extensions, and release/performance evidence. [The roadmap](docs/roadmap.md) orders the work; [the next-stage plan](docs/next-stage-plan.md) defines its scope, including UI polish and a liquid-glass investigation.

## Build and Run

An operational PTY session currently requires Windows.

```bash
cargo run
```

## Library Crates

The reusable subsystems have crate-level usage guides:

- [`harbor-widget`](crates/harbor-widget/README.md) — declarative components, state, layout, input, rendering, and optional `winit` hosting.
- [`harbor-terminal`](crates/harbor-terminal/README.md) — ANSI/VT parsing, screen state, PTY I/O, input encoding, scheduling, and wgpu rendering.

Both crates are currently documented as pre-1.0 workspace libraries. Their READMEs show path-based setup, minimal examples, host integration, feature flags, and ownership boundaries.

### Startup Configuration

Copy [`config.example.toml`](config.example.toml) to `~/.harbor/config.toml`. Harbor reads it once at startup; it does not create a missing file or hot-reload changes.

Supported settings include font family/size, shell program/arguments, default and ANSI terminal colors, and structured per-command keybindings.

- Invalid scalar font/shell fields fall back independently; an invalid supplied color resets the complete palette.
- Invalid keybinding overrides reset the complete binding table while retaining valid non-keybinding settings. Omitted commands keep defaults; `bindings = []` unbinds a command.
- Missing/unreadable files and invalid TOML use complete defaults. Unknown keys are warned about and ignored.
- Missing font family uses DirectWrite system monospace selection. Missing shell program uses `COMSPEC`, then `cmd.exe`; a configured executable that cannot start falls back without its configured arguments.
- Terminal background alpha is preserved. It does not configure the Windows Acrylic backdrop tint.

Named profiles, environment/working-directory settings, user-selectable themes and configuration reload are planned, not additional current TOML options.

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
