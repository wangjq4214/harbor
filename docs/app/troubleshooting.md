# Troubleshooting and compatibility

[Application guide](README.md) · [Configuration](configuration.md) · [Commands](commands.md) · [Releases](releases.md)

## Collect useful diagnostics

Record the Harbor revision/package version, debug or release build, Windows build, bundled ConPTY version, shell/application versions, selected configuration path, reproduction steps, and expected versus observed behavior. Identify active/inactive tabs, primary/alternate screen, ongoing output, DPI/resize, and IME state when relevant. Include only minimal, redacted logs/configuration and synthetic terminal text; do not publish clipboard contents, secrets, or unrelated output.

The [logging initialization](../../src/main.rs) writes JSON events to `harbor.log`, not stderr:

| Build | Location | Default filter |
| --- | --- | --- |
| Debug | Harbor's launch working directory | INFO |
| Release | `~/.harbor/harbor.log` | WARN |

`RUST_LOG` overrides the filter when Harbor starts. Portable mode changes the configuration path, **not** this log location. Ordinary successful reloads can be quiet in UI and filtered from release logs; absence of a success toast is not a failure. Configuration diagnostics omit environment values and TOML source snippets, but shell program/arguments and unrelated events may still be logged. Review before sharing.

## Harbor cannot open a usable shell

- Keep `harbor.exe` and the complete adjacent `conpty/` payload together. Restore it with the checkout fetch script or re-extract a matching package; do not substitute a random DLL or rely on the Windows inbox runtime. Missing/incomplete payload is an explicit startup error. See [runtime layout](../../third_party/conpty/README.md).
- Check the configured program and arguments. A failed configured program falls back to Windows defaults, so the actual shell may differ from your request. [Process creation](../../crates/harbor-pty/src/windows.rs) logs attempted candidates.
- Check `shell.directory`. It must exist at validation **and** spawn; loss after acceptance fails the new session rather than silently choosing another cwd. Restore it and retry.
- Native Unix shell runtime is deferred; the [Unix PTY source](../../crates/harbor-pty/src/unix.rs) is a stub. This does not answer whether a particular WSL/SSH session inside Windows works.

## Configuration did not change

