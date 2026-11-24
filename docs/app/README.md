# Harbor application guide

Harbor is a Windows-first terminal application with tabbed pane workspaces, GPU rendering, configurable shortcuts, TOML settings, and current-session search.

| I want to… | Read |
| --- | --- |
| Launch a shell, use tabs, copy/paste, or find output | [Getting started](getting-started.md) |
| Change fonts, colors, shell launch settings, or reload behavior | [Configuration reference](configuration.md) |
| Look up a command ID or default shortcut | [Commands and shortcuts](commands.md) |
| Split, focus, maximize or close independent panes | [Pane workspaces](panes.md) |
| Diagnose startup, input, rendering, or compatibility problems | [Troubleshooting](troubleshooting.md) |
| Choose MSI versus ZIP, upgrade, or build a distribution | [Windows releases](releases.md) |
| Review application visual foundations and the Quick Save design | [Design system (draft)](design-system.md) |

## Current limitations

Pane splitting and named-profile/quick-save flows are implemented, not fully runtime-accepted. [Pane verification limits](panes.md#verification-boundary) include pending native interaction and controlled WSL/direct-SSH observations. Native Unix shell sessions remain unavailable. The command palette and named-profile chooser retain their [scoped verification limits](commands.md#named-profile-chooser).

For embedding, see the [widget guide](../widget/README.md) and [terminal guide](../terminal/README.md).
