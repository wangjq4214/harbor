# OSC Indexed Palette Acceptance

## Scope and revision

This record covers [spec 0019](../.grimoire/spec/0019-osc4-osc104-palette.md) and [ADR 0053](../.grimoire/adr/0053-bounded-session-osc-palette-policy.md): indexes 0-255, hexadecimal RGB set/query, whole-request validation followed by ordered execution, startup RGBA reset, session lifetime, bounded replies, and retained indexed recoloring. It does not claim broader xterm color syntax, special indexes, or completion of N11.

- Run date: 2026-10-10.
- Base revision: `4f720937477ecaa3080ab038dcfa0012745d6eed`, with uncommitted palette changes.
- Executed production scope: `crates/harbor-config/src/color.rs`, terminal OSC routing/color/palette handlers, screen color state, and the existing coherent update/renderer consumers.
- Test scope: new `crates/harbor-terminal/src/palette_tests.rs`, handler tests in `parser/osc_palette.rs`, and GPU readback in `render/decoration_tests.rs`.
- Runtime probe: `scripts/verify_osc_palette_windows.ps1`, **OSC palette probe v1**.
- Final runtime executable: debug Harbor 0.1.0; SHA-256 `8148B57377DD4835140122811BF5B87E6626B83E327720838E0B10571B3C3E03`.
- Executed probe SHA-256: `BD2E7878108CC78672BE2B90DD473DF9E3EDA61FA90691E54CFFAC39586E3BA3`.
- Existing user edits to grimoire context/spec/ADR were present before implementation; they are not unrelated production changes or a claim of committed evidence.

## Environment

- Windows build: `10.0.26300.9457` (console banner); .NET reports `10.0.26300.0`.
- Architecture/toolchain: native Windows x86-64, `rustc 1.97.1`, `cargo 1.97.1`.
- Bundled ConPTY DLL file version: `1.24.2607.10001`.
- Named application workload: PowerShell `7.6.6`, `-NoProfile -File`, launched from the dedicated Harbor default `cmd` session at the repository root.
- `TERM=xterm-256color`; `TERM_PROGRAM` absent in the probe.
- Actual GPU readback adapter: AMD Radeon RX 7900 XT, Vulkan backend, AMD proprietary driver `26.8.1 (LLPC)`.
- Runtime used built-in default colors (no per-user config file), Harbor's normal translucent/background presentation, and a dedicated window containing only public test fixtures.

## Deterministic contract evidence

All named palette tests below executed with PASS. Paths are under `crates/harbor-terminal/src/` unless otherwise stated.

| Spec requirement | Evidence and assertions |
| --- | --- |
| R1: complete palette and baseline | `palette_tests::osc_palette_all_slots_baseline_aliases_alpha_and_reset`: all 256 set/query/reset slots, configured normal/bright RGBA, existing cube/grayscale formulas, ANSI aliases, alpha preservation and startup restoration |
| R2-R3: validation, order, exact replies | `osc_palette_ordered_queries_exact_terminators_and_quantization`: preceding vs later sets, repeated indexes, multiple replies, lowercase repeated-byte hex, mixed-width RGB rounding, BEL/ST; `osc_palette_rejects_entire_malformed_request_without_damage_or_replies`: invalid colors, query tokens, pair tails, negative/oversized/overflow indexes reject leading sets and replies |
| R3: reply capacity | `osc_palette_reply_capacity_is_atomic_and_does_not_suppress_sets`: exact 28-byte maximum ST reply, exactly remaining capacity vs one byte less, whole-reply drops, bounded multi-query output, later sets still executed |
| R4: reset and isolation | `osc_palette_selective_repeated_full_reset_and_default_separation`: full/selective/repeated reset, malformed lists, startup RGBA, unchanged defaults/cursor/selection and default resets leaving indexed colors alone; `osc_palette_lifetime_across_buffers_resets_and_independent_sessions`: 47/1047/1049, parked alternate, RIS/DECSTR/SGR and independent startup configurations |
| R5: semantic cells and coherent updates | `osc_palette_semantic_cells_skipped_updates_and_stale_acknowledgement`: unchanged cells/text/indexes/width/attributes, sampled slots 0/15/16/231/232/255, ANSI aliases, truecolor, complete damage, no-op sets, rejected stale acknowledgement and synchronized/skipped-frame replay |
| R5: actual retained rendering | `render::decoration::modern_tests::gpu_osc_palette_retained_layers_recolor_and_replay`: real GPU raster/readback through `TerminalRenderPipeline`, retained foreground/background/underline, ANSI aliases, truecolor comparison, skipped update, stale acknowledgement, dirty layers and reset reprojection; adapter acquired and PASS message observed, not a no-adapter skip |
| R6: framing and allocation limits | `osc_palette_fragmented_framing_cancellation_overflow_and_recovery`: every two-part split plus byte-wise BEL/ST ingestion, incomplete input, CAN/SUB, oversized set/reset requests with BEL/ST, no partial effects, following text/commands recover; `parser::osc_palette::tests::palette_actions_are_bounded_and_validate_before_delivery`: explicit 4096-byte handler limit, maximal bounded reset action collection, invalid tail rejection |
| R6: default colors and other routes | Existing default-color exact-byte/set/query/reset/alpha/lifetime tests and other OSC/parser regressions passed in the workspace suite; no parser retention, OSC 52 policy, PTY ownership or reply-buffer capacity change |
| R7: documentation and Windows path | Checklist 24.3 and only OSC 104 items in 24.4 now cite implementation/named tests; native probe results and presentation observations are recorded below |

