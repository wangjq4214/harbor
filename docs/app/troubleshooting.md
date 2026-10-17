# Troubleshooting and compatibility

[Application guide](README.md) · [Configuration](configuration.md) · [Commands](commands.md) · [Releases](releases.md)

## Collect useful diagnostics

When reporting a problem, include the Harbor version, Windows build, shell/application versions, configuration path, reproduction steps, and expected versus observed behavior. Mention display scaling, resize, active tab, ongoing output, or IME when relevant. Share only minimal, redacted logs/configuration; do not publish clipboard contents, secrets, or unrelated terminal output.

Harbor writes JSON events to `harbor.log`, not stderr:

| Build | Location | Default filter |
| --- | --- | --- |
| Debug | Harbor's launch working directory | INFO |
| Release | `~/.harbor/harbor.log` | WARN |

Set `RUST_LOG` before starting Harbor to override the filter. For example, from PowerShell in an extracted package directory:

```powershell
$env:RUST_LOG = "info"
./harbor.exe
```

Portable mode does not change the log location. Configuration diagnostics omit environment values and TOML snippets, but shell arguments and other events may still contain sensitive data. Review logs before sharing.

## Harbor cannot open a usable shell

- Keep `harbor.exe` and the complete adjacent `conpty/` payload together. Re-extract a matching package or use the [checkout restoration instructions](getting-started.md#1-launch-on-windows). Do not substitute a random DLL or rely on the Windows inbox runtime.
- Check the configured program and arguments. If the configured program fails, Harbor tries [Windows defaults](getting-started.md#1-launch-on-windows), so the actual shell may differ.
- Check `shell.directory`. It must exist when a session starts. Restore a missing directory and retry; Harbor does not silently choose another directory.

## Configuration did not change

1. Confirm the [selected path](configuration.md#configuration-location). Restart after adding or removing `harbor.portable`; portable mode never substitutes home settings.
2. Check the full TOML document. Invalid recognized settings or bindings reject the whole reload. Startup may instead recover to defaults.
3. Check timing: fonts/colors/bindings apply live, shell settings affect new tabs only, and OSC 52 policy needs restart. Existing shells do not receive a new environment or `cd` command.
4. Try manual reload (default **Ctrl+Shift+R**). Rewriting identical bytes does not retry a failed reload. A missing file retains settings; save a valid empty document to restore defaults.
5. Inspect configuration notices and logs. Unknown ordinary keys produce warnings; binding errors are strict. If watching fails, try manual reload or restart Harbor.

Notices can be dismissed without taking focus. Successful or unchanged reloads do not show a success notification. Repeated identical automatic errors may be suppressed; a manual retry can report them again.

## Keys, selection, or paste behave differently

- Ctrl+C without selection reaches the shell; Ctrl+Shift+C copies even without selection. A successful copy clears selection. Alternate-screen apps receive PageUp/PageDown/Home/End instead of local history commands. Child mouse tracking can take precedence over local selection and scrolling.
- Check [bindings](commands.md), including conflicts and explicit unbinding. Shift+Insert is not a built-in paste shortcut. Custom function-key tokens such as `"f3"` are not accepted, despite the built-in search shortcuts.
- Close or refocus search if editing keys do not reach the terminal. Clipboard shortcuts inside search edit the query. IME composition may consume Enter/Escape. A confirmation window blocks terminal input while open.
- Multiline paste confirmation depends on bracketed-paste mode and internal newlines; trailing newlines alone do not trigger it. A preview is not a shell safety guarantee. [OSC 52 write policy](configuration.md#clipboard-policy) is independent of paste; clipboard reads are always denied.

## Search says No Result, partial, or changes its count

`No Result` also appears for an empty query and briefly while work is pending. `Searching…` appears after a short delay. Search is case-sensitive literal text, limited to retained current-buffer content and 1024 matches; the field accepts at most 128 UTF-8 bytes. Use next/previous to select a result: receiving results does not automatically scroll.

Output, eviction, reset, and resize can change results or clear the selected result. `partial/limited` means coverage is incomplete. Search cannot recover clipped or evicted text, search files, or search other sessions.

## Transparency or fonts look different

`colors.background_opacity` changes terminal content, not application appearance or whole-window Acrylic. A six-digit background color is opaque unless opacity overrides it. Backdrop rendering can fall back to an opaque background.

Terminal fonts are independent of UI fonts. An unavailable family can fall back to a system font. Missing-glyph fallback does not imply full complex-script shaping or ligature support. Font-driven resize can reflow or clip text; history is not unlimited.

## Current limitations

- Native Unix shell sessions are not implemented. This is separate from running WSL or SSH inside a Windows-hosted session; compatibility with every remote/application combination is not guaranteed.
- Kitty keyboard/graphics protocols and OSC 52 clipboard reads are not implemented.
- Split panes, named launch profiles, and a command palette are not available.

For terminal protocol details, see [compatibility](../terminal/compatibility.md).