1. Confirm the [selected path](configuration.md#configuration-location). Marker changes require restart. Portable mode never substitutes home settings.
2. Check the full TOML document, not only the edited field. Invalid recognized settings or bindings reject the whole reload. Startup may instead have recovered to defaults.
3. Distinguish effect timing: fonts/colors/bindings are live, launch settings affect new tabs only, OSC 52 policy needs restart. Existing shells do not receive a new environment or `cd` command.
4. Try the configured manual reload command (default Ctrl+Shift+R). Watching coalesces byte changes, so rewriting identical bytes will not retry a failed resource preparation. A missing file silently retains accepted settings; use a valid empty document if you intend to restore defaults.
5. Inspect non-modal configuration notices and logs. Unknown ordinary keys generate ignored-setting warnings; binding errors are strict. Watch observation faults retain accepted state and manual reload remains usable. Setup failure may need restart to re-establish watching.

Configuration notices are manually dismissible and do not take focus; routine successful/no-change reloads do not create a success feed. Repeated identical automatic errors are suppressed, but explicit retries can report again. Resource recovery errors must not be read as successful rollback. See [notice policy](../../crates/harbor-app/src/notifications.rs), [host reload handling](../../src/shell.rs), [notice tests](../../src/configuration_notice_tests.rs), and [reload failure tests](../../src/configuration_reload_tests.rs).

## Keys, selection, or paste behave differently

- Ctrl+C without selection passes to the child; Ctrl+Shift+C copies even without selection. Successful copy clears selection. Alternate-screen apps receive PageUp/PageDown/Home/End instead of local history commands. Child mouse tracking can own pointer input.
- Verify [bindings](commands.md), including default conflicts and explicit unbinding. Shift+Insert is only in the example, not the built-in paste default. Function-key override tokens such as `"f3"` are not currently parsed, despite built-in search F3 shortcuts.
- Close or refocus search if editing keys appear not to reach the terminal. Query clipboard operations are editor operations; IME composition can consume confirmation/cancel keys. A separate confirmation window gates terminal input while open.
- Multiline paste confirmation is conditional on bracketed-paste mode and meaningful internal newlines; trailing newlines alone do not trigger it. A preview is not a shell safety guarantee. OSC 52 write authorization is independent of paste and always denies clipboard reads.

Evidence: [workspace routing tests](../../crates/harbor-app/src/workspace_tests.rs), [editor tests](../../crates/harbor-widget/tests/text_input.rs), [paste controller/tests](../../src/dialog.rs), and [terminal input tests](../../crates/harbor-terminal/src/terminal_tests.rs). Native keyboard layouts, IME candidate placement, held-key changes, and clipboard behavior require platform evidence beyond headless routing tests.

## Search says No Result, partial, or changes its count

`No Result` is also the empty/initial and short pending label, not always completed-negative evidence. The delayed `Searching…` hint is presentation-only: matching starts immediately. Matching is case-sensitive literal text, limited to retained current-buffer content and 1024 ranges; the field accepts at most 128 UTF-8 bytes. No regex, normalization, cross-session archive, or automatic initial result navigation is provided.

Output changes, eviction, reset, and resize can invalidate results. Positive ranges are revalidated before publication/navigation; partial/limited coverage is not complete coverage. A newly accepted batch may clear the selected ordinal. Search cannot recover clipped or evicted text. See [search implementation/tests](../../crates/harbor-app/src/search_tests.rs) and [bounded matcher evidence](../terminal-literal-search.md).

## Transparency, fonts, or glass look different

Terminal `colors.background_opacity` does not select application appearance or change whole-window Acrylic. A six-digit explicit background is opaque unless opacity overrides it. The [native backdrop backend](../../src/backdrop.rs) has fallback paths; dependency OS baselines do not establish a Harbor support matrix.

Search uses local `BackdropFilter` blur/refraction with application-owned [material presets](../../crates/harbor-app/src/appearance.rs), not a screenshot imitation or only whole-window Acrylic. Its reusable field, fixed result slot, and transparent icon controls are composed in [workspace code](../../crates/harbor-app/src/workspace.rs). Tests establish composition/state seams, not visual acceptance across GPUs, DPI, backgrounds, or remote desktops. Record actual expected/observed images before asserting appearance quality.

Terminal fonts are independent of UI fonts. A valid unavailable family can fall back to the system primary; missing glyph fallback differs from full complex-script shaping/ligature support. Font-driven resize has bounded retention/clipping, not unlimited lossless history. See the [text backend](../../crates/harbor-text/src/font.rs), [current inventory](../current-status.md), and [validation matrix](../validation.md).

## Compatibility and evidence limits

| Area | Interpretation |
| --- | --- |
| PowerShell/cmd, full-screen apps, IME, mouse, resize | Implementation and scoped tests/evidence exist; use the exact [protocol checklist](../protocol/checklist.md) and [validation procedure](../validation.md), not a blanket compatibility claim |
| WSL/SSH/remote application combinations | Unrecorded transport/application cases are **unverified**; distinguish Windows-host behavior from native Unix runtime |
| Kitty keyboard/graphics, OSC 52 reads | Not implemented in the current [product inventory](../current-status.md); parsing a frame safely is not protocol support |
| Splits, profiles, command palette | Not implemented application workflows; command metadata is not a palette |
| Search glass, ongoing output/resize, large-history cost | Source/automated seams do not establish complete native visual, interaction, or measured-performance acceptance |
| MSI/portable/public releases | Packaging automation is not proof of ordinary-user install, live publication, or downloaded-artifact smoke; see [release limits](releases.md#verification-boundary) |

Follow [Validation](../validation.md) when reporting PASS/FAIL/NOT RUN/BLOCKED. Do not report an unperformed native test as passing or an unverified combination as absent functionality.
