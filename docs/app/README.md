# Harbor application guide

Harbor is a Windows-first terminal application: one desktop workspace with terminal tabs, GPU rendering, configurable shortcuts, TOML settings, and current-session search. This guide is for people running Harbor, not embedding its libraries.

## Choose a starting point

| I want to… | Read |
| --- | --- |
| Launch a shell, use tabs, copy/paste, or find output | [Getting started](getting-started.md) |
| Change fonts, colors, shell launch settings, or reload behavior | [Configuration reference](configuration.md) |
| Look up a command ID, default shortcut, or input context | [Commands and shortcuts](commands.md) |
| Diagnose startup, input, rendering, or compatibility problems | [Troubleshooting](troubleshooting.md) |
| Choose MSI versus ZIP, upgrade, or build a distribution | [Windows releases](releases.md) |

For reusable UI components, see the [widget product line](../widget/README.md); for terminal embedding and protocol behavior, see the [terminal product line](../terminal/README.md). The [target guide](../targets.md) owns platform scope, and the [system architecture](../architecture/system.md) explains subsystem boundaries.

## What this guide promises

The application currently composes a tab rail, one active terminal, configuration notifications, confirmation windows, and a search overlay. Search uses the reusable `TextField` inside a **single-row Portal glass panel**: input, fixed result slot, previous, next, close. It is not a pane or a command palette. See the [workspace source](../../crates/harbor-app/src/workspace.rs), [search lifecycle](../../crates/harbor-app/src/search.rs), and [composition/routing tests](../../crates/harbor-app/src/workspace_tests.rs).

Pane splitting, a command palette, and named profiles are **not implemented application workflows** in the current [product inventory](../current-status.md). Registry metadata such as `palette_visible` is not a palette UI. WezTerm is a usability/documentation benchmark, not a compatibility promise: Harbor does not thereby commit to Lua configuration, an SSH multiplexer, or WezTerm-style CLI commands. Use only the [implemented TOML schema](configuration.md) and [registry commands](commands.md).

## Implementation is not acceptance

These pages describe inspected source and link to tests and existing evidence. A test file, configured CI job, historical run, or specification does not establish a fresh native Windows pass. In particular, integrated reload/input/IME acceptance, search under ongoing output and resize, visual glass quality, distribution lifecycle, and downloaded-release smoke have separate evidence requirements.

Consult [Current Status](../current-status.md) for repository-wide evidence limits and [Validation](../validation.md) for acceptance procedures. Unrecorded WSL/SSH/application combinations are **unverified**, not automatically unsupported or proven compatible. This documentation change performs no GUI or clipboard operations.
