# Independent-QA corrective cycle at 7539847

## Scope and snapshot

- Base: `75398470b6941eae42373a3e3084e62a55a5d870`, after the five original delivery commits (`8c4aa60`, `8149957`, `1cbf701`, `2af7639`, `7539847`). This record belongs to one separate corrective commit; it does not amend those commits.
- Source: [Spec 0056](../../../spec/0056-pane-workspace.md), especially R1/R3/R4/R6/R7/R8 and E4/E7–E11; the independent findings and repair request reopen T0004. T0005's full native/remote acceptance remains pending.
- Explicit inline grimoire-loop: original requirements/source inspection → focused regressions/counterexamples → authorized fixes → combined verification → joint Code Risk / Intent Check → affected refresh → separately authorized path-specific staging/full cached-diff review/local commit. Loaded implement, test, review, check, loop recovery, UI/boundary/status guidance, debug and commit/format skills. Reused the ticket approach; no new plan, delegation or external approval claimed.
- Worktree initially contained only unrelated untracked `.omnidiff.toml`, with an empty index. It was not read, edited or staged. No spec, CONTEXT, ADR, GitHub or original observation JSON changes; no remote connection, tool/integration installation, global key injection or physical/OS clipboard test.
- All commands run from `D:/Code/harbor`, 2026-11-16 UTC, existing Windows x64 dependencies/fonts/ConPTY only. Local pwsh reports 7.6.9; WARP reports Microsoft Basic Render Driver / Dx12, driver 10.0.26100.9818. Existing bundled ConPTY 1.24.2607.10001 is staged by the wrapper. These are local observations, not WSL/SSH evidence.

## Independent findings: reproduction, repair and restoration

### F1 — shifted native pane-cycle chords (R6, E7/E10)

`crates/harbor-app/src/pane_input_tests.rs::native_shifted_pane_cycle_is_consumed_once` mounts the actual three-pane workspace/capture writers and feeds `WindowEvent::ModifiersChanged(CONTROL | SHIFT)` plus native logical `{` / `}` press and release through `WinitAdapter`. Original defaults produce **FAIL**, `left: []`, `right: [Pane(Previous)]` (`target/qa-repair/red-input.log`). The old direct `[` fixture had bypassed native Shift semantics.

Repair: only the two registry defaults change to logical `{` / `}` with Ctrl+Shift (US bracket keys). No new punctuation normalization, text rewriting, modifier folding or configured-key migration. The final test asserts one command per press/release, no second action, correct pane and no PTY bytes. It also proves explicitly configured `]` remains `]` (not `}`), and native Shift+`{` without Ctrl writes ordinary `{` text. Existing override/unbind/conflict/close-tab semantics remain covered. `docs/app/commands.md` and `docs/app/panes.md` explain the layout-dependent logical-symbol spelling.

### F2 — unfocused exit destroys survivor runtime focus (R1/R6/R7, E7/E9)

`PaneAllocations::publish` changes generation and generation-keyed leaves remount after a visible sibling closes. `close_pane` correctly reports no active bridge change when the exiting pane was unfocused; Host PreserveRail correctly does not request terminal focus. The missing piece was restoration of the surviving public focus identity after Fiber reconciliation.

The final mounted regression closes the unfocused middle pane, calls production `sync_panes`, reconciles and types `SURVIVOR`, with **no request_focus after closure**. It covers both handle focus and pointer-focused external leaves, and asserts genuine rail/editor focus identities are not moved (editor still receives its text). A controlled restoration-disabled counterexample reproducing the original fallback makes the corrected regression **FAIL**, empty writer versus `SURVIVOR` (`red-focus-corrected.log`); restoring the implementation passes. The earliest test version had an incorrect initial focused-pane assumption; that failure is not used as sole product proof.

The real ConPTY `mounted_native_mixed_panes_route_markers_under_output_resize_and_close` now sends `exit` to the unfocused middle shell, consumes the real exit observer/reducer, syncs/reconciles without manual focus, then types a command through the Runtime. The surviving clicked shell must report its own `HARBOR_PANE_TEST=2` as `SURVIVOR_FOCUS_2`, not just create a shared-path marker from any pane. A partial handle-only fix failed this native case because clicks focus the inner external leaf; the final ancestor-handle snapshot fixes that path too. Final lifecycle suite passes all five tests.

