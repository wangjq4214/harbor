# Configuration reference

[Application guide](README.md) · [Command IDs and binding grammar](commands.md) · [Troubleshooting](troubleshooting.md)

Settings use TOML. Start with the [example file](../../config.example.toml) or the small example below; the example file includes customizations, not just built-in defaults.

## Configuration location

| Launch mode | Selected file |
| --- | --- |
| Installed or unmarked executable | `~/.harbor/config.toml`, where `~` is the resolved user home directory |
| Executable with adjacent **file** `harbor.portable` | `config.toml` beside that executable |

Selection occurs once at startup. The same retained path drives watching, manual reload, and diagnostics. Restart after adding/removing the marker to switch modes. Neither the working directory nor the active shell's directory selects the file. Portable mode does not fall back to the home configuration if its file is missing or invalid. Harbor's settings loader does not create directories or create/overwrite configuration files. Portable configuration isolation is not log/history isolation; see [releases](releases.md).

## Startup versus reload

| Condition | Startup | Manual or watched reload |
| --- | --- | --- |
| Missing file | Complete defaults, no error diagnostic | Keep current settings silently; do not reset to defaults |
| Unreadable file or malformed TOML | Complete defaults with diagnostic | Reject whole candidate |
| Invalid recognized ordinary field | Recover that field (or its table/unit below); keep valid unrelated fields | Reject whole candidate, even alongside valid edits |
| Any invalid color/palette field | Restore **entire** built-in palette | Reject whole candidate |
| Invalid binding shape, ID, key, duplicate, or conflict | Restore **all** default application bindings | Reject whole candidate |
| Unknown ordinary table/key | Ignore with warning | Ignore with warning; otherwise valid candidate may be accepted |

Unknown fields inside binding records/command tables are errors, not ignored keys. A valid reload is a **complete replacement document**, not a merge with the previous file: removing an option restores its default or inherited value.

Harbor watches the selected file by default and coalesces saves before reloading. Invalid intermediate saves do not replace accepted settings. Rewriting identical bytes does not trigger another attempt; use Ctrl+Shift+R to retry after a reload failure.

### When changes take effect

| Options | Accepted effect |
| --- | --- |
| `font.*`, `colors.*`, `keybindings.*` | Live for applicable existing and future terminal/application resources |
| `shell.program`, `shell.args`, `shell.environment`, `shell.directory` | First startup child, then **new sessions only** after reload; existing children are unchanged |
| `clipboard.osc52_write` | **Restart required**, including for new tabs in the running process |
| Portable marker/path choice | **Restart required**; not a TOML setting |

Live terminal-font changes do not change UI fonts or restart shells. Resizing can reflow primary-buffer text or clip alternate-screen content; retained history is bounded. If a font or resize change fails, inspect the diagnostic before continuing.

## Font options

| Option | Type / accepted values | Default | Startup invalid-value recovery |
| --- | --- | --- | --- |
| `font.family` | Nonempty string after trimming; requested system family, not a font-file path | Omitted: Windows primary-font selection | System selection |
| `font.size` | Finite integer/float, inclusive **6–144** | `24.0` | `24.0` |

An unavailable font family can fall back to a system font with a warning. Missing-character fallback is separate from primary grid metrics. There is no UI-font, font-file, or profile option.

## Shell launch options

| Option | Type / accepted values | Default | Startup invalid-value recovery |
| --- | --- | --- | --- |
| `shell.program` | Nonempty trimmed executable string | Omitted: platform shell fallback | Platform fallback |
| `shell.args` | Array of strings, passed as individual arguments | `[]` | No arguments |
| `shell.environment` | String-to-string table with rules below | Empty override map over inherited environment, with protected `TERM` | Discard the entire invalid map; inheritance/protected `TERM` remain |
| `shell.directory` | Nonempty literal path without NUL; must resolve to an accessible existing directory | Omitted: inherit Harbor's process directory | Inherit directory |

If a configured program fails to start, Harbor tries default shells without that program's arguments. With no configured program, arguments apply to the selected default. See [getting started](getting-started.md#1-launch-on-windows) for the order.

