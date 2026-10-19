# Getting started

[Application guide](README.md) · [Configuration](configuration.md) · [Commands](commands.md)

## 1. Launch on Windows

For packaged builds, choose the [MSI or portable ZIP](releases.md). Launch an MSI installation from its Start Menu entry. For ZIP, extract the **entire** archive and launch `harbor.exe`; keep its adjacent `conpty/` directory intact. Copying the executable alone is insufficient.

For a source checkout with Rust and Windows C++ build tools, run from the repository root:

```powershell
./scripts/setup/fetch_conpty.ps1
cargo run
```

The fetch restores the pinned runtime; rerun it when the pin changes. See the [ConPTY layout and restoration guide](../../third_party/conpty/README.md).

Harbor first attempts a configured shell, if any. Windows defaults are tried in order: `pwsh.exe`, `powershell.exe`, nonempty `COMSPEC`, then `cmd.exe`. If a configured program fails, its arguments are **not** reused with fallback shells; an args-only configuration applies to the selected default shell.

## 2. Work with tabs

- **Ctrl+T** creates and activates a terminal tab.
- **Ctrl+Tab / Ctrl+Shift+Tab** cycle forward/backward, wrapping around.
- **Ctrl+1…Ctrl+9** select the corresponding one-based position when it exists. Ctrl+9 means ninth, not last.
- **Ctrl+W** closes the active tab; closing the final tab requests window closure. Treat this as closing a terminal session, not hiding it.
- Use the left tab rail to activate or close a tab or create another. It expands at 900 logical pixels of window width and compacts below that; there is no configuration option for this breakpoint.

Background output marks a tab unread; activation clears the indicator. Closing an active tab selects its right neighbor, otherwise its left neighbor. Tabs are sessions, not split panes or named launch profiles.

To discover these and other existing actions, press **Ctrl+Shift+P**, type part of a command label or ID, select with Up/Down and press Enter. Escape closes the palette after cancelling any active IME composition. See [palette behavior and verification limits](commands.md#command-palette).

## 3. Review, select, copy, and paste

In ordinary terminal review, use the wheel or scrollbar to inspect retained history. Drag to select cells; double-click selects a word and triple-click a logical line. **Ctrl+Shift+C** copies selection (empty text if there is none). **Ctrl+C** copies a nonempty selection, otherwise reaches the child as interrupt input. A successful terminal copy clears selection. PageUp/PageDown and Home/End review the primary buffer; in alternate-screen applications these commands pass through instead. Mouse reporting enabled by the child can take precedence over local selection/scrolling.

**Ctrl+V** pastes text. Shift+Insert is an optional binding in the [example configuration](../../config.example.toml), **not** a built-in default. Without bracketed-paste mode, text with internal newlines opens a separate confirmation window; inspect it and choose Paste or Cancel. With bracketed paste enabled by the child, text is sent directly with protocol framing. Confirmation previews do not rewrite the raw text that will be sent, so inspect pasted commands carefully.

## 4. Find output in the current session

1. Press **Ctrl+Shift+F** to open/refocus search over the active terminal.
2. Enter literal text. Matching is **case-sensitive**, without regex or Unicode normalization. The query is limited to **128 UTF-8 bytes**, not 128 characters; a control-containing, multiline, or over-limit edit is rejected rather than truncated.
3. Read the result slot. `— / N` means valid matches without a selected result; receiving results does **not** automatically scroll. Use previous/next buttons, **F3 / Shift+F3**, or **Enter / Shift+Enter while editing** to navigate with wraparound. Initial next chooses the oldest result; initial previous chooses the newest.
4. Close with the X button or **Escape inside the field** to restore terminal focus. During IME composition, Escape first cancels composition. Switching the active tab cancels/closes search; no per-tab query archive is retained.

The search field supports caret movement, Shift selection, mouse selection, Backspace/Delete, Ctrl+A/C/X/V, undo (Ctrl+Z), and redo (Ctrl+Y or Ctrl+Shift+Z). Clipboard actions inside the field edit the query, not the terminal.

`No Result` is also the initial/empty label and the short pending label: it is not always proof of a completed negative search. Work starts immediately; `Searching…` appears only after a 200 ms busy delay. `partial/limited` means coverage is incomplete. Search scans retained content in the current buffer, with at most 1024 results; it cannot recover evicted text, search files on disk, or search other sessions. Output, erase, reset, and resize can invalidate results and clear the selected ordinal; highlights/navigation revalidate current content.

## 5. Customize without restarting a shell

Create the selected [configuration file](configuration.md#configuration-location), optionally starting from [the repository example](../../config.example.toml). Save it: watching is on by default. **Ctrl+Shift+R** requests an independent manual reload. Valid colors, bindings, and terminal fonts apply live; shell launch changes apply to new tabs; clipboard policy needs an application restart. A rejected reload retains accepted settings, not startup defaults. Use [configuration diagnostics](troubleshooting.md#configuration-did-not-change) if an edit does not take effect.
