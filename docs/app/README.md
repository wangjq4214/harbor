# Harbor application guide

Harbor is a Windows-first terminal application with terminal tabs, GPU rendering, configurable shortcuts, TOML settings, and current-session search.

| I want to… | Read |
| --- | --- |
| Launch a shell, use tabs, copy/paste, or find output | [Getting started](getting-started.md) |
| Change fonts, colors, shell launch settings, or reload behavior | [Configuration reference](configuration.md) |
| Look up a command ID or default shortcut | [Commands and shortcuts](commands.md) |
| Diagnose startup, input, rendering, or compatibility problems | [Troubleshooting](troubleshooting.md) |
| Choose MSI versus ZIP, upgrade, or build a distribution | [Windows releases](releases.md) |
| Review application visual foundations and the Quick Save design | [Design system (draft)](design-system.md) |

## Current limitations

Pane splitting, quick saving of presets, and native Unix shell sessions are not available. The command palette and named-profile chooser are implemented with [scoped verification limits](commands.md#named-profile-chooser).

For embedding, see the [widget guide](../widget/README.md) and [terminal guide](../terminal/README.md).