Repair is generic Runtime reconciliation: snapshot the nearest stable focus handle before unmount, and restore it only when the old Fiber is invalid and no explicit focus request is pending. Retired/disabled targets retain existing fallback behavior. Pane allocation generations, stale-observer rejection, session identity, pointer/preedit unmount cleanup and clipboard/IME target retirement are unchanged. This is not unconditional Host focus stealing.

### F3 — late pending-auth prompt clipped by fixed presentation (R3/R4/R6)

`pane_layout::tests::gpu::pending_authentication_late_prompt_is_visible_and_reachable` mounts the real reserved-session popup, renders a 40×100 terminal containing a synthetic green `Password:` prompt at row 20 and checks GPU pixels plus cursor/preedit reachability. Original production projection fails **`late Password prompt clipped at DPI 1`** (`red-auth.log`), before the resize repair.

Repair: the popup observes its actual mounted terminal allocation. One target-qualified Host call per pending turn submits current allocation/DPI/accepted-metric intent and advances at most one normal resize transaction. The 240dp presentation no longer projects an unrelated captured grid. Drawing and layout callbacks never resize. Normal published sessions retain the existing two-attempt budget, with one extra bounded admission for the sole unpublished session; no draw-driven sweep or new resize engine.

Final WARP regression passes at 1×/1.5×/2×, independently checks row capacity from 240dp/current physical metrics, verifies drawing alone leaves the original 40 rows unchanged, reads visible prompt pixels, and places the IME candidate inside the popup. `pending_authentication_host_resize_preserves_late_prompt_and_identity` adds a **real local pwsh/ConPTY** child that emits the synthetic row-20 prompt and waits; the mounted 1.5× allocation converges to **20×144**, the prompt remains in the visible grid after asynchronous native repaint, and cursor is in range. Reserved binding remains unpublished, source binding unchanged, and closure completes the native shutdown receipt.

Test infrastructure corrections are not product failures: a fake blocking reader without interrupt capability cannot exercise native resize, so GPU rendering uses a headless engine and the separate real-ConPTY case checks reader/barrier/repaint behavior. A direct synthetic engine mutation was replaced by actual child output for native repaint. Existing capture-writer auth tests still prove reserved-session input across source changes. No credentials or remote authentication were tested; existing Host clipboard eligibility denies all pending splits and unpublished bindings.

### F4 — raw readiness ahead of buffered OSC 52 parsing (R3/R6, E4/E8)

This concern is **confirmed, not refuted**. `src/split_launch/tests.rs::queued_startup_clipboard_before_readiness_cannot_escape_after_publication` uses the real reader-thread queue: 24 startup chunks, OSC 52 in chunk 25, attempt-qualified readiness in chunk 26. The reader reports completion only after all chunks are enqueued, remains alive, and never touches the OS clipboard. Original poll drains just 16 chunks, publishes from raw readiness, and later foreground parser delivery emits the old request: **FAIL `startup OSC52 escaped after attach`** (`red-backlog.log`). The old already-parsed-output fixture could not expose this backlog.

Repair: raw `StartupProbe` retains only the readiness-end byte offset; the terminal exposes monotonic parsed-output progress. Each poll keeps its existing bounded drain and discards external startup side effects through the existing metadata-only collector. Publication requires parser progress through the observed marker and a live source/result. No wait-for-empty loop (which could starve under continuous output), unbounded draining, second clipboard authorization path or new OS effect is introduced.

The final queued regression explicitly requires first-poll Pending with exactly **16 × 9 parsed bytes**, bounded subsequent readiness/claim/attachment, no startup ClipboardWrite after remaining output drains, then a normal live OSC 52 parser event **without manually changing delivery policy**. Joint review removed an intermediate extra `Discard` policy change: permanently changing a factory's parser retention mode could suppress legitimate post-publication requests. The final repair leaves factory/TabManager delivery policy and Host authorization unchanged.

### Minor documentation correction

`docs/app/getting-started.md` now explains per-session unread clearing: focusing one pane does not clear another session's unread contribution to the tab indicator. Pane docs also distinguish focused-leaf successor selection from preserving actual focus on an unfocused exit.

## Changed API handoff

