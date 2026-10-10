# Modern SGR: T0003 scoped acceptance

## Snapshot and execution contract

- Sources: [T0003](../.grimoire/ticket/0015-modern-sgr-decorations/T0003-capability-neovim-acceptance.md), [Spec 0018](../.grimoire/spec/0018-modern-sgr-decorations.md), [ADR 0051](../.grimoire/adr/0051-modern-sgr-decoration-and-blank-retention-policy.md), [ADR 0052](../.grimoire/adr/0052-conpty-live-primary-producer-geometry-and-styled-tail-clipping.md), [validation policy](validation.md).
- Baseline: `cb6ee0aa0eaae2250807c56b8e666d9853884a41`; working tree initially clean. Production dirty scope: `crates/harbor-terminal/src/parser/xtgettcap.rs` and `parser/status_strings.rs`. Additional scope: `render/decoration_tests.rs`, `tests/modern_sgr_acceptance.rs`, the two fixtures below and these documentation updates. No commit/staging, identity change, new PTY writer or ConPTY bundle change.
- Binary: `cargo build --bin harbor`, debug Harbor 0.1.0, no HMR. SHA256: `4aa5db89a0a3a7697832601c9f032340abc6a7c6697da1363424df6dcb2b7151`. Final test-only boundary coverage and documentation edits do not change this binary's production source.
- Method: existing ticket approach -> implementation -> focused/workspace/GPU/native checks -> inline combined-diff review and criterion check -> final gates. No new plan file or delegation; the existing ticket supplies sequencing. The only production deviation is the demonstrated status-replay fix below.
- Outcome: **PASS for this configured native Windows modern-SGR slice**, not broad application, transport or release acceptance. Remaining #162 palette/mouse/box-drawing scope is not completed by this record.

## Environment

- Windows cmd reports `10.0.26300.9457`; the synthetic fixture's OS API reports `Microsoft Windows NT 10.0.26300.0`.
- Bundled x64 ConPTY: file version `1.24.2607.10001`, product version `1.24.260710001`; `third_party/conpty/x64/conpty.dll`, copied by the normal build into `target/debug/conpty`.
- Native path: newly launched `target/debug/harbor.exe` -> dedicated cmd tab -> native Neovim / PowerShell; no WSL, SSH, tmux or native Unix path.
- Neovim: `NVIM v0.12.5`, Release, LuaJIT `2.1.1774638290`; native executable under the installed Scoop Neovim `0.12.5/bin/nvim.exe`.
- PowerShell synthetic fixture: `7.6.6`. Rust `1.97.1 (8bab26f4f 2026-07-14)`, Cargo `1.97.1 (c980f4866 2026-06-30)`, `x86_64-pc-windows-msvc`; Python `3.12.10`.
- GPU readback: AMD Radeon RX 7900 XT, Vulkan, AMD proprietary driver `26.8.1 (LLPC)`, wgpu 30. Actual adapter/device/queue, encode, raster and mapped readback ran; no adapter-less skip.
- Display observation: primary `3840x2160`, scale `1.5x`. Native screenshots were also observed at full resolution. Existing startup settings were used without edits. No new font-settings reload or cross-monitor DPI transition was performed.

## Reproducible fixtures and native observations

Run only in a dedicated Harbor test session, from the repository root. These fixtures contain public generated text; conceal is presentation, **not redaction**.

### Neovim

```cmd
nvim --clean -n -i NONE -S scripts/verify_modern_sgr_neovim.lua
```

The [Lua fixture](../scripts/verify_modern_sgr_neovim.lua) explicitly enables `termguicolors`, creates red/green `undercurl` with `sp=#ff5050/#50ff50`, blue double, yellow dotted, magenta dashed highlights, and an ERROR diagnostic using `DiagnosticUnderlineError`. It uses a scratch buffer, no swap or shada writes. No terminal identity/terminfo override or forced underline capability option is set. This establishes **configured highlighting**, not an independent automatic-detection claim or proof of which probe Neovim used.

