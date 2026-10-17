# Windows VT mouse byte probe

[`scripts/terminal/verify_mouse_windows.py`](../scripts/terminal/verify_mouse_windows.py) is a Python standard-library-only interactive probe. Run it **inside the Harbor executable you want to validate**, using Python 3.10 or newer. It requests terminal modes through application output and reads application-visible bytes with Win32 `ReadFile` and `ENABLE_VIRTUAL_TERMINAL_INPUT` (input code page 65001). It does not call Harbor's encoder directly.

This is an application-side observation **after Console/ConPTY processing**, not a tap of Harbor's input pipe. The backend can consume queries, translate encodings, or lose information. Record the actual session's backend/version separately; a failure at this boundary alone does not identify which component failed. Probe tests and native acceptance remain separate in the [multi-encoding evidence record](multi-encoding-mouse-acceptance.md).

## Start and exit

In a new Harbor terminal, change to the repository root and run:

```powershell
python .\scripts\terminal\verify_mouse_windows.py --label "Harbor debug build; device=...; DPI=...; ConPTY/backend=..."
```

Do not redirect stdin/stdout or pipe the probe through another command. The default is SGR cell encoding (`1006`) and click/wheel tracking (`1000`). Wait until mode replies finish (or time out) before clicking or switching modes. The display shows `Modes confirmed=True` only if all requested replies match.

A query timeout makes confirmation inconclusive for the rest of that run: DECRQM replies have no request identifier, so a later retry cannot prove that replies are fresh. Fix/record the transport issue and restart to seek confirmation. A mismatched query reply is also `NEEDS_VERIFICATION`, with expected/observed values retained. If a legacy/UTF-8 mouse prefix arrives before mode confirmation (including during a switch), or a mouse frame is malformed, the probe quarantines remaining buffered input and exits safely with a `framing_stopped` record rather than turning payload bytes into menu commands. Keep the pointer still during encoding changes, especially under `1003`.

Press `q` or Ctrl-C to exit. The probe disables its tracking/encoding modes, leaves the alternate screen, and attempts to cancel the blocking reader and restore the saved console input/output modes and input code page, including on setup failure. Use a fresh terminal: it deliberately establishes its own terminal mouse state, and does not restore an application's previous terminal mouse flags. Forced process termination cannot guarantee cleanup. Uppercase **`Z` sends RIS and can clear terminal state/history**; use it only for an intentional reset test in a disposable session.

On exit, the script prints its JSONL log path. Default logs are `%TEMP%\harbor-mouse-<timestamp>-<pid>.jsonl`; `--log <new-path>` selects a new file and refuses to overwrite an existing one. The environment record includes probe/Python/OS versions, source revision, dirty paths, and the supplied label. This source revision does **not** identify a separately built Harbor binary: put that binary's build/profile/revision in the label yourself. Logs include incoming keyboard bytes as well as mouse data; do not type secrets while the probe is running.

## Menu

Each mode change automatically queries all seven supported flags and unsupported `9`. Commands other than quit are blocked while the query batch is pending (maximum two seconds).

| Key | Action |
| --- | --- |
| `1` | Legacy: disable all extended encodings |
| `2` | UTF-8 `1005` only |
| `3` | urxvt `1015` only |
| `4` | SGR cell `1006` only |
| `5` | SGR pixel `1016` only |
| `a` | Enable all encodings; pixel should win |
| `f` | Disable winning encoding: pixel → SGR cell → urxvt → UTF-8 → legacy |
| `t` | Cycle exclusive tracking: off → `1000` → `1002` → `1003` → off (starts at `1000`) |
| `b` | Enable all tracking flags; `1003` should win |
| `d` | Disable winning tracking: `1003` → `1002` → `1000` → off |
| `v` | Repeat mode queries |
| `Z` | Explicit destructive RIS test: clears tracking and encoding flags |
| `q` / Ctrl-C | Exit and clean up |

## Suggested manual sequence

1. **Cell formats:** click the `3` in row 2, column 3 of the numbered display. Repeat left press/release under `1`, `2`, `3`, and `4`. Without modifiers, the expected bytes at `(3,2)` are:

   | Encoding | Left press | Release |
   | --- | --- | --- |
   | Legacy / UTF-8 at this ASCII position | `1b 5b 4d 20 23 22` | `1b 5b 4d 23 23 22` |
   | urxvt | `ESC[32;3;2M` | `ESC[35;3;2M` |
   | SGR cell | `ESC[<0;3;2M` | `ESC[<0;3;2m` |

   Legacy and UTF-8 reports are indistinguishable for ASCII fields. To exercise UTF-8 encoding, use a sufficiently wide window and a higher column: left press at column 128, row 2 expects `1b 5b 4d 20 c2 a0 22`. Columns 95/96 test the `7f`/`c2 80` boundary. Legacy column 224 should emit nothing, not wrap or trigger selection; absence alone requires a human-confirmed gesture and sufficient viewport size. Do not claim unexercised range tests.

