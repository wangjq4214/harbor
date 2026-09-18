# Application Command Dispatch and Keybinding Precedence

**Status:** Proposed
**Date:** 2026-09-18

## Context

Issue #104 requires configurable Harbor keybindings, explicit precedence over terminal application input, and proof that UI-only chords do not reach the PTY. Harbor already resolves widget shortcuts before delivering unmatched keyboard events to the focused terminal, and `Store<S, A>` provides FIFO event-turn transport from widgets to the Application Business Host. Future command-palette invocation should use the same application operations without coupling configuration or UI surfaces to concrete handlers.

A generic publish/subscribe message bus would also attract PTY output, redraw, window lifecycle, and other internal events that are not user-invokable commands. Using stringly typed commands throughout the application would weaken internal exhaustiveness, while hard-coded shortcut actions would keep shortcut and future palette invocation on separate paths.

## Decision

Introduce an application command layer only for user-invokable application operations. Stable string command IDs form the configuration and future command-palette boundary; Harbor parses them into typed application commands internally. A command registry supplies command metadata, default keybindings, and availability to both shortcut resolution and future command-palette presentation.

Route buttons, menus, shortcuts, and the future command palette through one application command dispatcher backed by the existing scoped FIFO event-turn action transport. For a matched keyboard shortcut, the in-tree typed `Actions` handler synchronously returns `Consumed` or `PassThrough`; only a consumed command enters the FIFO, and the Application Business Host drains and executes it after widget routing. The Host remains the command executor and side-effect owner. Do not introduce a generic publish/subscribe message bus, and do not represent PTY output, redraw, window lifecycle, or other internal notifications as application commands.

A matched command returns either `Consumed` or `PassThrough`. `harbor-widget` action invocation must propagate that result so `Consumed` stops the current keyboard route and `PassThrough` continues the same event to the focused terminal without enqueueing a command. This preserves `terminal.copy-or-interrupt` on `Ctrl+C`: copy an active selection and consume the chord, otherwise pass it through as terminal interrupt input. The separate `terminal.copy` command on `Ctrl+Shift+C` preserves always-copy behavior, including copying empty text when no selection exists, and is the copy command exposed by a future palette; `terminal.copy-or-interrupt` is configurable but palette-hidden. Two application commands may not claim the same key chord; such a conflict is a configuration error rather than registration-order precedence.

User overrides live in the existing startup TOML settings under `[keybindings]`, keyed by stable command ID with an array of chord strings. An omitted command retains its defaults, an empty array removes its defaults, and one command may have multiple chords. If any keybinding entry has an unknown command ID, invalid chord syntax, or duplicate chord ownership, Harbor rejects the complete keybinding override set and uses default bindings while continuing to apply valid non-keybinding settings. The initial configurable registry contains `app.new-tab`, `app.close-active-tab`, `app.next-tab`, `app.previous-tab`, `app.select-tab-1` through `app.select-tab-9`, `terminal.copy`, `terminal.copy-or-interrupt`, `terminal.paste`, `terminal.page-up`, `terminal.page-down`, `terminal.scroll-to-top`, and `terminal.scroll-to-bottom`. Primary-screen scroll commands consume and execute; in the alternate screen they pass through. Search and zoom remain unregistered until implemented. Parameterized typed commands such as closing a specific tab may use the same dispatcher without becoming keybinding registry entries.

## Consequences

- Shortcut invocation and the future command palette share command identity, availability, dispatch, and execution semantics.
- Internal application handling remains typed even though persisted configuration uses stable strings.
- UI-only consumed chords cannot leak bytes or protocol markers to the PTY, while context-sensitive commands can deliberately preserve terminal behavior through `PassThrough`.
- Existing `Store`/`Dispatcher` event-turn ordering is reused instead of adding a second generic bus or moving application policy into `harbor-widget`.
- Configuration parsing must diagnose unknown command IDs, invalid chord syntax, and duplicate chord ownership, and tests must cover defaults, overrides, unbinding, precedence, conditional pass-through, and absence of PTY injection.
- Commands are registered only when their behavior exists; future search, zoom, menu, and palette work can extend the registry without creating placeholder handlers now.