1. Expected: distinct red/green curves, independent special color, double/dotted/dashed patterns, continuous styled spaces and complete wide CJK coverage. Observed: all appeared in the actual Harbor window. **PASS**.
2. Edit line 7, leave insert mode and request Ctrl-L redraw. Observed: appended ` EDITED T0003` survives redraw and resizing; highlight rows remain intact. **PASS**. The accepted edit/redraw used native Neovim's local RPC key injection, not a claim of physical keyboard/IME acceptance.
3. Change the window frame `1804x947 -> 850x947 -> 1804x947` on the observed 1.5x display (captured interior/window screenshots `1802x945` and `848x945`). Observed: wrapped highlighted lines remain clipped/aligned, CJK is intact, and widening restores the same edited buffer and colored patterns. **PASS** for these widths at this scale.
4. Exit with `:qa!`. Observed: alternate screen exits to the prior cmd prompt/output, with ordinary source restored. Subsequent synthetic fixture and `echo T0003 NORMAL SHELL` execute; echo response and next prompt are ordinary, not concealed or decorated. **PASS**.

For deterministic automation, the accepted run added `--listen \\.\pipe\harbor-sgr-t0003`. Neovim's local `--remote-send` sent `<Esc>7GA EDITED<Space>T0003<Esc><C-L>` and later `<Esc>:qa!<CR>`. A local `--remote-expr` inspected only fixture buffer/version/highlight/TERM values, confirming `TERM=xterm-256color`, Neovim 0.12.5 and red `sp=16732240`. Windows command-line backslash escaping may require four initial backslashes in the client argument for the two-backslash pipe name. The pipe is a test-control channel, not a replacement for Harbor's ConPTY rendering path.

### Integrated synthetic VT and original-text copy

```cmd
pwsh -NoLogo -NoProfile -File scripts/verify_modern_sgr_windows.ps1
```

The [PowerShell fixture](../scripts/verify_modern_sgr_windows.ps1) clears only that dedicated test terminal.

- Expected: all five styles over spaces and both occupied CJK cells, overline coexistence, RGB and indexed colors, inverse with explicit underline color unchanged, `24` off/re-enable retaining color and `59` restoring foreground following. Observed: distinct patterns, requested colors and ordinary OFF text. **PASS**.
- Expected: HIDDEN and LINKHIDE expose no hidden glyph, styled underline, overline, strike or hyperlink fallback; backgrounds and width remain. Observed: hidden spans are blank, reveal text and non-space fallback are visible. **PASS**.
- Select the HIDDEN row and use native Ctrl-Shift-C. Compare only to `HIDDEN: <PUBLIC-e` + U+0301 + `-` + U+754C + `> REVEALED`, without dumping clipboard contents. Observed `CopyExact=True`; selection overlay did not reveal concealed glyphs. **PASS**.
- Narrow/widen using the same frames, with an automation-generated unsubmitted `c` at the cmd prompt. Observed: complete END text, prompt and pending input survive; clipped style-only tails do not introduce live rows. On widening, overflow decoration does not return, as required by ADR 0052. After removing the `c`, execute `echo T0003 NORMAL SHELL`; response and following prompt are correct. **PASS**.

Automation caveats: background WM_CHAR text was ineffective; background modifier hotkeys inserted literal letters. Fresh snapshots established the no-ops before action-scoped foreground paste/drag/hotkeys. One early Neovim buffer was polluted by delivery attempts and discarded; only the clean restarted run is accepted. A GUI tool session expired after the Neovim exit; a fresh session confirmed the same owned test window before continuing. Clipboard writes contained only controlled launch commands/fixture data; unrelated prior clipboard text was not read or logged. Clipboard was cleared and the owned test window closed at completion.

## Automated evidence and requirement mapping

| Command/check | Observed result | Outcome |
| --- | --- | --- |
| `cargo test -p harbor-terminal --no-default-features --test modern_sgr_acceptance` | Final focused suite: 6 passed, 0 failed; includes status replay and 16/17-slot boundary coverage | PASS |
| `cargo test -p harbor-terminal gpu_ -- --nocapture` | 3 tests passed; actual T0001/T0002 GPU completion markers printed | PASS |
| `cargo test --workspace` | Final combined tree: 2310 passed, 0 failed, 5 ignored | PASS; ignored manual tests are NOT RUN |
| `cargo clippy --all-targets --all-features -- -D warnings` | Exit 0; non-fatal incremental-cache access-denied note, no lint failure | PASS |
| `cargo fmt --check` | Exit 0 | PASS |
| `python scripts/check_docs.py` | Exit 0 | PASS |
| `python scripts/checklist_summary.py` | Exit 0; inventory, not an acceptance score | PASS |
| `git diff --check` | Exit 0 | PASS |

