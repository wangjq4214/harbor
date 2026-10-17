# Configurable Application Keybindings

**Spec ID:** 0013
**Status:** Implemented
**Date:** 2026-09-18

## Requirement

Harbor must expose currently implemented user-invokable application operations through typed application commands, bind keyboard chords to stable command IDs with startup TOML overrides, and give matched application commands explicit first refusal over terminal input without leaking consumed UI chords to the PTY.

The same command identity and dispatcher must support buttons, menus, shortcuts, and a future command palette. Internal PTY output, redraw, window lifecycle, and other non-user-invokable notifications must remain outside the command system.

## Solution

### Command boundary

`harbor-app` owns typed application commands, a registry, and dispatch metadata. Stable string IDs are used only at persisted configuration and UI-discovery boundaries; execution remains typed and exhaustive inside the application.

The registry records each implemented command's stable ID, display metadata, default chords, availability, and whether it should appear in a future command palette. Parameterized typed commands, such as closing a specific tab from its rail button, use the same dispatcher without requiring a configurable registry ID.

The initial configurable command set is:

| Command ID | Default chord | Required behavior |
| --- | --- | --- |
| `app.new-tab` | `ctrl+t` | Create and activate a terminal tab using existing Host behavior. |
| `app.close-active-tab` | `ctrl+w` | Close the active tab using existing close and focus behavior. |
| `app.next-tab` | `ctrl+tab` | Activate the next tab. |
| `app.previous-tab` | `ctrl+shift+tab` | Activate the previous tab. |
| `app.select-tab-1` … `app.select-tab-9` | `ctrl+1` … `ctrl+9` | Activate the corresponding tab index using existing bounds behavior. |
| `terminal.copy` | `ctrl+shift+c` | Always consume; copy the active selection or empty text when no selection exists. Future palettes expose this copy command. |
| `terminal.copy-or-interrupt` | `ctrl+c` | Copy and consume when a non-empty selection exists; otherwise pass the same input through to the terminal. This command remains configurable but palette-hidden. |
| `terminal.paste` | `ctrl+v` | Invoke the existing clipboard, bracketed-paste, multiline-confirmation, and cross-window input-gate flow. |
| `terminal.page-up` | `pageup` | Scroll one primary-screen page; pass through in the alternate screen. |
| `terminal.page-down` | `pagedown` | Scroll one primary-screen page; pass through in the alternate screen. |
| `terminal.scroll-to-top` | `home` | Move to the top of primary-screen scrollback; pass through in the alternate screen. |
| `terminal.scroll-to-bottom` | `end` | Return to the live bottom on the primary screen; pass through in the alternate screen. |

Search, zoom, and other unavailable operations are not registered as no-op commands.

### Dispatch and input precedence

The existing widget action path gains a synchronous action result with two values:

- `Consumed`: enqueue the typed command in scoped FIFO event-turn transport and stop routing the triggering keyboard event.
- `PassThrough`: enqueue nothing and continue routing the same keyboard event to the focused terminal.

For a key-down event, shortcut lookup occurs before focused terminal delivery. An unmatched chord follows existing widget and terminal routing. A matched command synchronously evaluates availability and context, returns one of the two outcomes, and only a consumed command enters the FIFO. The Application Business Host drains consumed commands after widget effects are applied and remains responsible for application models, clipboard/paste policy, terminal and PTY effects, focus, windows, and fatal-error handling.

This mechanism must preserve the current event-turn ordering established by the Widget Store boundary. It must not add reducers or Harbor policy to `harbor-widget`, and it must not become a generic publish/subscribe message bus.

Key-up and IME events are not command triggers and retain existing routing. A consumed key-down must not write its normal terminal encoding or any synthetic protocol marker to the PTY.

### Configuration contract

Harbor loads overrides from the existing startup file `~/.harbor/config.toml`:

```toml
[keybindings.app.new-tab]
bindings = [{ modifiers = ["ctrl"], key = "t" }]
[keybindings.terminal.copy]
bindings = [{ modifiers = ["ctrl", "shift"], key = "c" }, { modifiers = ["ctrl"], key = "insert" }]
[keybindings.terminal.paste]
bindings = [{ modifiers = ["ctrl"], key = "v" }, { modifiers = ["shift"], key = "insert" }]
[keybindings.terminal.page-up]
bindings = []
```