Environment overrides affect new shells only, never Harbor or already-running shells. Windows names are case-insensitive; conflicting spellings reject the whole map. Names must be nonempty, without `=` or NUL; values must not contain NUL. An empty string sets an empty value rather than deleting the variable. Removing an override restores inheritance in later children; deleting inherited variables is not supported. `$VAR`, `%VAR%`, and shell expressions remain literal. `TERM` must stay `xterm-256color`, regardless of name casing. Review logs before sharing: shell arguments and unrelated events may contain sensitive data.

Relative directories resolve against the selected **configuration file's parent**, not Harbor's launch directory, active-tab directory, or OSC 7 metadata. Paths are literal; no shell/environment expansion is provided. If a directory disappears, creating a new session fails rather than launching elsewhere. Restore it and retry; existing sessions remain intact.

## Terminal colors

All color strings accept exactly `#RRGGBB` or `#RRGGBBAA`; six digits mean opaque. Omitted fields retain their built-in defaults. Colors configure terminal content, **not** application light/dark mode, tab chrome, search glass, or whole-window Acrylic tint.

| Option | Type / range | Built-in default |
| --- | --- | --- |
| `colors.foreground` | Color string | `#CDD6F4` |
| `colors.background` | Color string | RGB `#1E1E2E`, alpha **0.85** |
| `colors.background_opacity` | Finite integer/float, **0–1** inclusive | Omitted: retain effective background alpha |
| `colors.cursor` | Color string | `#F5E0DCCC` (alpha 0.8) |
| `colors.selection` | Color string | `#F5E0DC66` (alpha 0.4) |
| `colors.normal.<name>` | Color string; names below | ANSI slots 0–7 |
| `colors.bright.<name>` | Color string; names below | ANSI slots 8–15 |

| `<name>` | Normal | Bright |
| --- | --- | --- |
| `black` | `#45475A` | `#585B70` |
| `red` | `#F38BA8` | `#F38BA8` |
| `green` | `#A6E3A1` | `#A6E3A1` |
| `yellow` | `#F9E2AF` | `#F9E2AF` |
| `blue` | `#89B4FA` | `#89B4FA` |
| `magenta` | `#F5C2E7` | `#F5C2E7` |
| `cyan` | `#94E2D5` | `#94E2D5` |
| `white` | `#BAC2DE` | `#A6ADC8` |

`background_opacity`, when present, overrides the hex background's alpha. With neither background setting, alpha is 0.85; explicitly setting `background = "#1E1E2E"` without opacity makes it **opaque**. Indexed slots 16–255 use the built-in color cube/grayscale; there is no TOML extended-slot table. Without a working compositor backdrop, the host's opaque fallback still applies.

Reload updates the session palette's reset colors. Colors changed by a terminal program through OSC 4/10/11/12 remain overridden until the program resets them with OSC 104/110/111/112. New sessions use the configured colors, not another session's overrides.

## Clipboard policy

| Option | Type / values | Default | Startup invalid-value recovery |
| --- | --- | --- | --- |
| `clipboard.osc52_write` | String: `"allow"`, `"deny"`, `"confirm"` | `"allow"` | `"allow"` |

This controls terminal-program-requested OSC 52 **system clipboard writes/clears**, not terminal paste or search-field editing. Requests require an active, live, foreground session. `confirm` authorizes one request, not future writes. Clipboard reads are denied; decoded UTF-8 payload is limited to 4 MiB. Restart Harbor after changing this policy.

## Keybinding options

`[keybindings.<namespace>.<command>]` takes `bindings`, an array of records containing `modifiers` (array of strings) and `key` (string). Omitted command tables keep built-in defaults; `bindings = []` unbinds; records replace rather than append to a command's defaults. Accepted changes are live. See the complete [command inventory, grammar, and context rules](commands.md).

## Small complete example

This changes terminal size/opacity, adds a literal child variable, and moves New tab from Ctrl+T to Ctrl+N. Other omitted settings use defaults. It neither configures a profile nor changes UI fonts.

```toml
[font]
size = 22.0

[colors]
background_opacity = 0.9

[shell.environment]
HARBOR_EXAMPLE = "literal value"

[keybindings.app.new-tab]
bindings = [{ modifiers = ["ctrl"], key = "n" }]
```

Merge examples into one document without duplicate TOML table declarations. Save to the selected file and use [reload diagnostics](troubleshooting.md#configuration-did-not-change); an unnoticed syntax error at startup has different recovery semantics from a live rejection.
