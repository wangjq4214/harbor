# Integrate Pane Commands, Input Safety and Exit Lifecycle

**Ticket ID:** T0004
**Source:** [Spec 0056](../../spec/0056-pane-workspace.md), R6–R7; command/attachment integration of R1–R5; E1–E12, especially E4/E7–E10.
**Status:** In Progress — reopened by independent QA; corrective component evidence below, full native/remote acceptance pending

## Goal

Make the split workspace usable through actual application commands while guaranteeing correct pane/session input, confirmation lifetimes, close/collapse and natural-exit behavior.

## Affected Surfaces

- Typed command registry, configurable defaults, dispatcher, workspace/tab UI and Host command execution.
- Focus/hit testing/pointer capture, terminal-local coordinates and IME.
- User paste and OSC 52 confirmations, search/results, profile capture and metadata consumers.
- Session exit/output/launch/resize races and full session-resource retirement.

## Approach

Connect the identity-qualified inherited/fallback launch path and pane layout through the existing application command path. Use one authoritative pane/session target per operation and revalidate delayed results. Do not register placeholder or partially safe production split commands.

Keep pointer hit/capture routing distinct from keyboard focus, and keep all visible panes distinct from the single intended keyboard/IME target. Extend existing source cancellation/authorization checks to pane changes without weakening the cross-window paste gate.

Retain the meaning of the existing close-tab command ID and rail close action. Change its default chord to `Ctrl+Shift+W`; provide focused-pane close on `Ctrl+W`. New command spelling and other split/focus/maximize chords remain implementation details under the established configurable registry contract.

## Dependencies and Coordination

- **Blocked by:** T0002 for required production launch/fallback behavior; T0003 for mounted geometry/visibility and split-tree interaction.
- **Blocks:** T0005's complete integrated runtime acceptance.
- **Coordination risks:** Confirmations, search, source metadata, configuration and close events must all use the same live session bindings established in T0001. Launch completion and shell exit must distinguish startup failure from later natural exit.

## Acceptance

- [x] Horizontal/vertical split, pane focus by click/shortcut, ratio dragging and maximize/restore operate through the intended application/widget boundaries and focus the correct resulting session.
- [x] Split results use the operation's intended source/workspace, including delayed inherited failure/fallback; switching or closing cannot attach a late result to another workspace.
- [x] Default `Ctrl+W` closes the focused pane; default `Ctrl+Shift+W` and rail close close the entire tab. Explicit configured close-tab bindings retain whole-tab semantics.
- [x] Overrides, unbinding, duplicate/conflicting binding rejection and consumed/pass-through behavior match existing contracts. Consumed UI chords execute once and write no PTY bytes.
- [x] Keyboard, IME/preedit/candidate positioning, selection/copy/paste, mouse reporting, wheel, scrollbar and pointer capture route to the correct pane/session using its local geometry; divider drag does not leak terminal input.
- [x] Focus loss, unmount, closure and lifecycle cancellation retire capture/transient IME state without leaking late composition or clipboard delivery.
- [x] Paste confirmation preserves the existing independent-window gate and adds explicit original-session lifetime protection: the current completion path receives the then-active terminal, so retaining/revalidating the original target is new work. Pending paste/OSC 52 requests stay source-bound; unfocused visible panes cannot obtain foreground-write permission, and natural exit or other retirement cannot redirect a pending request to a survivor/replacement.
- [x] Search/highlights/navigation, profile capture and source metadata preserve their existing session/provenance semantics without adding cross-pane search or new shell workflows.
- [x] Shell-session exit automatically closes its pane and collapses the layout; ordinary foreground-program completion does not. Explicit close and exit races retire the session only once.
- [x] A surviving pane receives deterministic eligible focus without process/content loss. Last-pane close closes its tab; last-tab close closes the window without replacement. Hiding and switching never terminate sessions.
- [x] Late output/exit/ACK/search/launch results after retirement cannot target survivors, revive registrations or leave child/reader/PTY/bridge resources orphaned.
- [x] Mounted application routing and focused Host/PTY tests cover several simultaneous producers, distinct typed markers, IME/capture cancellation, confirmation races and close/fallback/resize races. Do not substitute unit tests for T0005's real Windows matrix.