2. **Independent flags/fallback:** press `a`, then press `f` four times, waiting for each query batch and clicking between steps. Queries should keep lower-priority flags enabled until explicitly cleared, while effective reports change pixel → SGR cell → urxvt → UTF-8 → legacy.

3. **Tracking and capture:** use `t` to test press/release/wheel-only `1000`, held-button drag `1002`, no-button movement `1003`, and off. Use `b`/`d` for independent tracking fallback. Tracking off should stop reports and restore Harbor's local interaction. Check that tracked gestures do not select/scroll locally, release outside the content does not leave a stale drag, and focus/capture loss recovers. The probe cannot automatically verify these host effects or prove that an expected absent event happened.

4. **Wheels:** under `4` and `5`, exercise vertical, horizontal, and diagonal gestures with the devices available to you. Decoded base codes are 64 up, 65 down, 66 left, 67 right; modifiers are Shift=4, Alt/Meta=8, Ctrl=16. Harbor maps native positive `dx` to 66 and negative `dx` to 67; gesture direction alone does not establish native delta sign. A true dual-axis native event should report vertical then horizontal, one per nonzero axis. Read chunk boundaries do **not** identify native events, so the probe cannot prove grouping, wheel increment units, original magnitudes, or zero/zero delivery without separate event/device evidence.

5. **Pixel precision/DPI:** select `5`, then use tracking `1003`. Move within one cell; pixel coordinates should change where cell coordinates under `4` would stay constant. Compare content-relative positions under different display scales, padding, window geometry and allocations. Pixel coordinates are 1-based physical pixels, not cells; `(3,2)` in pixel mode is not row 2/column 3. The probe checks syntax/ranges, not geometry correctness without an independently measured position.

6. **Reset:** in a disposable session only, press `Z`, wait for queries, and verify all supported flags are disabled and no reports arrive. Re-enable tracking/encoding to continue. Test nvim separately; this probe does not establish nvim interoperability.

For a stricter stationary cell-click test, use:

```powershell
python .\scripts\terminal\verify_mouse_windows.py --encoding legacy --expect-position 3 2 --label "stationary left click at cell (3,2); build/device/DPI=..."
```

`--expect-position` checks **every** report, including release/wheel/motion. Do not use this fixed expectation while dragging, moving, or switching to pixel coordinates. `--duration 30` exits after 30 seconds; the default waits for quit. A normal exit code means the probe completed, not that every scenario passed.

## Inspect logs and interpret results

```powershell
$log = Get-ChildItem "$env:TEMP\harbor-mouse-*.jsonl" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
$events = Get-Content $log.FullName | ForEach-Object { $_ | ConvertFrom-Json }
$events | Where-Object kind -eq 'mouse_report' | Select-Object action, code, x, y, requested_encoding, outcome, hex
$events | Where-Object outcome -eq 'FAIL' | Format-List
$events | Where-Object kind -eq 'summary' | Format-List
```

- `input_chunk` records exact `ReadFile` bytes; `output` records mode/query commands. Chunk boundaries may split or combine reports. Mouse reports are framed incrementally so embedded payload bytes such as `q` are not menu keys.
- `mode_reply` `PASS` means a specific reply matches the requested independent flag. Supported enabled/disabled replies are 1/2; unsupported `9` expects 0.
- `mouse_report` `PASS` means the observed report has valid syntax/ranges, matches the requested wire family and tracking filter, and satisfies any optional fixed position, **after mode confirmation**. It does not prove that the physical action, button/modifiers, geometry, report count, capture or device path was correct.
- A confirmed-state syntax/format/filter/position mismatch is `FAIL`. Unconfirmed-state mouse/invalid input and query timeouts are `NEEDS_VERIFICATION`, not a pass. Investigate transport interception/transcoding as well as Harbor; do not attribute a missing query directly to the encoder.
- `framing_stopped` is a conservative safety stop: inspect the raw chunk and framing reason, then repeat in a fresh run after resolving the ambiguity. It is not a native acceptance pass. Malformed confirmed-state frames still retain their separate `FAIL` record.
- Missing queries at early exit and sessions with no decoded mouse reports are `NOT_RUN`. The summary always says `overall=NOT_AUTOMATICALLY_ACCEPTED`; a clean count is not full spec acceptance.

Attach the log plus human-recorded expected/observed actions, device, DPI, actual binary/backend versions and scenario outcome to native evidence. Leave unavailable wheel units/devices, Vim/nvim, WSL/SSH/tmux, and other unrun paths explicitly unvalidated.

## Probe self-tests (not native mouse acceptance)

```powershell
python -m unittest discover -s scripts/tests -p 'test_verify_mouse_windows.py' -v
python scripts/terminal/verify_mouse_windows.py --help
```

The portable tests cover exact decoding, byte-stream splits, embedded menu-like payloads, UTF-8 boundaries, malformed input, query state/timeouts, tracking and encoding fallback, scoped verdicts, JSONL summary, exclusive log creation, setup failure and console restoration contracts. Console I/O is mocked; these checks do not exercise a physical Windows mouse session through Harbor.
