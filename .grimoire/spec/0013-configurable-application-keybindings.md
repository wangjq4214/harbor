# Configurable Application Keybindings

**Spec ID:** 0013
**Status:** Draft
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
bindings = [
  { modifiers = ["ctrl"], key = "t" }
]

[keybindings.terminal.copy]
bindings = [
  { modifiers = ["ctrl", "shift"], key = "c" },
  { modifiers = ["ctrl"], key = "insert" }
]

[keybindings.terminal.paste]
bindings = [
  { modifiers = ["ctrl"], key = "v" },
  { modifiers = ["shift"], key = "insert" }
]

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

### Seams

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Startup keybinding settings | `harbor-config` → application command registry | Nested `[keybindings.<namespace>.<command>]` tables containing structured `bindings` records, with diagnostic isolation from other settings | Validated atomic overrides or complete default bindings |
| Shortcut action result | `harbor-app` actions ↔ `harbor-widget` routing | Typed action plus synchronous `Consumed`/`PassThrough` result | Deterministic continuation or suppression of the current key-down |
| Event-turn command transport | Widget command sources → Application Business Host | Consumed typed commands in FIFO order | Host-owned execution after widget effects, preserving ADR-0029 ordering |
| Terminal command execution | Application Business Host ↔ active terminal/paste controller | Existing selection, screen mode, clipboard, paste gate, and tab state | Existing copy, paste, scroll, tab, focus, and PTY behavior through commands |
| Native input adaptation | `harbor-widget::winit` → platform-independent shortcut routing | Existing logical key and complete modifier conversion | No application-owned duplicate winit event translation |

## End-to-End Tests

### E2E: Defaults preserve current application behavior

- **Given:** Harbor starts without a `[keybindings]` override.
- **When:** The user invokes every documented default tab, copy, paste, and primary-screen scroll chord.
- **Then:** Each operation matches its pre-command behavior, including tab focus policy, paste confirmation, selection copying, and primary-screen scroll boundaries.

### E2E: UI command wins without PTY injection

- **Given:** A live terminal has focus and a PTY writer records input.
- **When:** The user presses a chord bound to a consuming application command such as `ctrl+t`.
- **Then:** The command executes once, the key-down is not delivered to terminal encoding, and the PTY receives neither the chord bytes nor a synthetic marker.

### E2E: Conditional Ctrl+C preserves terminal interrupt

- **Given:** `terminal.copy-or-interrupt` is bound to `ctrl+c`.
- **When:** The active terminal has a non-empty selection and the chord is pressed.
- **Then:** The selected text is copied, the command is consumed, and no interrupt byte reaches the PTY.

- **Given:** The active terminal has no non-empty selection.
- **When:** The same chord is pressed.
- **Then:** No command is queued, the same input continues through terminal encoding, and the PTY receives the existing Ctrl+C interrupt bytes exactly once.

### E2E: Always-copy remains distinct

- **Given:** `terminal.copy` is bound to `ctrl+shift+c` and the active terminal has no selection.
- **When:** The chord is pressed.
- **Then:** Harbor consumes the chord, performs the existing empty clipboard write, and sends no bytes to the PTY.

### E2E: Alternate-screen scroll chords pass through

- **Given:** The active terminal is in the alternate screen.
- **When:** The user presses a configured page or boundary-scroll chord.
- **Then:** No scroll command is queued and the same key-down reaches terminal encoding.

### E2E: Valid override replaces defaults

- **Given:** Valid nested command tables rebind one command, assign multiple structured bindings to another, and assign `bindings = []` to a third.
- **When:** Harbor starts and those old and new chords are pressed.
- **Then:** Unspecified commands retain defaults, replacement bindings invoke their commands, old replaced chords no longer do so, every record invokes the multi-bound command, and the command with empty `bindings` has no application shortcut.

### E2E: Invalid keybinding set falls back atomically

- **Given:** A settings document contains valid non-keybinding settings and any unknown command, malformed command table or binding record, unsupported modifier or key, duplicate modifier or binding, or cross-command chord conflict.
- **When:** Harbor loads the document.
- **Then:** It reports an error, uses the complete default keybinding set, and still applies the valid non-keybinding settings.

### E2E: Superseded string-array format is rejected

- **Given:** A settings document uses the former `[keybindings]` command-to-string-array syntax and also contains valid non-keybinding settings.
- **When:** Harbor loads the document.
- **Then:** It diagnoses the keybinding shape, uses the complete default keybinding set, and still applies the valid non-keybinding settings.

### E2E: All command sources share dispatch

- **Given:** A tab operation can be invoked from a rail control and a registered shortcut.
- **When:** Each source invokes the equivalent typed command.
- **Then:** Both travel through the same dispatcher and Host execution policy while preserving source-specific parameters and focus disposition.

## Decisions

### Structured per-command binding records

- **Choice:** Use nested command tables with a `bindings` array of self-contained `{ modifiers = [...], key = "..." }` records.
- **Reason:** Each binding owns its modifiers and key, avoiding positional pairing and Cartesian-product ambiguity while leaving room for future per-binding metadata.
- **ADR reference:** [0041-structured-keybinding-configuration](../adr/0041-structured-keybinding-configuration.md)

### Typed application commands with stable external IDs

- **Choice:** Persist stable string IDs and convert them to typed internal commands through an application-owned registry.
- **Reason:** Configuration and a future palette need stable discovery identifiers, while internal execution needs exhaustive types and should not spread string dispatch through the application.
- **ADR reference:** [0040-application-command-dispatch-and-keybinding-precedence](../adr/0040-application-command-dispatch-and-keybinding-precedence.md)

### Scoped FIFO dispatch, not a generic message bus

- **Choice:** Reuse event-turn action transport and keep Host-owned execution rather than introducing publish/subscribe infrastructure.
- **Reason:** This preserves the existing application/widget boundary and prevents unrelated lifecycle and I/O notifications from becoming commands.
- **ADR references:** [0029-widget-store-state-action-boundary](../adr/0029-widget-store-state-action-boundary.md), [0040-application-command-dispatch-and-keybinding-precedence](../adr/0040-application-command-dispatch-and-keybinding-precedence.md)

### Synchronous shortcut outcome before terminal delivery

- **Choice:** Action invocation returns `Consumed` or `PassThrough` synchronously; only consumed commands enter the FIFO.
- **Reason:** The router must decide during the current key-down whether terminal delivery is allowed, while the Host must remain the eventual side-effect owner.
- **ADR reference:** [0040-application-command-dispatch-and-keybinding-precedence](../adr/0040-application-command-dispatch-and-keybinding-precedence.md)

### Widget adapter retains native event adaptation

- **Choice:** Application commands consume platform-independent key chords; the binary does not duplicate winit-to-widget key conversion.
- **Reason:** Native event adaptation belongs to the reusable widget winit boundary, while application policy belongs to the Host.
- **ADR reference:** [0031-widget-winit-adapter-owns-native-host-infrastructure](../adr/0031-widget-winit-adapter-owns-native-host-infrastructure.md)

## Test Plan

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

## Future Evolution

- A command palette may enumerate palette-visible registry entries and submit the same typed commands.
- Newly implemented user operations may add registry entries, defaults, and configuration IDs without changing the dispatch contract.
- Multi-stroke sequences or additional per-binding metadata require an explicit configuration-contract extension; the current `modifiers` field represents simultaneous modifier keys only.
