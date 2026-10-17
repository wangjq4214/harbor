# Configuration reference

[Application guide](README.md) · [Command IDs and binding grammar](commands.md) · [Troubleshooting](troubleshooting.md)

This is the current TOML interface, not a wishlist. The authorities are the [loader, defaults, validators, and tests](../../crates/harbor-config/src/lib.rs), [palette implementation](../../crates/harbor-config/src/color.rs), [application binding resolver](../../crates/harbor-app/src/command.rs), and [native acceptance path](../../src/shell.rs). The [example file](../../config.example.toml) is a customization example, not an exact serialization of built-in defaults.

## Configuration location

| Launch mode | Selected file |
| --- | --- |
| Installed or unmarked executable | `~/.harbor/config.toml`, where `~` is the resolved user home directory |
| Executable with adjacent **file** `harbor.portable` | `config.toml` beside that executable |

Selection occurs once at startup. The same retained path drives watching, manual reload, and diagnostics. Restart after adding/removing the marker to switch modes. Neither the working directory nor the active shell's directory selects the file. Portable mode does not fall back to the home configuration if its file is missing or invalid. Harbor's settings loader does not create directories or create/overwrite configuration files. Portable configuration isolation is not log/history isolation; see [releases](releases.md).

## Startup versus reload

| Condition | Startup | Manual or watched reload |
| --- | --- | --- |
| Missing file | Complete defaults, no error diagnostic | Silent non-acceptance; retain accepted settings, do not reset to defaults |
| Unreadable file or malformed TOML | Complete defaults with diagnostic | Reject whole candidate |
| Invalid recognized ordinary field | Recover that field (or its table/unit below); keep valid unrelated fields | Reject whole candidate, even alongside valid edits |
| Any invalid color/palette field | Restore **entire** built-in palette | Reject whole candidate |
| Invalid binding shape, ID, key, duplicate, or conflict | Restore **all** default application bindings | Reject whole candidate |
| Unknown ordinary table/key | Ignore with warning | Ignore with warning; otherwise valid candidate may be accepted |

Unknown fields inside binding records/command tables are errors, not ordinary ignored keys. A valid reload is a **complete replacement document**, not a merge with the previous file: removing an option restores its default/inheritance behavior after acceptance.

Watching is default-on and independent of widget HMR. The [observer](../../src/configuration_watch.rs) polls bytes every 200 ms with a 300 ms settling interval, coalescing saves/atomic replacements; these are implementation details, **not a latency guarantee**. The application re-reads the current file on its thread. Invalid intermediate saves never install defaults. Byte-identical rewrites do not trigger another attempt: use manual reload to retry the same candidate after a resource failure. See [watcher tests](../../src/configuration_watch_tests.rs) and [reload tests](../../src/configuration_reload_tests.rs).

### When changes take effect

| Options | Accepted effect |
| --- | --- |
| `font.*`, `colors.*`, `keybindings.*` | Live for applicable existing and future terminal/application resources |
| `shell.program`, `shell.args`, `shell.environment`, `shell.directory` | First startup child, then **new sessions only** after reload; existing children are unchanged |
| `clipboard.osc52_write` | **Restart required**, including for new tabs in the running process |
| Portable marker/path choice | **Restart required**; not a TOML setting |

Live terminal-font changes do not change chrome/dialog/UI fonts, recreate children, or promise lossless unlimited history. Grid changes use ordinary bounded primary reflow and alternate-screen clipping. Resource/native resize failure rejects the candidate; if compensation also fails, diagnostics identify unrecovered sessions rather than promising full geometry restoration. The [native transaction](../../src/tab_coordinator.rs) and [failure tests](../../src/configuration_reload_tests.rs) cover these seams. Source/tests do not establish complete interactive reload acceptance.

## Font options

| Option | Type / accepted values | Default | Startup invalid-value recovery |
| --- | --- | --- | --- |
| `font.family` | Nonempty string after trimming; requested system family, not a font-file path | Omitted: Windows primary-font selection | System selection |
| `font.size` | Finite integer/float, inclusive **6–144** | `24.0` | `24.0` |

A syntactically valid but unavailable family is distinct from invalid configuration: the [DirectWrite font backend](../../crates/harbor-text/src/font.rs) can select a system primary with a warning. Missing-character fallback is separate from primary grid metrics. No UI font, font-file, or profile option is parsed here.

## Shell launch options

| Option | Type / accepted values | Default | Startup invalid-value recovery |
| --- | --- | --- | --- |
| `shell.program` | Nonempty trimmed executable string | Omitted: platform shell fallback | Platform fallback |
| `shell.args` | Array of strings, passed as individual arguments | `[]` | No arguments |
| `shell.environment` | String-to-string table with rules below | Empty override map over inherited environment, with protected `TERM` | Discard the entire invalid map; inheritance/protected `TERM` remain |
| `shell.directory` | Nonempty literal path without NUL; must resolve to an accessible existing directory | Omitted: inherit Harbor's process directory | Inherit directory |

Executable launch attempts, argument quoting, and fallback are implemented in [Windows PTY creation](../../crates/harbor-pty/src/windows.rs). A configured program failing to spawn is not a TOML parse error: default shells are then attempted without that program's arguments. With no configured program, args apply to the selected default. See [getting started](getting-started.md#1-launch-on-windows) for the order.

Environment overrides affect the child only, never Harbor's environment or already-running shells. Windows names are case-insensitive; conflicting configured spellings reject the whole map. Names must be nonempty, without `=` or NUL; values must not contain NUL. An empty string means explicit empty, not deletion. Removing an override restores inheritance in later children; there is no inherited-variable deletion syntax. `$VAR`, `%VAR%`, and shell expressions remain literal. `TERM` must stay `xterm-256color`, regardless of name casing. Environment values are omitted from settings Debug/diagnostics; do not assume all shell arguments or unrelated logs are secret-safe.

Relative directories resolve against the selected **configuration file's parent**, not launch cwd, active-tab cwd, or OSC 7 metadata. Absolute paths are literal; no shell/environment expansion is provided. A directory disappearing after acceptance causes visible new-session creation failure, not a launch elsewhere; restore it and retry. Existing sessions remain intact. See [launch integration tests](../../src/session_launch_tests.rs).

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

Accepted reload updates the session palette reset baseline. Explicit OSC 10/11/12 default-color and OSC 4 indexed RGB overrides survive with alpha from the new baseline; OSC 110/111/112 and OSC 104 clear their respective overrides back to that baseline. Selection has no OSC override slot; truecolor cells are not rewritten. New sessions inherit the baseline, not another session's overrides. See [palette tests](../../crates/harbor-terminal/src/palette_tests.rs) and [recorded scoped palette evidence](../osc-palette-acceptance.md).

## Clipboard policy

| Option | Type / values | Default | Startup invalid-value recovery |
| --- | --- | --- | --- |
| `clipboard.osc52_write` | String: `"allow"`, `"deny"`, `"confirm"` | `"allow"` | `"allow"` |

This is startup host authorization for terminal-program-requested OSC 52 **system clipboard writes/clears**, not terminal paste or query-field editing. Requests require an eligible active, live, foreground session. `confirm` authorizes one request, not future writes. Reads are denied; decoded UTF-8 payload is bounded at 4 MiB. Changing the accepted file does not change effective policy until Harbor restarts. See [host authorization](../../src/clipboard.rs), [protocol checklist](../protocol/checklist.md), and [manual probe](../../scripts/terminal/test_osc52_manual.ps1); probe existence is not a fresh clipboard acceptance result.

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