## Out of Scope

Cross-window pane movement, persisted workspaces, new clipboard permission modes, global search, arbitrary process replay and unrelated profile-save redesign.

## Delivery Evidence

> Historical T0004 delivery at `2af7639`, not an unqualified certificate for HEAD.
> Independent QA at `7539847` invalidated the native pane-cycle, survivor-focus,
> fixed-grid authentication and startup clipboard-backlog assumptions. The
> [corrective cycle](evidence/T0004-T0005-independent-qa-repair.md) supersedes those
> claims with regression failures, repairs and current-snapshot evidence; original
> requirements and historical command results below are preserved.

### Scope, recovery and execution

- Resumed actual HEAD `1cbf701` (T0003; predecessors `8149957` T0002 and `8c4aa60` T0001), not the interrupted instructions. Inspected all eight tracked partial files and the new `src/pane_lifecycle_tests.rs`; the module was already registered in `tab_coordinator.rs`. Preserved and completed those changes rather than replaying them. Unrelated untracked `.omnidiff.toml` remains untouched.
- Explicit **grimoire-loop**: reuse this ticket's approach → implement/test → joint inline **Code Risk / Intent Check** against original spec/ticket and complete tracked/new diff → authorized repairs and affected checks → formatting → ticket-only staging/full cached-diff review/one local commit. Loaded loop, implement, test, review, check, commit and format reference; recovery/handoff/UI/status references and debug for the mounted capture defect. No plan artifact, delegates, T0005 work, CONTEXT/ADR/spec/GitHub changes, remote connections or installation.
- The checked boxes establish this ticket's production integration and targeted component/native-resource criteria. They do **not** mark E1–E12, T0002's real WSL/SSH acceptance or T0005 Done. The owner's option 2 permits the local implementation commit while those known remote observations remain **BLOCKED / NOT RUN**, not waived or passed.

### Delivered behavior and handoff