For every registered command:

- omission retains the registry defaults;
- a command table's `bindings` array replaces all defaults for that command;
- an empty `bindings` array unbinds the command;
- every binding is a self-contained record with a `modifiers` string array and one `key` string;
- a `bindings` array may contain multiple records with different modifier sets and keys.

The documented binding parser must accept every default binding and the modifier/key combinations shown above. Modifier names and key names are validated independently. Unsupported modifiers, unsupported keys, duplicate modifiers, or malformed binding records are invalid rather than silently normalized to a different binding.

The complete keybinding override set is atomic. An unknown command ID, invalid command-table or binding shape, unsupported modifier or key, duplicate modifier, repeated binding within one command, or chord owned by multiple commands produces an error diagnostic and restores all default keybindings. Valid font, shell, and color settings from the same document remain applied. Binding conflicts never use registration order, table order, or last-writer-wins behavior.

The superseded `[keybindings]` command-to-string-array format is not accepted through a compatibility path. It is an invalid keybinding shape and follows the same diagnostic and atomic fallback behavior.

Defaults and the override format must be documented in the user-facing configuration example or linked settings documentation.

Native adaptation converts logical keys and the complete modifier set in harbor-widget::winit, without duplicate application event translation.

## Decisions

[ADR 0040](../adr/0040-application-command-dispatch-and-keybinding-precedence.md) defines dispatch and precedence; [ADR 0041](../adr/0041-structured-keybinding-configuration.md) defines structured bindings. Preserve [Store event-turn ordering](../adr/0029-widget-store-state-action-boundary.md) and [adapter-owned native adaptation](../adr/0031-widget-winit-adapter-owns-native-host-infrastructure.md).

## Test Plan

Exercise every default and override through real command sources: consumed UI chords execute once without normal or synthetic PTY bytes; Ctrl+C without selection queues nothing and interrupts once; always-copy with no selection consumes and writes empty clipboard text; alternate-screen scroll forwards the same event. Rail controls and shortcuts share typed dispatch and preserve parameter/focus policy.

- **Configuration unit tests:** Defaults, replacement, multiple structured bindings, empty-`bindings` unbinding, nested command-ID reconstruction, unknown IDs, malformed command tables and binding records, rejection of the superseded string-array format, unsupported modifier and key names, duplicate modifiers, duplicate bindings, cross-command conflicts, and preservation of valid non-keybinding fields after keybinding fallback.
- **Registry tests:** Unique stable IDs, unique default chords, correct palette visibility, deterministic ID-to-typed-command conversion, and complete expected default table.
- **Widget routing tests:** Consumed action stops focused delivery; pass-through action continues the same event; unmatched chords remain unchanged; key-up and IME do not invoke commands.
- **Application integration tests:** FIFO ordering, one execution per source, existing tab focus policy, paste gate/confirmation behavior, primary versus alternate screen scrolling, copy clipboard effects, and PTY byte capture for consumed versus passed-through chords.
- **Documentation checks:** Update `config.example.toml`, the P6 roadmap pointer, and keyboard-mode/validation guidance only where executable evidence exists; run `scripts/check_docs.py`.
- **Manual Windows test:** Start Harbor with defaults and with a valid override; exercise tabs, selection copy, Ctrl+C interrupt, paste confirmation, primary/alternate screen scrolling, invalid override diagnostics, `nvim`, and `tmux`. Confirm consumed UI chords never appear as terminal input.

## Out of Scope

- ModifyOtherKeys and Kitty keyboard protocols.
- Search, zoom, themes, or other commands whose behavior is not implemented today.
- Mouse binding configuration or changes to existing selection and SGR mouse precedence.
- Runtime configuration reload; keybindings remain startup-only with the existing settings system.
- Backward-compatible parsing or automatic migration of the superseded command-to-string-array format.
- A command-palette UI. This work provides registry metadata and shared invocation semantics only.
- Replacing the Store/Dispatcher transport, moving reducers into `harbor-widget`, or introducing a general event bus.
