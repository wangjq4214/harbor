# Commands and shortcuts

[Application guide](README.md) · [Configuration](configuration.md) · [User workflows](getting-started.md)

## Built-in shortcuts

Shortcuts use exact simultaneous modifiers, not multi-stroke sequences. These are application command IDs, **not shell commands or CLI subcommands**.

| Command ID | Built-in shortcut | Meaning / context |
| --- | --- | --- |
| `app.command-palette` | Ctrl+Shift+P | Open/focus the command palette for existing application commands |
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

Search commands consume their shortcuts even if no search or match is available; unbind them to allow those keys through. Escape in the query field closes search (or first cancels IME composition); Enter/Shift+Enter navigate. There is **no global Escape binding**.

## Command palette

Press **Ctrl+Shift+P** to open/focus the query. The integrated catalog includes configuration reload; tab creation, closing, navigation and numeric selection (1–9); terminal copy/paste and primary-buffer scrolling; and current-session search open/close/next/previous. `terminal.copy-or-interrupt` is deliberately hidden: palette Copy always copies, including empty text without a selection. Profiles, panes and application appearance switching are not delivered.

Type a **case-insensitive substring** of a display label or stable command ID (not fuzzy matching). An empty query exposes the catalog; no matches shows an empty state and cannot execute. Up/Down selects a row; Enter invokes it once. Rows show accepted **effective** shortcuts, including multiple alternatives and `Unbound`, rather than superseded defaults. Accepted configuration reload updates them; a rejected reload retains the prior bindings. The opener ID is `app.command-palette`, configurable with the grammar below.

Unavailable rows explain why—for example, scrolling in the alternate screen, a missing tab position, or closed search. Invocation rechecks context; rejecting a palette action does not send Enter or a scroll key to the terminal. Successful and no-change invocations show no notice; the palette closing and the action's own effect (including the separate paste-confirmation window) are the feedback. Rejected and failed invocations show a notice naming the command and reason, and a later success retires it. Reload errors are independent notices and also auto-close after three seconds; a palette result is not a promise of new search matches or successful execution of pasted shell text.

The query uses the reusable single-line editor: caret/selection, Unicode-safe edits, Ctrl+A/C/X/V, Ctrl+Z undo, and Ctrl+Y/Ctrl+Shift+Z redo. Clipboard shortcuts edit the field, not terminal input or selection. IME preedit does not filter until committed; confirming a candidate does not invoke a command. Escape first cancels composition, then closes the palette on a separate press.

Cancel returns to terminal interaction. Tab actions target the resulting active terminal; closing the final tab requests window closure. Search open and match navigation target the search editor; search close restores the terminal. Copy, scroll and reload return to terminal interaction. Paste preserves the separate confirmation window and its input gate rather than restoring palette focus. Losing window focus closes the palette; reopen it after returning.

**Verification limits:** focused tests cover the full visible catalog, but integrated Windows acceptance is still incomplete. Native checks observed Chinese candidate commit/cancel, search destinations, alternate-screen rejection, binding reload/display and field clipboard edits. Confirmation completion, late-result/root-replacement races and parts of the tab/scroll matrix remain unresolved. On the tested Windows setup, terminal text shows through the palette and reduces readability; notifications can overlap it. See the [revision-bound evidence and remaining scenarios](../verification/T0003-command-palette/README.md). This is not completion of issue #156 or broad terminal/application certification.

## Where shortcuts apply

Ctrl+C without selection reaches the shell. PageUp/PageDown/Home/End review the primary buffer but pass through in alternate-screen applications.

The focused search or palette field handles its own editing shortcuts, including Ctrl+A/C/X/V/Z/Y. These modify the query, not terminal selection or input. While a confirmation window is open, terminal input is blocked.

## Override grammar

Use the selected [configuration file](configuration.md#configuration-location):

```toml
[keybindings.app.command-palette]
bindings = [{ modifiers = ["ctrl", "shift"], key = "p" }]

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
- **Current limitation:** built-in F3/Shift+F3 exist, but custom function-key strings such as `"f3"` are not accepted. Omit those command tables to retain defaults, unbind them, or replace them with supported keys.
- Unknown IDs, unsupported tokens, repeated chords within a command, or conflicts across commands—including defaults of omitted commands—are errors. Startup restores the complete default binding table; live reload rejects the whole candidate. Reassigning an occupied chord requires removing/replacing its previous owner's binding too.

The example above adds Shift+Insert; it is not a built-in default.
