# Commands and shortcuts

[Application guide](README.md) · [Configuration](configuration.md) · [User workflows](getting-started.md)

## Built-in inventory

This table follows `command_registry()` in the [command module](../../crates/harbor-app/src/command.rs), not the example TOML. Shortcuts use exact simultaneous modifiers, not multi-stroke sequences. These are application command IDs, **not shell commands or CLI subcommands**.

| Command ID | Built-in shortcut | Meaning / context |
| --- | --- | --- |
| `app.reload-configuration` | Ctrl+Shift+R | Strict reload of the retained configuration path; independent of watcher |
| `app.new-tab` | Ctrl+T | Create/activate a new session |
| `app.close-active-tab` | Ctrl+W | Close active session; final tab requests window closure |
| `app.next-tab` | Ctrl+Tab | Next tab, wrapping |
| `app.previous-tab` | Ctrl+Shift+Tab | Previous tab, wrapping |
| `app.select-tab-1` | Ctrl+1 | Select position 1 if present |
| `app.select-tab-2` | Ctrl+2 | Select position 2 if present |
| `app.select-tab-3` | Ctrl+3 | Select position 3 if present |
| `app.select-tab-4` | Ctrl+4 | Select position 4 if present |
| `app.select-tab-5` | Ctrl+5 | Select position 5 if present |
| `app.select-tab-6` | Ctrl+6 | Select position 6 if present |
| `app.select-tab-7` | Ctrl+7 | Select position 7 if present |
| `app.select-tab-8` | Ctrl+8 | Select position 8 if present |
| `app.select-tab-9` | Ctrl+9 | Select position 9, not last |
| `terminal.copy` | Ctrl+Shift+C | Copy selection, or empty text without one; clear selection |
| `terminal.copy-or-interrupt` | Ctrl+C | Copy nonempty selection; otherwise pass through to child |
| `terminal.paste` | Ctrl+V | Host clipboard paste and applicable multiline confirmation |
| `terminal.page-up` | PageUp | Primary-buffer page review; alternate screen passes through |
| `terminal.page-down` | PageDown | Primary-buffer page review; alternate screen passes through |
| `terminal.scroll-to-top` | Home | Primary-buffer top; alternate screen passes through |
| `terminal.scroll-to-bottom` | End | Primary-buffer bottom; alternate screen passes through |
| `terminal.search` | Ctrl+Shift+F | Open/refocus current-session search |
| `terminal.search-next` | F3 | Next match, initially oldest, wrapping |
| `terminal.search-previous` | Shift+F3 | Previous match, initially newest, wrapping |
| `terminal.search-close` | None globally | Close/cancel search, restore terminal focus |

Search commands consume their chords even if no search/match is available. Unbind to allow those keys through. Escape in the query field closes search (or first cancels active IME composition); Enter/Shift+Enter navigate. There is **no global Escape binding** in this registry. Internal query-change and notification-dismissal intents are not configurable commands. `palette_visible` metadata does not implement a command palette.

## Input ownership matters

The [workspace action handler](../../crates/harbor-app/src/workspace.rs) returns pass-through for Ctrl+C without selection and for primary-review commands in the alternate screen. Consumed commands enqueue typed host requests rather than sending their keystrokes to the PTY. [Routing tests](../../crates/harbor-app/src/workspace_tests.rs) cover these contexts and consumed press/repeat/release behavior across rebinding.

The focused reusable search editor owns its editing shortcuts, including Ctrl+A/C/X/V/Z/Y, ahead of terminal copy/paste actions. Those modify the query, not terminal selection or PTY input. Composition has its own key ownership. See [editor implementation](../../crates/harbor-widget/src/widgets/editable_text.rs) and `reusable_search_editor_replaces_at_limit_and_keeps_clipboard_out_of_terminal_commands` in the routing tests. Confirmation windows gate main-window input separately; they are not extra registry commands.

## Override grammar

Use the selected [configuration file](configuration.md#configuration-location):

```toml
[keybindings.app.new-tab]
bindings = [{ modifiers = ["ctrl"], key = "n" }]

[keybindings.terminal.paste]
bindings = [
  { modifiers = ["ctrl"], key = "v" },
  { modifiers = ["shift"], key = "insert" },
]

[keybindings.terminal.page-up]
bindings = []
```

- Omission keeps a command's built-in chords. A present `bindings` array **replaces** them; an empty array unbinds. Multiple records provide alternative chords, not a sequence.
- Each record requires `modifiers` and `key`; extra fields are invalid.
- Modifier tokens (trimmed, ASCII-case-insensitive): `ctrl`/`control`, `shift`, `alt`, `meta`/`super`/`win`. Repeating aliases for the same modifier is invalid. Use `[]` for no modifiers.
- Key tokens (trimmed, ASCII-case-insensitive): one non-control character, or `tab`, `enter`, `space`, `escape`/`esc`, `backspace`, `insert`, `delete`, `home`, `end`, `pageup`/`page-up`, `pagedown`/`page-down`, `up`, `down`, `left`, `right`.
- **Current limitation:** built-in F3/Shift+F3 exist, but `parse_key` does not accept function-key strings such as `"f3"`. Omit those command tables to retain defaults, unbind them, or replace them with supported keys. Do not copy the built-in F3 label into a TOML record.
- Unknown IDs, unsupported tokens, repeated chords within a command, or conflicts across commands—including defaults of omitted commands—are errors. Startup restores the complete default binding table; live reload rejects the whole candidate. Reassigning an occupied chord requires removing/replacing its previous owner's binding too.

The example above adds Shift+Insert; it is not a default without configuration. See [structural parsing](../../crates/harbor-config/src/lib.rs) and [semantic resolution/tests](../../crates/harbor-app/src/command.rs).

## Keeping the inventory testable

When registry changes land, compare every ID/default in the table with `command_registry()`; retain all nine generated numeric commands and the globally unbound close command. Existing tests cover uniqueness/completeness, replacement/multiple bindings/unbinding, conflicts, and startup-versus-live rejection:

```powershell
cargo test --locked -p harbor-app --lib command::tests
```

This is a repeatable check, not a claim of native keyboard acceptance or an automatic Markdown-to-registry assertion. No generator or validation script is added by this guide.
