# Compatibility and troubleshooting

[Terminal core](README.md) · [VT reference](vt.md) · [Embedding](embedding.md)

Compatibility depends on the complete application, transport, parser, renderer and host-input path. The live backend is Windows ConPTY; a native Unix PTY backend is not implemented.

## TERM, terminfo and identity

The [Windows environment builder](../../crates/harbor-pty/src/windows.rs) sets `TERM=xterm-256color` for spawned children and rejects a configured TERM override to another value, including case-insensitive environment-name handling. That identity is a compatibility baseline, not a claim of full xterm behavior or an exhaustive terminfo implementation.

The [XTGETTCAP registry](../../crates/harbor-terminal/src/parser/xtgettcap.rs) currently answers only `TN`, `RGB`, lowercase `u8` and `Su`. It does not install a terminfo database on the host/remote machine. `Co/colors` and uppercase `U8` are not advertised. Lowercase `u8` has a known ncurses naming collision; do not use it as generic ncurses capability proof. [DA replies](vt.md#queries-and-replies) are fixed conservative protocol identities, not the Cargo package version.

Practical rules:

- Keep the supplied identity. Do not set `TERM=ghostty`, `xterm-kitty` or another richer name to force graphics/keyboard features Harbor does not implement.
- On Unix-like **guest or remote** systems, the application needs the appropriate terminfo entry there. `infocmp xterm-256color` can inspect availability; `tput colors` reports that database's declaration, not a measured Harbor capability. These are diagnostic recipes, not native Unix Harbor launch instructions.
- Explicit true-color or underline configuration in an application can exercise rendering, but does not prove automatic feature discovery worked. Record overrides when reporting a result.
- Inside tmux, TERM normally describes tmux's virtual terminal, not Harbor. Do not overwrite it globally with the outer terminal name. Diagnose the outer and inner declarations separately.
- Environment values inside WSL/SSH/tmux may differ from the initial Windows child environment. Observe them at the failing layer rather than assuming inheritance.

## ConPTY is an active layer

The implemented native session path uses a bundled ConPTY API/host pair. [Loading code](../../crates/harbor-pty/src/windows/conpty.rs) requires the application-relative `conpty` distribution, including architecture host directories, and loads the fixed DLL with restricted search paths. It does not silently fall back to the inbox implementation when the package is incomplete. Distribute the complete matching directory beside a custom executable using this backend; do not copy an arbitrary DLL from PATH.

ConPTY can translate Windows console operations and input. What reaches Harbor need not be the bytes an application would emit directly to a Unix PTY. The reader's resize barrier orders completed reader chunks; it is not a boundary for all bytes still in the OS pipe. Terminal tests that feed bytes directly bypass this layer.

Primary live ConPTY resize intentionally preserves live-top producer geometry and may clip style-only overflow; history/non-ConPTY reflow and alternate rectangular resize differ. A missing ordinary trailing tail or clipped decoration after widening is not automatically a parser defect. See [retention concepts](concepts.md#blanks-and-retention), [styled-tail test](../../crates/harbor-terminal/tests/conpty_styled_tail_resize.rs).

For mouse/input diagnosis, distinguish Windows applications reading classic console records from those reading VT bytes with `ENABLE_VIRTUAL_TERMINAL_INPUT`. The two input paths can produce different observations.

## WSL, SSH and tmux: paths, not blanket promises

| Path | What can be said | What needs its own proof |
| --- | --- | --- |
| Native Windows child through ConPTY | The implemented live backend. | Your application version, input mode, font/scale, resize and effect behavior. |
| WSL launched from a Windows session | A guest application can be reached through the Windows process/ConPTY path. This is not a native Linux Harbor PTY backend. | Distribution, locale, guest TERM/terminfo, forwarding and input/resize behavior. |
| SSH client in the session | Remote output is still untrusted terminal output and traverses the client/transport path. | Remote terminfo, negotiated environment, locale, client/version and OSC/query forwarding. |
| tmux inside a guest or remote session | Harbor implements several protocol prerequisites used by terminal applications. | tmux virtual-terminal declarations, version/configuration and outer/inner behavior. DCS tmux passthrough wrappers are not implemented. |
| Native Unix Harbor process | The [Unix PTY source](../../crates/harbor-pty/src/unix.rs) rejects shell spawning as unimplemented. | A future backend and target-specific build/runtime acceptance; headless tests do not supply either. |

Do not infer tmux compatibility from XTGETTCAP alone: tmux itself does not emit that query, and nested applications may encounter filtering or unsupported passthrough wrappers.

## Troubleshooting by boundary

| Symptom | First checks | Avoid |
| --- | --- | --- |
| Shell startup fails | Exact executable/arguments/cwd; complete matching ConPTY bundle; startup error and architecture. | Replacing DLLs or changing TERM until the error disappears. |
| Escape text appears literally | Capture controlled producer bytes; confirm ESC is byte `0x1b`, not printed `\\x1b`; check output mode, wrapping and framing. | Treating safe consumption of an unsupported payload as implemented semantics. |
| Colors/underlines absent | Application configuration, guest/remote terminfo, actual SGR bytes, then palette/renderer behavior. Compare controlled [VT examples](vt.md#safe-examples). | Claiming auto-detection from a fixture that forces true color. |
| Wrong cursor/input in an editor | Current application cursor/keypad modes, structured input routing, duplicate IME commit, stale viewport mapping, transport input mode. | Sending both raw bytes and `TerminalEvent` for one action. |
| Mouse selects locally or reports wrong positions | Tracking versus encoding flags; queued output ingested before input; scale/content origin; cell versus physical-pixel units; application Console/VT mode. | Equating `WheelPixel` deltas with `?1016` coordinates. |
| Frozen or stale display | Output draining, wake success, `ordinary_present_eligible`, host recovery deadline, retained/live geometry and acknowledgement. | Clearing damage or resetting nested `?2026` merely to hide a scheduler bug. |
| Text shifts/disappears on resize | Actual accepted grid and resize errors, primary/alternate state, retained history bounds and documented tail policy. | Assuming reader-barrier acknowledgement means every OS-pipe byte was drained. |
| Clipboard request does nothing | Supported write selection/encoding, delivery policy, active live source and actual foreground eligibility; confirmation lifecycle and native effect error. | Weakening permission checks or logging clipboard contents. |
| Link/CWD seems wrong | URI metadata versus local path; remote hostname; allowed scheme and host authorization. | Automatically opening a remote `file:` URI as a trusted local path. |
| Search result is stale | Session/query gate, buffer generation, row proofs and explicit continuation rebase/coverage. | Reusing visible row coordinates or treating a partial scan as complete no-match. |

Use controlled public text when isolating a boundary. Do not paste arbitrary escape-rich logs into a trusted terminal or enable clipboard/URI effects just to test framing. The [security page](security.md) explains the policy boundary.

## Reporting a problem

A useful issue includes revision/dirty scope, build/features, OS/architecture, bundled ConPTY version, application and shell versions, full transport chain, TERM at each layer, relevant configuration, grid/font/scale, controlled expected/observed bytes or display behavior, and a minimal reproduction. State separately whether it reproduced in direct headless input, native ConPTY, WSL, SSH or tmux. Redact private text/URIs and never attach clipboard payloads or secrets.