Final gates are refreshed after test authoring/formatting and documentation updates. Raw logs/screenshots are retained locally under `target/t0003-evidence/`, not committed or used as local-only links; this summary and checked-in fixtures/tests are self-contained. Artifacts include workspace/GPU/clippy/fmt/docs logs, before-fix status failure, copy equality, clean/edited/narrow/restored/exit Neovim captures and synthetic/shell captures.

| T0003 criterion | Evidence and exact boundary |
| --- | --- |
| Boolean Su and existing bounded replies | Registry unit tests and `modern_sgr_acceptance`: exact `ESC P 1+r5375 ESC \\`, mixed TN/RGB/u8/unknown order, case sensitivity, every input split, CAN/SUB, exact 256-byte success, reply overflow and request overflow recovery |
| Identity and transport unchanged | Exact primary/secondary DA assertions; TN remains xterm-256color; only registry entry and serializer changed, no new writer/Kitty advertisement |
| Combined status exact/replay/default/bounds | Off plus all five styles, indexed/RGB underline color, conceal/overline and other attrs; every split and actual reply payload reconstructed from reset; 16/17-slot boundary test; existing legacy/default/framing tests |
| Actual configured Neovim edit/redraw/resize/DPI observation/exit/shell | Native observations above; current 1.5x scale observed, cross-monitor transitions NOT RUN |
| Versions/path/setup/revision/artifacts | Environment and binary identity above; both checked-in fixtures; local artifacts named without pretending CI exists |
| Controlled synthetic matrix | New core integration and CPU matrix plus existing modern-underlines/conceal suites run in workspace; native all-style/color/inverse/off/copy/link fixture above |
| Conceal/source and decorated blanks/ordinary rules | CPU all-style/colored/link suppression; GPU colored-curly/conceal/link case; native exact original copy; five-style combined print/ECH/EL/DECFRA non-ConPTY reflow; existing ordinary-tail and live ConPTY tests |
| GPU clipping/invalidation/retained damage and Windows visuals | Actual readback/scissor/dirty-removal/full-vs-incremental, resize/palette/metrics/1.5x projection tests; native narrow/wide observations |
| Existing regression contracts | Workspace includes replies, OSC, focus, SGR mouse, IME, synchronized output, selection/copy, alternate and 16 real ConPTY resize cases; not a new native acceptance run for every input feature |
| Required quality commands | All commands above executed successfully |
| Durable truthful documentation and exclusions | This record, scoped protocol/status/ticket updates; no parent issue closure or untested transport claims |

## Demonstrated defect and repair

A valid input with all attrs and colon-grouped colors fits the parser, but the prior DECRQSS serializer expanded colors to semicolon fields beyond the parser's 16 top-level slots. Replaying the returned status lost background/underline color. The durable regression applies the **actual reply payload** to a reset screen and compares complete cells, not a separately generated expected pen.

For modern state only, when the legacy serialization would exceed 16 slots, extended color groups now use already-supported colon forms. At/below 16 slots and for legacy/default state, original exact bytes remain unchanged. No parser/reply bounds were raised. Exact output and replay tests fail before and pass after the repair; combined-diff review found no remaining demonstrated in-scope defect.

## Exclusions

- **NOT RUN:** cross-monitor DPI transitions, native startup font changes/reload, native IME smoke, less/fzf and a broad physical-keyboard/focus/mouse application matrix. Simulated GPU DPI/resource invalidation and automated input regressions are not those native passes.
- **NOT RUN / outside this slice:** WSL/SSH/tmux, native Unix runtime, performance/daily-use release acceptance, unrelated OSC palette/clipboard/mouse/box-drawing scope and other Kitty protocols.
- No separate automatic application-discovery pass is inferred from configured Neovim highlighting. Su itself is evidenced by exact core protocol tests; this record does not claim a captured native Neovim Su exchange.
- No confirmed in-scope FAIL or BLOCKED case remains. Failed automation delivery and the repaired status regression are documented above, not silently counted as initial passes.