- New `Terminal::parsed_output_bytes() -> u64`: monotonic, saturating byte progress for this engine's output stream. Direct `process_output`/`put_bytes` feeds also advance it; do not mix unrelated streams when comparing a raw-reader watermark. `StartupProbe` records its marker-end offset before publishing READY with Release; polling observes status with Acquire before comparing the offset. No terminal-content retention or full-history scan.
- New `WorkspaceUiController::advance_split_authentication(target, scale, surface, metrics) -> bool`: Host-turn-only, bound to the current popup's exact reserved target. It rejects stale/mismatched popup targets, validates allocation through the existing terminal geometry helper, coalesces normal resize intent and advances at most one transaction; true means committed geometry needs a frame. Errors use existing diagnostics. Callers must not invoke it from draws.
- Host `TabCoordinator::advance_split_authentication` supplies accepted factory metrics; `ActiveSession::advance_pane_split` calls it once per pending turn using current viewport. Internal bridge helper performs normal request/advance; unpublished resources remain outside TabManager until qualified claim/attachment.
- Runtime `focused_handle` / `clear_dead_targets` exchange is internal. No public Fiber identity, weakened generation check, new command ID, changed explicit binding meaning or clipboard permission API.

## Combined verification

`T` expands exactly to `pwsh -NoProfile -File scripts/dev/test_cargo.ps1 --locked --offline`. Logs are local ignored artifacts under `target/qa-repair/`; the causal results and native extracts here are durable. Final production source is the corrective tree over the base above; later changes limited to evidence/docs or formatting/test assertions do not establish a different production behavior.

| Command | Result / artifact |
| --- | --- |
| `T --workspace` | **2953 passed, 0 failed, 232 ignored**, 78 harness summaries; `workspace-final.log`. Ignored resources are not passes. |
| `T -p harbor-terminal --no-default-features --lib` | **876 passed, 6 ignored**, `core.log`. |
| `T -p harbor-app --lib pane_ -- --test-threads=1` | **16 passed, 2 GPU ignored**, `pane-mounted.log`. |
| `T -p harbor --bin harbor split_launch::tests -- --test-threads=1` | **13 passed, 5 resource ignored**, `split-final.log`; includes bounded queued-reader boundary and live delivery. |
| `HARBOR_TEST_GPU=dx12-warp T -p harbor-app --lib gpu -- --ignored --test-threads=1 --nocapture` | **8 passed**, `app-gpu.log`; final allocation assertions additionally refreshed with `pending_authentication_late` filter, **1 passed**, `auth-final.log`. |
| `cargo build --locked --offline -p harbor-pty --example launch_probe` | PASS, `probe-build.log`. |
| `HARBOR_TEST_LAUNCH_PROBE=D:/Code/harbor/target/debug/examples/launch_probe.exe HARBOR_TEST_GPU=dx12-warp T -p harbor --bin harbor configuration_reload_tests -- --ignored --test-threads=1` | **24 passed**, `host-resources.log`. |
| `T -p harbor --bin harbor pane_lifecycle_tests -- --ignored --test-threads=1 --nocapture` | **5 passed**, `lifecycle-final.log`. Twelve-cycle samples: baseline **152 handles**, each cycle **152**, final **142**, elapsed **3046ms** in the recorded final run. Bounded characterization, not a universal leak/latency threshold. |
| `T -p harbor --bin harbor split_launch::tests::pty -- --ignored --test-threads=1` | **4 passed**, `split-native-final.log`; local only. |
| `T -p harbor --bin harbor paste_command_tests::pty -- --ignored --test-threads=1` | **1 passed**, `paste-native.log`; original-session exit race, no OS clipboard. |
| `T -p harbor-pty pty -- --ignored --test-threads=1` | **3 passed**, `pty.log`. |
| `cargo check --locked --offline --workspace --all-targets` | PASS, `check-final.log`. |
| `cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings` | PASS, `clippy-final.log`. Existing manifest-only unused `parking_lot` warning remains; no suppression/hook bypass. |
| `cargo fmt --all -- --check`; `git diff --check` | PASS, `fmt-final.log` / direct whitespace check. |
| `RUSTDOCFLAGS='-D warnings' cargo doc --locked --offline --workspace --all-features --no-deps` | PASS, `rustdoc.log`. |
| `python scripts/docs/check_examples.py` | PASS, **17 examples**, `examples.log`; no_run compiles only. |
| `python scripts/docs/check_docs.py` | PASS, refreshed after evidence edits; `docs-final.log`. |

