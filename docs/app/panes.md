# Pane workspaces

[Application guide](README.md) · [Commands](commands.md) · [Shell integration](../../scripts/shell/README.md)

A tab is a workspace containing independent terminal sessions. Splitting does not
mirror the source PTY, replay its foreground program/history, or relaunch a saved
agent. Workspaces are not persisted and cannot move between windows.

## Split, focus and close

- **Ctrl+Shift+D** splits left/right; **Ctrl+Shift+E** splits top/bottom.
- Click a terminal to focus it. **Ctrl+Shift+] / Ctrl+Shift+[** cycle panes in
  creation order, wrapping (not directional/geometric navigation).
- Drag a divider to change its ratio. **Ctrl+Shift+M** maximizes/restores the
  focused pane without destroying siblings or the split tree.
- **Ctrl+W** closes only the focused pane. **Ctrl+Shift+W** closes the entire tab,
  as does its rail close control. Explicit `app.close-active-tab` overrides still
  mean whole-tab close; rebinding does not change command meaning.
- A session shell exiting closes that pane; a foreground program finishing does
  not. Closing a leaf collapses its redundant split and focuses the first surviving
  pane in creation order. The last pane closes its tab; the final tab closes the
  window without a replacement shell.

Commands are also available through the palette and the configurable registry.
See [all IDs/defaults](commands.md#built-in-shortcuts) before changing bindings.
Search, input and selection operate on the intended session, not every pane.
Pending paste/OSC 52 confirmation remains tied to its original live source;
closing or exiting that source cannot redirect approval to a sibling. The existing
separate confirmation-window input gate is unchanged.

## Environment and directory inheritance

A split uses the source's **current recognized** environment/directory, not its
initial launch directory. Recognition is opt-in: source the existing
[PowerShell or bash integration](../../scripts/shell/README.md#setup) deliberately.
OSC 7 alone is not sufficient. PowerShell reports local location; bash is supplied
an explicit WSL distribution or direct-SSH target. That declaration does not
verify the target, discover nested environments, or guarantee zero-configuration
recognition. Integration is not installed automatically in the new shell.

Only requesting a split authorizes the inherited connection/directory entry;
receiving/rendering metadata alone never connects or changes directory. Normal
WSL/SSH authentication still applies. A pending inherited launch has a dedicated
in-window authentication terminal and Cancel action, without clipboard access.
Readiness is attempt-qualified; merely spawning `wsl.exe`/`ssh.exe` is not success.

Missing/unusable recognition, startup/connection/cwd failure or the current
60-second startup deadline selects the accepted default `[shell]` policy after
failed-attempt cleanup. Its executable, arguments, directory and environment are
not merged with the failed source. A notice identifies the default-shell result;
it is not inherited success. If default launch also fails, existing sessions
survive and no falsely live pane is published. A later shell exit closes the pane,
not another fallback/restart. This exception applies to split only: named-profile
fail-visible behavior and ordinary New tab launch policy remain separate.

## Geometry and hidden work

New ratios start at half and clamp to 0.05–0.95. Each split reserves a 6 logical
pixel divider and, where possible, 24 logical pixels per immediate side along its
axis; when less than 48 remain, both sides receive half. This is not a recursive
minimum-window-size guarantee. Each terminal subtracts 16 physical pixels of
padding per edge and floors by its accepted cell metrics, with at least 1 row × 2
columns. Tiny panes clip rather than enlarging siblings; invalid/zero geometry
and out-of-range PTY targets are rejected.

All visible panes can update regardless of keyboard focus. Maximized siblings and
inactive-tab panes keep their last valid grid and bounded output ingestion, but
have no mounted terminal draw/cursor schedule provider. Restore measures current
allocation/DPI. Ordinary native resize admission is at most two attempts per Host
turn; that is not a constant-total-work or latency promise. Configuration reload
still coordinates applicable resources for all live sessions, including hidden
ones, without broadcasting one pane's size to differently sized siblings.

## Verification boundary

The implementation has model, mounted-input, offscreen WARP and real local ConPTY
coverage. A bounded dedicated native-window probe also exercises local pwsh
horizontal/vertical inheritance, unknown-context fallback, output and lifecycle.
It uses custom single-key bindings and HWND-targeted messages, not physical
Ctrl chords, manual IME/candidate or clipboard observations. Real controlled
WSL/direct-SSH inheritance/authentication/failure cases, cross-monitor DPI and
remaining native interaction/resource acceptance are still pending. See
[T0005 evidence and E1–E12 dispositions](../../.grimoire/ticket/0002-pane-workspace/T0005-windows-acceptance-and-scope-evidence.md).
No complete runtime acceptance or broad remote compatibility is claimed.