- Typed `PaneCommand` operations join the existing source-aware registry/dispatcher/configuration/palette, with `app.split-horizontal` (Ctrl+Shift+D), `app.split-vertical` (Ctrl+Shift+E), `app.next-pane` (Ctrl+Shift+]), `app.previous-pane` (Ctrl+Shift+[), `app.maximize-pane` (Ctrl+Shift+M) and `app.close-pane` (Ctrl+W). The existing `app.close-active-tab` still means whole-tab close, now default Ctrl+Shift+W; rail close is unchanged. Valid explicit overrides preserve command meaning; conflicting ownership still rejects the whole override table.
- Production Host owns one `PendingSplit`: service the captured source's bounded output/metadata, reject if exit reduction changed that source, snapshot T0002's operation/default/initial grid, poll by its deadline, and consume T0003's atomic `attach_split(ready, expected_operation, axis)`. No completion activates/substitutes another workspace. Success focuses the reserved result only in its still-active workspace; failed/cancelled completion discards the reservation and unpublished ownership. Fallback is reported as default shell, not inherited success.
- Pending authentication uses a modal, shortcut-blocking in-window terminal surface bound directly to the **reserved session's bridge**, with operation-targeted Cancel and no clipboard authorization. It keeps the captured grid while unpublished; drawing it cannot admit extra native resizes. It is removed on ready/failure/cancel/cleanup. Watched configuration is deferred until startup completes, then uses the normal coherent transaction; no unaccepted generation is mixed into an unpublished attempt.
- Each live pane has a stable focus handle. Runtime focus notifications enqueue the complete original binding; Host revalidates visibility/workspace and does not cancel the press just captured by a clicked terminal. Focus/next/previous/maximize/close update search source, title/cwd/capture/palette consumers through the existing session-qualified model. Focus-handle records are pruned on retirement.
- Pointer/selection/mouse/wheel/scrollbar coordinates use the pane's committed allocation and accepted metrics/DPI, including the first hit after layout changes before another draw or resize admission. This GPU-independent input-geometry publication does not resize/drain or bypass the at-most-two-attempt Host batch. Config acceptance/rejection republishes the actual accepted metrics.
- Runtime distinguishes a focus wrapper from its effective delegated input target, and retires old-focus capture before installing a new press. Unmount and focus loss cancel terminal pointer/preedit state; lifecycle cancellation bypasses the paste input gate. Late IME commit remains tied to its original runtime target.
- User paste retains a weak original `PaneBinding` + engine identity, revalidates liveness and current binding, and reads approval-time input modes. Natural exit/focus/closure revokes the independent-window gate without redirecting bytes. OSC 52 retains its existing single policy path and original session; visible-but-unfocused panes, exited sources and unpublished authentication cannot inherit foreground permission.
- The Windows exit observer waits on a duplicated handle for the **exact session shell**, not PTY EOF or OSC 133/foreground completion. The source-qualified Host reducer closes/collapses once, retires resize/I/O/bridge ownership, selects the deterministic first surviving leaf, and exits on the final tab without replacement. A queued save/capture command also revalidates its original binding after output/exit service rather than capturing a survivor.

### Criterion mapping (original acceptance order)

| # | Result / evidence |
| --- | --- |
| 1 | MATCH: production registry/Host branch, mounted mixed-pane click/shortcut/maximize/close tests, existing captured-divider/ratio tests, WARP mixed layout and native mixed-tree attachment. New result focus uses its reserved binding. |
| 2 | MATCH for integration: T0002 operation-qualified readiness/cleanup/fallback and T0003 atomic attachment are consumed in production; source captured before metadata/exit service; existing stale-source/switch/registration/closure tests and four native split-launch cases pass. Actual WSL/SSH observations remain pending in T0002/T0005. |
| 3 | MATCH: default Ctrl+W pane-close, Ctrl+Shift+W whole-tab and unchanged rail intent; mounted valid explicit close-tab override; native/model survivor and final-window outcomes. |
| 4 | MATCH: new registry conflict/duplicate/override/unbind matrix; mounted consumed chords enqueue once with no keyboard-byte leakage in capture-writer fixtures. Existing native-adapter repeat/release cancellation and conditional copy/alternate-scroll pass-through tests pass. |
| 5 | MATCH: three-pane distinct ALPHA/BETA/GAMMA input at 1×/1.5×; exact SGR local (3,2) reports, cross-pane capture, first hit after pre-draw drag, selection/copy, wheel/scrollbar and divider non-leakage. WARP candidate positions checked independently at 1×/1.5×/2×. Three actual ConPTY children receive separate marker commands while producing output and resizing. |
| 6 | MATCH: mounted IME/preedit/capture focus, gate and unmount cancellation; stale commit never reaches sibling/replacement; generic widget/default regression suites pass. |
| 7 | MATCH: original binding/weak-engine confirmation tests (including same-binding wrong engine), one-shot cancellation, approval-time modes, write failure and native shell-exit paste race; native OSC 52 source-exit/foreground-survivor race. Independent-window policy is retained, not replaced. |
| 8 | MATCH: existing session-qualified search/revision/palette/profile-save/metadata suites pass; focus publishes active session; save/capture rechecks original source after exit service. No cross-pane search or agent/preset policy changes. |
| 9 | MATCH: registered and executed native exact-shell exit, foreground subprocess and OSC 133 non-exit tests, explicit-close/late-exit idempotence and pending-resize retirement. |
| 10 | MATCH: mounted/native deterministic survivor engine identity/content lifetime, final-pane natural exit, whole-tab/final-window outcomes and existing hidden/maximize/tab retention suites. Native desktop window destruction remains T0005 observation, not a claimed recording here. |
| 11 | MATCH: source-qualified late output/exit, retired bridge callbacks, search provenance, launch reservations/cleanup and resize/ACK regressions; native shutdown receipts complete even with retained presentation/engine clones. No process-count/handle-leak soak is inferred. |
| 12 | MATCH: six new mounted routing tests + registry matrix, extended WARP mixed-pane IME test, three native lifecycle cases, native paste race and refreshed launch/configuration/resize resource suites. These are not substitutes for T0005's full desktop/environment/resource matrix. |

### Executed checks

All commands ran from `D:/Code/harbor`. `T` expands exactly to `pwsh -NoProfile -File scripts/dev/test_cargo.ps1 --locked --offline`. Logs under `target/t0004-*.log` are ignored local execution artifacts, not required committed files.

| Exact command | Actual result / log |
| --- | --- |
| `T --workspace` | PASS: **2950 passed, 0 failed, 229 ignored**, 78 harness summaries (`target/t0004-workspace.log`). After the final Host-only source-service/metrics repairs, affected default/resource suites below were refreshed; unchanged app/widget/core evidence remains applicable. |
| `T -p harbor --bin harbor` | PASS: **150 passed, 0 failed, 36 ignored** on final Host source (`target/t0004-host-default-final.log`). |
| `T -p harbor-terminal --no-default-features --lib` | PASS: **876 passed, 0 failed, 6 ignored** (`target/t0004-core.log`). |
| `T -p harbor-app --lib pane_ -- --test-threads=1` | PASS: **14 passed, 0 failed, 1 ignored** during iteration; final workspace includes all seven new default tests (`target/t0004-pane-app.log`). |
| `T -p harbor --bin harbor pane_lifecycle_tests -- --ignored --test-threads=1` | PASS: **3 passed**, including independent final producer markers in all three actual ConPTY engines, bounded resize convergence, survivor identity and native retirement (`target/t0004-lifecycle-final.log`). |
| `T -p harbor --bin harbor paste_command_tests::pty -- --ignored --test-threads=1` | PASS: **1 passed**, actual shell exit before/after model retirement (`target/t0004-paste-native.log`). |
| `T -p harbor --bin harbor split_launch::tests::pty -- --ignored --test-threads=1` | PASS: **4 passed**, local pwsh current-cwd/readiness, missing directory/default fallback, both-stage failure and source-close cleanup (`target/t0004-split-native.log`). No remote-success inference. |
| `HARBOR_TEST_GPU=dx12-warp T -p harbor-app --lib gpu -- --ignored --test-threads=1` | PASS: **7 passed**, includes mixed-pane rendering/IME/DPI/cancellation (`target/t0004-app-gpu.log`). |
| `HARBOR_TEST_LAUNCH_PROBE=D:/Code/harbor/target/debug/examples/launch_probe.exe HARBOR_TEST_GPU=dx12-warp T -p harbor --bin harbor configuration_reload_tests -- --ignored --test-threads=1` | PASS: **24 passed**, refreshed after final Host metrics/source repairs (`target/t0004-host-resources-final.log`). |
| `cargo check --locked --offline --workspace --all-targets` | PASS (`target/t0004-check.log`); an intermediate now-unused Host accessor warning was removed. Final strict Clippy below verifies the resulting tree. |
| `cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings` | PASS (`target/t0004-clippy-final.log`); only the unchanged manifest-level unused `parking_lot` warning remains. No lint/hook bypass. |
| `RUSTDOCFLAGS='-D warnings' cargo doc --locked --offline --workspace --all-features --no-deps` | PASS (`target/t0004-rustdoc.log`). |
| `cargo fmt --all -- --check`; `git diff --check` | PASS (`target/t0004-fmt-final.log` for fmt); scoped formatting applied through anchored edits. |
| `python scripts/docs/check_docs.py` | PASS: language/local-link/fragment checks for **224 working-tree files** (`target/t0004-docs.log`); refreshed after the final evidence edit. Obsolete checklist/docs paths were not recreated or claimed as executed. |

Fault detection and repairs: before the Runtime focus repair, both mounted capture tests failed at the intended new-press capture assertion, while marker and close tests passed. The repaired effective-target/capture ordering makes those exact cases pass. Compatibility fixtures initially expected Ctrl+W whole-tab/12 close keycaps; they now test Ctrl+Shift+W and its extra keycap, without weakening whole-tab meaning. Initial new-test/API compilation failures and strict-Clippy failures were repaired; they are not relabeled baseline or environmental failures. Review additionally repaired gated cancellation, pre-draw local geometry, unpublished draw resize admission, focus-handle retirement, configuration-metric publication and source revalidation after output/exit reduction.

### Joint inline assessment and limits

- **Code Risk:** separate source-based pass over complete production/test diff (including all new files), original requirements and unchanged command/palette, widget focus/capture/IME, native endpoint ownership, T0002 readiness/fallback, T0003 attachment/allocation/resize, search/profile and clipboard consumers. Zero unresolved demonstrated correctness/security/compatibility blockers in the T0004 scope after the repairs and affected checks above. No active-workspace substitution, second clipboard permission path, generic-widget business IDs or resize-engine redesign. This is an inline audit, not a delegated independent-agent verdict.
- **Intent Check:** all 12 original T0004 component/integration criteria map above; no unauthorized extras or T0005 work. Production commands are now admitted only with source-qualified launch/readiness/attachment and safe pending authentication. Existing named/default launch and saved-agent policies remain separate.
- Environment refreshed: Windows **10.0.26300.0**, PowerShell **7.6.9**, cargo **1.102.0** (huacnlee/cargo 0.103.0-1). Tests use the existing bundled x64 ConPTY **1.24.2607.10001** via the wrapper, dedicated local cmd (**10.0.26100.9949**) / pwsh children, synthetic markers/disposable temporary paths and DX12 WARP. No credentials or unrelated terminal contents are captured.
- **NOT RUN:** actual native-desktop multi-pane pointer/IME/candidate/clipboard/window-exit observations, full E1–E12 runtime acceptance, sustained latency/idle/resource measurements and repeated process/handle-leak soak. **BLOCKED / NOT RUN:** real WSL/direct-SSH environment/cwd/authentication/fallback observations without controlled pre-existing integrated targets. Client installation, local Bash surrogates and local fallback do not establish them.
- **T0005 handoff only:** execute its original matrix against the resulting integrated revision; include actual local/WSL/SSH child environment/cwd, pending authentication and source closure, DPI/drag/input/clipboard races, final window and repeated lifecycle/resource observations. Retain T0002's pending remote evidence. Do not close #158 or mutate GitHub without separate authority. T0005 remains untouched/Todo.
- The one ticket-only local commit is created only after documentation checks, full cached-diff review and index isolation. Its actual hash is returned after success, not embedded self-referentially. No push, amend or hook bypass.

## Independent-QA corrective refresh over 7539847

- All four findings/concerns are confirmed and repaired at their production boundaries; see the [causal reproducers, changed API handoff, combined checks and joint review](evidence/T0004-T0005-independent-qa-repair.md). Native defaults now register shifted logical braces; Runtime restores surviving stable focus without discarding allocation generations or stealing rail/editor focus; reserved authentication follows mounted allocation/DPI through bounded Host resize; publication waits for bounded parser progress through raw readiness and discards older external startup effects without changing normal clipboard authorization/delivery.
- Original `Done` is reopened to **In Progress** because independently demonstrated regressions invalidated the earlier evidence, not merely because remote prerequisites are unavailable. Corrective local component evidence is not full native/remote acceptance. The original checked criterion mapping is historical; it must be read with this refresh, especially criteria 1/4/5/6/7/10/12.
- Current combined gates: workspace **2953 passed / 232 ignored**, core **876 / 6 ignored**, mounted pane **16 / 2 GPU ignored**, app WARP **8**, Host resources **24**, native lifecycle/auth **5**, native split **4**, native paste **1**, PTY **3**, strict check/Clippy/fmt/rustdoc/docs and compiled guide examples. Exact commands, failure/restoration evidence, snapshot applicability and native probe details are in the linked record.
- T0005's E2/E3 remain BLOCKED and seven full native scenarios remain NOT RUN. No OS clipboard, remote connection, installation, GitHub change, history rewrite or unrelated file staging is authorized by these repairs. One separate corrective local commit is authorized after the full cached diff is reviewed; its actual hash is returned after success.