The full workspace, strict checks and affected split/native checks are refreshed after the joint-review delivery-policy correction. App/core/Host configuration resources do not consume the changed startup polling policy, so their explicit runs remain applicable. Obsolete checklist/docs paths, unrelated release/HMR/performance/font/application lanes and remote/physical clipboard suites are NOT RUN, not invented successes. Unchanged script suites retain historical evidence only; they were not rerun as part of this Rust/doc repair.

## Native-window observation and applicability

The existing isolated `observe_panes_windows.ps1` probe was also rerun, using a fresh portable output directory after `pwsh -NoProfile -File scripts/dev/run_harbor.ps1 -BuildOnly`. It posts custom keys/resize only to its verified owned PID/HWND, not physical defaults or arbitrary windows. First corrective run: `target/qa-repair/native-final/observation.json`, 11:03:46–11:04:13 UTC, app PID **33860**. Four pwsh PIDs **25308/2740/30788/34752**, all at the fixture's changed `current` directory; original PID **34752** remains responsive. Actual inherited horizontal/vertical children, unknown-context default startup count **2**, 2000 synthetic producer rows, natural/explicit/tab-close counts **4→3→2→0**, exit **0**. Five-second quiet sample: **46.875ms CPU**, handles **823→823**, private bytes **564887552→564887552**. These short debug samples are not calibrated thresholds, visual content retention, GPU residency or a soak certificate.

Final policy-corrected production probe: `pwsh -NoProfile -File scripts/terminal/observe_panes_windows.ps1 -Executable D:/Code/harbor/target/debug/harbor.exe -OutputDirectory D:/Code/harbor/target/qa-repair/native-policy-final` → **PASS**, `native-probe-final.log` and `native-policy-final/observation.json`, **11:14:50–11:15:16 UTC**. App PID **34712**, child PIDs **11932/13916/25288/36824**, all pwsh **7.6.9** in that run's changed `current` directory; original **13916** remains responsive. Same two default startups, 2000 producer rows, **4→3→2→0** owned-child lifecycle and exit **0**. Quiet sample: **31.25ms CPU**, handles **822→822**, private bytes **564125696→564125696**. Only test assertions/formatting/evidence changed after this production build. Original `evidence/T0005-observations.json` and T0005 Q/N counts remain historical pre-repair evidence; they cannot alone certify the corrective snapshot. No old E-row acceptance is broadened by these local regressions.

## Joint Code Risk / Intent Check

- **Code Risk:** read the complete production/test/doc repair diff and original spec/tickets, then traced unchanged Winit logical conversion/chord ownership, TabManager close/attach and clipboard delivery overrides, generation-keyed allocation/stale observers, Runtime focus/capture/IME reconciliation, terminal resize barriers, raw reader observation/FIFO parser ingestion, Host pending publication and the existing clipboard eligibility/effect path. The delivery-policy risk above was repaired and its affected tests refreshed. No unresolved demonstrated blocker in this corrective scope; no claim of independent delegated-agent review or unseen native/remote behavior.
- **Intent Check:** F1 matches R6 exact native consumed routing/configured semantics; F2 matches R1/R6/R7 survivor input without focus stealing or generation/capture weakening; F3 matches bounded allocation-correct reserved authentication and unchanged unpublished/permission identity; F4 matches R3/R6 pre-publication side-effect exclusion with bounded progress and one authorization path. The unread clarification matches session-level aggregation. No additional feature, policy migration, remote permission or requirement relaxation.
- **Remaining gates:** T0005 keeps **E4/E10/E11 scoped PASS, E2/E3 BLOCKED, E1/E5/E6/E7/E8/E9/E12 NOT RUN** as full scenarios. Physical native keyboard/IME/candidate/mouse/clipboard and independent confirmation races, cross-monitor DPI/drag and simultaneous/hidden presentation, original native content observation, combined sustained native/fallback resource behavior and controlled WSL/direct-SSH identity/cwd/authentication/failure/cleanup remain pending. Local adapter/offscreen/ConPTY/probe evidence supports subcases only. T0004 is reopened In Progress rather than retroactively treating the original Done claim as unqualified current acceptance; T0005 remains Todo. No GitHub closure or full Spec 0056 acceptance.