The palette remains fixed-size, with ANSI slots authoritative in `normal`/`bright` and slots 16-255 in `extended`; no duplicated ANSI palette or write-time RGB replacement. Existing engine appearance comparison and renderer palette synchronization carry the enlarged projection without a new ownership boundary.

## Native Harbor/ConPTY probe

Build from the repository root with `cargo build -p harbor`, launch `target/debug/harbor.exe`, and run in its dedicated CMD session:

```cmd
pwsh -NoProfile -File scripts\verify_osc_palette_windows.ps1 -ReportPath "%TEMP%\harbor-osc-palette-v1-final.json" -PhaseSeconds 20
```

The probe uses raw, unechoed console input and VT output, checks CPR before palette acceptance, captures complete application-visible replies, displays samples once, then changes only palette entries. It restores console modes and the startup indexed palette on completion. The following expected and observed values matched exactly; escape spelling below is Rust byte-string notation rather than printed terminal text.

| Check | Expected / observed result | Outcome |
| --- | --- | --- |
| CPR input control | `b"\x1b[1;1R"` | PASS |
| Startup 1, BEL | `b"\x1b]4;1;rgb:cdcd/0000/0000\x07"` | PASS |
| Startup 42, ST | `b"\x1b]4;42;rgb:0000/d7d7/8787\x1b\\"` | PASS |
| Set 42 to `#123456`, ST query | `b"\x1b]4;42;rgb:1212/3434/5656\x1b\\"` | PASS |
| Set 1 to `#ff00ff`, BEL query | `b"\x1b]4;1;rgb:ffff/0000/ffff\x07"` | PASS |
| Selective `104;42;42`, query 42 | Same startup-42 ST reply | PASS |
| Selective reset keeps index 1 changed | Same magenta-1 BEL reply | PASS |
| Full `104`, query 1 | Same startup-1 BEL reply | PASS |
| Full `104`, query 42 | Same startup-42 ST reply | PASS |

Observed window phases:

- **SET:** previously printed indexed foreground, background and underline changed to the blue set color; the already-printed ANSI red alias became magenta. The truecolor comparison remained blue and matched the set foreground visually.
- **SELECTIVE RESET:** the indexed samples returned to their green startup color while ANSI remained magenta; truecolor unchanged.
- **FULL RESET:** indexed samples remained at startup color and ANSI returned to red; truecolor unchanged. Ordinary CMD input resumed after the probe.

These are presentation observations through Harbor/ConPTY, not a screenshot-derived claim of exact uncomposited RGBA: translucent backdrop/unfocused-window composition affects displayed pixels. Exact rendered colors and unchanged truecolor pixels are independently asserted by the retained GPU readback test.

Raw JSON, window-phase screenshots and command logs remain local, with no local-only artifact links committed. The JSON records executable/probe hashes and versions. Early probe drafts using `Console.ReadKey` or one-byte console reads received only ESC from the CPR control; those runs were BLOCKED and not palette acceptance. A 256-byte native console-read buffer with bounded timeout/cancellation received complete replies in both successful retries. No Harbor production workaround was added based on the early probe symptom.

## Quality gates and limitations

| Command / activity | Result |
| --- | --- |
| `cargo fmt --check` | PASS after formatting only changed Rust files |
| `cargo clippy --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --workspace` | PASS: 2320 passed, 0 failed, 5 existing ignored; existing ignored manual/performance/platform tests and one illustrative parser doctest remain excluded |
| `cargo test -p harbor-terminal --no-default-features --lib` | PASS: 675 tests, one existing ignored throughput probe |
| `cargo test -p harbor-terminal --lib osc_palette -- --nocapture` | PASS: 10 tests including the actual GPU readback; no palette test ignored |
| `python scripts/check_docs.py` | PASS |
| `python scripts/checklist_summary.py` | PASS: inventory 843 checked / 156 open / 999 total; inventory only, not acceptance |
| Integrated inline review and spec check | No confirmed production blocker in the inspected combined changes; requirements mapped to the evidence above |
| Native OSC palette probe v1 | PASS: 9 application-visible checks, plus SET/selective/full-reset presentation observations |

Some builds emitted Windows incremental-cache finalization access-denied notes; commands still exited successfully. These are cache reuse limitations, not demonstrated product failures.

Known exclusions: WSL, SSH, tmux, native Unix, configuration hot reload, other N11/Kitty protocols, the general application/lifecycle matrix, performance acceptance, and palette-specific DX12 evidence are NOT RUN or out of this slice. The scoped native probe is the named workload for this contract; it is not Neovim or broader application acceptance. Existing ignored cold-start/font/hidden-host and manual throughput checks are not promoted by this record.
