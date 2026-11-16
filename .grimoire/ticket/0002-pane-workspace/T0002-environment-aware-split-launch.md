# Launch a Split in the Source Environment and Directory

**Ticket ID:** T0002
**Source:** [Spec 0056](../../spec/0056-pane-workspace.md), R2–R3, startup lifetime portions of R1/R7, E1–E4/E12 and C5's final correction.
**Status:** In Progress
**Implementation delivery:** Implemented and locally verified; one local T0002 commit authorized by the user's option 2.
**Remote acceptance:** Pending — real WSL/SSH acceptance is BLOCKED / NOT RUN, not passed; this ticket is not Done.

## Goal

Provide the identity-qualified Host launch path needed by split commands: enter the source session's same environment/current directory, or automatically launch the accepted default `[shell]` policy after inherited startup fails.

## Affected Surfaces

- Host/factory and existing default-shell launch pipeline.
- Session-scoped project/context recognition and existing environment-aware launch boundaries.
- Local/WSL/SSH process creation, startup-readiness/failure observation and partial-launch cleanup.

## Approach

Reuse existing opt-in project integration and environment launch validation/quoting where suitable. Capture the intended source environment/directory for the split operation; do not read whichever pane is active at delayed completion. Current directory is not initial directory or an earlier agent's directory.

The split action authorizes the required connection/directory entry. Receiving terminal metadata alone does not. Preserve normal authentication, credentials confidentiality, and the absence of unsolicited integration installation.

Inherited-launch success must mean the intended environment/directory was reached, not just that `ssh.exe` or `wsl.exe` spawned. Detection/signaling mechanisms are implementation details constrained by that observable contract. Keep this independently testable before the production UI command is admitted in T0004.

Current implementation facts to account for: the bash integration receives its environment target from the user rather than discovering/verifying it, and the existing project launch templates provide no Host-observable, launch-attempt-correlated readiness result. Local pwsh can remain alive under `-NoExit` even after a directory-entry error; WSL/SSH launcher creation likewise does not prove connection/cwd success. Deliver explicit observable startup success/failure under R3 rather than treating these existing paths as an already complete contract.

## Dependencies and Coordination

- **Blocked by:** T0001, because source/session/operation identity must survive delay and replacement.
- **Blocks:** T0004's production split command admission.
- **Parallel with:** T0003 after T0001; agree on creation/completion and resource attachment contracts. Shared Host files are merge coordination, not another blocking edge.

## Acceptance

- [ ] Local pwsh, WSL bash and direct-SSH bash fixtures use the actual current environment/directory at the source operation, not a native-local reinterpretation or stale launch cwd.
- [ ] A successful inherited launch creates an independent shell/session without copying the foreground process or automatically relaunching a recognized agent.
- [ ] Unknown/unusable recognition, failed connection, unavailable WSL target and failed directory entry automatically choose the accepted default `[shell]` launch policy.
- [ ] Fallback uses default executable/arguments/directory/environment semantics without source remote paths or stale source-specific launch fields.
- [ ] Connection/cwd startup failure is distinguishable from a successful session's later exit; only startup failure invokes this fallback. Preserve ordinary default-launch recovery/failure behavior rather than recursively relaunching.
- [ ] Failed partial attempts release their child/reader/PTY resources before a replacement is published; a delayed result cannot replace the fallback or attach to an unrelated/closed workspace.
- [ ] Default launch failure remains an honest failure with no falsely live pane, orphan process or damage to survivors.
- [ ] Existing named-profile fail-visible behavior, save-time unknown/correction behavior, normal New tab and configuration validation/reload are unchanged.
- [ ] Tests exercise argument/path boundaries, current versus initial cwd, source switching/closure, partial failure and both-stage failure. Real connection/cwd observations are distinguished from mocked launch assertions; missing hosts/integration are recorded as BLOCKED/NOT RUN.

## Out of Scope

Production split widgets/shortcuts, arbitrary shell-command replay, agent/preset saving changes, runtime environment cloning, credentials persistence and automatic remote installation.

## Delivery Evidence — implemented/local-verified delivery; remote acceptance pending

### Scope and completion gate

- Base revision: `8c4aa60421a07354d9200fbfc10a3d0b98d9dab6` (accepted T0001). Evidence applies to this ticket's production/test diff and this record, delivered in one authorized local T0002 commit; the resulting hash is reported separately. Unrelated untracked `.omnidiff.toml` is untouched; no other ticket/spec/CONTEXT/ADR is changed.
- Explicit **grimoire-loop** for T0002 only: reuse this ticket's implementation approach → implement → focused test design/execution → joint inline Code Risk + Intent Check of the complete tracked diff and all new source/tests → authorized fixes → refreshed affected/integrated verification → report. Loaded implement, test, review and check skills, plus handoff/recovery references. Loaded debug when the native exit test exposed the EOF limitation. No separate plan artifact or delegates; no production split command admission.
- **Implementation delivery accepted for local commit; overall acceptance NOT established.** Controlled WSL/bash and direct-SSH/bash fixtures, their opt-in integration and actual target/cwd observations were not supplied. Real remote success, connection failure, unavailable-target and cwd-failure cases remain **BLOCKED / NOT RUN**, not passing fallback evidence. Installed client versions and mocked declarations do not establish remote execution. Status is `In Progress`, not `Done`; the original acceptance criteria and unchecked boxes are preserved.
- **Resumed checkpoint / user option 2:** the user explicitly authorized committing implemented/local-verified T0002 now and proceeding while actual WSL/SSH acceptance remains pending. This revises workflow sequencing only; it neither waives the original criteria nor turns missing remote evidence into PASS. Ticket-only staging, entire cached-diff review and exactly one local commit are authorized without further confirmation. No push/amend/hook bypass or GitHub mutation; T0003 is not started in this resumption.

### Delivered behavior and API handoff

- `src/split_launch.rs` owns a one-shot `SplitOperation`: capture the full source `PaneBinding` and the current opt-in `project_snapshot` at initiation, remove remembered agent invocation, reserve distinct unpublished pane/session/draw identities, and snapshot the accepted default shell policy. Initial launch cwd, OSC 7 interpreted as a native remote path, later metadata and the then-active workspace are not substitute sources.
- Independent checked operation/attempt identities qualify an internal OSC 9001 readiness/failure marker and separate observer lifetime. Process spawn is only `Pending`. Local pwsh reports after literal filesystem entry with terminating-error handling even under `-NoExit`; remote templates report from the final interactive bash, after its rc file and literal `builtin cd`. SSH retains its second remote-shell quoting boundary; targets still pass existing `ProjectRecipe` validation. This is a startup signal, not a credential/authentication protocol.
- EOF, observed launcher/shell-process exit, explicit failure or the 60-second startup deadline fails the inherited attempt. `Terminal::shell_process_exited` is a separate nonblocking process observation: existing `is_session_closed`/ordinary New tab semantics are unchanged. Native testing showed why EOF alone is insufficient on this ConPTY bundle.
- Failed unpublished `TerminalTabResources` retire bridge/I/O ownership. The operation waits for `PtyEndpoints::shutdown_receipt` before even spawning the default replacement. A receipt becomes complete after native handle/ConPTY teardown; leaked/stuck existing shutdown paths do not authorize a replacement. This is not a process-count/handle-leak soak or a guarantee about failed OS termination/reaper exhaustion.
- Fallback uses the entire default `ShellCommand`, including ordinary executable recovery, args, configured directory and child-environment policy, without merging any source fields. It runs once. Both-stage failure is returned honestly; successful startup makes the operation terminal, so later natural exit never restarts it.
- Pending children use bounded ingestion and at most three coalesced title/cwd/shell-marker observations. Project recognition stays in the engine; pre-publication clipboard effects are discarded, not replayed into an attached foreground session. Inherited display metadata is cleared before fallback.
- Host entry points: `TabCoordinator::{begin_split,poll_split,claim_split}`; factory seam: `TerminalTabFactory::create_split_attempt` returning `AttemptLaunch { resources, cleanup }`, including a cleanup receipt on post-spawn construction failure. `TabManager::reserve_split_ids` burns reservation IDs without publishing a workspace or leaf. `TerminalTabResources::{terminal,bridge}` expose pending I/O/presentation/authentication while the resource owner still controls retirement.
- T0003/T0004 handoff: retain the operation, schedule `next_poll`, route any pending authentication presentation/input through `pending_resources` and the reserved target, and consume `Ready(Box<SplitReady>)` with its expected `OperationId`. `claim_split(*ready, id)` rechecks source membership, shell liveness and operation lifetime. Attach the returned `PreparedSplit` **in the same Host turn**, using its original source/workspace, reserved target/draw, resources, bounded metadata and `LaunchKind`. T0003 owns atomic layout/model attachment and its resource identity validation; T0004 owns production command admission, focus, authentication presentation, degraded-result notification, cancellation and natural-exit close. No attachment is performed by this ticket.

### Criterion-to-evidence assessment

| Acceptance, in original order | Assessment |
| --- | --- |
| 1. Current local/WSL/SSH environment and directory | Local PASS: real pwsh launched in `initial`, existing integration sourced, then changed to a quoted/Unicode `current` directory; new independent child and surviving source both report that cwd. Model fixtures cover all three environment identities, split-time capture and later metadata/focus changes. Actual WSL/SSH distribution/target/cwd observations: **BLOCKED / NOT RUN**. Overall criterion not accepted. |
| 2. Independent shell, no foreground copy/agent relaunch | MATCH in native local/model evidence: distinct engine/session identity, original shell still responds, split builder removes a remembered `codex --resume` invocation and its old cwd. No history replay or runtime environment clone is introduced. Remote interoperability remains unverified, not inferred from this local proof. |
| 3. Unknown/unusable/connection/target/cwd fallback | Local missing-directory race PASS through actual ConPTY and `-NoExit`; unknown/invalid recognition, controlled remote spawn/EOF/cwd failures and timeout PASS in model tests. Real failed SSH connection/unavailable WSL target/remote cwd failure: **BLOCKED / NOT RUN**. Overall remote acceptance pending. |
| 4. Exact default launch semantics | MATCH: equality assertions for complete default command plus real default cwd/environment/argv reports. Existing native launch-recovery/profile/configuration suites pass. No source-specific args, environment or remote directory merge. |
| 5. Startup failure versus later exit; no recursion | MATCH for implemented state machine/local runtime: pending until qualified signal; terminal readiness state; explicit native shell-process exit observation; after real child `exit`, another poll is `Finished` and never invokes the launcher. Remote authentication/startup behavior still needs controlled runtime verification. |
| 6. Partial cleanup and stale attachment protection | MATCH for native/model lifecycle evidence: replacement spawn asserts the first native receipt is complete; retained engine is retired; delayed result/source closure/cancel/drop/wrong-operation tests cannot claim another workspace. Actual layout attachment is deliberately a T0003/T0004 seam, not claimed as delivered UI. |
| 7. Default failure is honest, survivors intact | MATCH: both native launch stages fail, no new published tab/pane, first receipt complete, failure returned once, source remains live. Controlled post-spawn error tests wait for each stage's cleanup receipt. |
| 8. Existing named/default/profile-save/reload semantics | MATCH: default workspace and selected native Host suites pass; saved-project native agent argv test still passes for all four supported executables. Named launch remains strict/fail-visible; ordinary default pipeline and save/correction code are unchanged. |
| 9. Focused tests and truthful runtime distinctions | MATCH: operation/attempt qualification, byte fragmentation, current/initial/agent cwd, quoting, delayed failures, source switches/closure, owner cleanup, both failures, bounded pending metadata/clipboard and later exit are covered. Local Bash surrogate is explicitly not remote transport evidence. Required remote gaps still block full acceptance, not the option-2 implementation-delivery commit. |

### Exact executed checks

Run from `D:/Code/harbor`. In this table, `T` expands exactly to `pwsh -NoProfile -File scripts/dev/test_cargo.ps1 --locked --offline`. Logs are ignored local artifacts under `target/t0002-*.log`, not committed evidence dependencies.

| Command | Actual result / artifact |
| --- | --- |
| `T --workspace` | PASS, exit 0: 77 harness summaries, **2933 passed, 0 failed, 222 ignored**; includes the final source/accessor changes (`target/t0002-workspace.log`). Ignored resource cases are not counted as passes. |
| `T -p harbor --bin harbor split -- --test-threads=1` | PASS: **14 passed, 5 ignored** (`target/t0002-focused.log`): 13 ticket unit/builder tests plus one existing dialog test matched by `split`. Final workspace run also exercises these tests. |
| `T -p harbor-terminal --no-default-features --lib` | PASS: **876 passed, 6 ignored** (`target/t0002-core.log`). Subsequent app-only resource accessor changes do not affect this core-only build. |
| `T -p harbor --bin harbor split_launch::tests::pty -- --ignored --test-threads=1` | PASS: **4 passed** (`target/t0002-native.log`), refreshed after the full-diff review fixes and bridge-accessor assertion. Real Windows/ConPTY current-cwd inheritance, disappeared directory/default policy, both failures, source-close/delayed readiness and natural shell exit. |
| `HARBOR_TEST_BASH='D:/Program Files/Git/usr/bin/bash.exe' T -p harbor --bin harbor split_launch::tests::native -- --ignored --test-threads=1` | PASS: **1 passed**, four local cases (`target/t0002-bash-local.log`). Controlled temporary HOME/rc file, literal quoted/Unicode cwd and missing cwd through WSL-script and SSH-string quoting boundaries. Login-file startup prefix is disabled for this local surrogate; **NOT WSL/SSH connection evidence**. |
| `HARBOR_TEST_LAUNCH_PROBE=D:/Code/harbor/target/debug/examples/launch_probe.exe HARBOR_TEST_GPU=dx12-warp T -p harbor --bin harbor configuration_reload_tests -- --ignored --test-threads=1` | PASS: **22 passed** (`target/t0002-host-resources.log`): WARP configuration and native PTY/profile/default-launch regressions. Used the existing T0001 probe, no installation. |
| `HARBOR_TEST_LAUNCH_PROBE=D:/Code/harbor/target/debug/examples/launch_probe.exe T -p harbor --bin harbor profile_launch::project::tests::pty -- --ignored --test-threads=1` | PASS: **1 passed**, original argv delivery for all four saved-project agent executables (`target/t0002-project-agent.log`). This preserves preset relaunch, not permission to relaunch an agent on split. |
| `T -p harbor-pty pty -- --ignored --test-threads=1` | PASS: **3 passed** (`target/t0002-pty-resources.log`): native endpoints/read/resize/shutdown and ordinary executable recovery. |
| `cargo check --locked --offline --workspace --all-targets` | PASS, exit 0 (`target/t0002-check.log`). |
| `cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings` | PASS, exit 0 (`target/t0002-clippy.log`). The unchanged manifest-level unused `parking_lot` dependency warning remains; no code warning was bypassed. |
| `cargo fmt --all -- --check`; `git diff --check` | PASS (`target/t0002-fmt.log` for fmt), after scoped formatting. |
| `python scripts/docs/check_docs.py` | PASS: 224 working-tree documentation files (`target/t0002-docs.log`), refreshed after this evidence update. Obsolete `scripts/check_docs.py` / `scripts/checklist_summary.py` were not recreated or claimed as executed. |

Fault detection: temporarily removing local `-ErrorAction Stop` reproduced the original false success under `-NoExit`. The real removed-directory regression failed at the expected launch-count assertion (**1 instead of 2**, `target/t0002-mutation-cwd.log`), not at compilation/setup. The mutation was undone; all four native tests pass on the restored code. The initial native `exit` test failed because it waited solely for reader EOF even though the shell had exited; the independent zero-time process-handle observation fixed that causal gap without globally changing ordinary launch/close policy. Initial fixture API-name and strict-Clippy failures were repaired; they are not reported as baseline defects.

### Joint inline Code Risk + Intent Check

- **Code Risk:** complete tracked production/test diff and all four new files inspected, including unchanged source-recognition, project validation/quoting, default-shell, endpoint shutdown, parser-event and resource-owner consumers. No remaining demonstrated correctness/security blocker in the inspected implementation. Resolved the native EOF/liveness gap, bounded pending metadata instead of allowing an unconsumed side-effect queue, and repaired test-only partial-report/BrokenPipe races; removed diagnostic terminal-content capture. Checked operation/attempt lifetime, source membership, cleanup-before-replacement, post-spawn construction errors, default failure, literal argument boundaries and no post-ready restart. Final affected checks above passed. This is the requested inline assessment, not a delegated independent reviewer or final integrated pane-workspace acceptance.
- **Intent Check:** R2/R3 policy, T0001 identity/ownership handoff and T0003/T0004 scope boundary match the delivered code; no split command/widget/shortcut, agent policy, clipboard permission, global shell fallback or architectural-record change was admitted. **Required remote outcomes remain needs-verification and block full T0002 acceptance, not the now-authorized implementation-delivery commit.** Matching code/model assertions and no confirmed code defect do not establish those outcomes.
- Runtime: Windows API version `10.0.26300.0`; PowerShell **7.6.9**; x64 ConPTY DLL file version **1.24.2607.10001**; cargo **1.101.0** (huacnlee fork); local Git Bash **5.2.37**. Version-only probes reported WSL client **3.0.5.0** and OpenSSH **10.2p1** / OpenSSL **3.5.5**; no distribution or SSH session was started by those probes. Remote shell versions/authentication/integration are **unknown**, not verified by client installation.
- Remaining remote acceptance work: provide controlled WSL and direct-SSH targets with normally authorized authentication, pre-existing opt-in bash integration and disposable known initial/current/absent directories; execute actual source-current → new-child environment/cwd observations and controlled connection/target/cwd-failure cleanup/fallback cases through these seams; resolve any observed defects and refresh affected review/checks before claiming acceptance. The user released the implementation-commit gate separately via option 2; these observations are still missing, not passed or waived. No credentials, remote integration installation or permissions were invented to obtain evidence.
- NOT RUN: real horizontal/vertical UI attachment, pending-authentication UI, mixed-pane rendering/input/close/resize, full E1–E12 and repeated process-count/handle-leak soak. Those are later-ticket/integrated acceptance, not inferred from these component/native startup tests. The remaining WSL/SSH launch checks above are a T0002 evidence gap, not silently deferred as a passing T0005 result.

### Resumption verification and delivery decision

- Resumed from the complete actual diff: all 12 tracked T0002 files plus `src/split_launch.rs`, `src/split_launch/tests.rs`, `src/split_launch/tests/native.rs` and `src/split_launch/tests/pty.rs`. Re-read the source ticket/spec and prior Delivery Evidence before reassessment. Loaded loop/checkpoint/handoff/recovery, implement, test, review and check skills; no delegates or implementation replay.
- **Joint Code Risk:** zero additional demonstrated correctness/security/compatibility blockers after inspecting the complete implementation/tests and relevant recognition, validation/quoting, resource-owner, default-launch and native shutdown consumers. No production or test change was required during resumption. Cleanup gates fallback; source/operation qualification and one-shot claim reject stale results; ordinary launch/profile policy remains separate. This is an inline source-based review, not evidence from an independent delegated reviewer.
- **Intent Check:** implementation/local-verification delivery is accepted under option 2 only. The criterion table above retains all nine original outcomes, including needs-verification for real remote identity/cwd and startup/fallback interoperability. Overall T0002/Spec 0056 runtime acceptance is not claimed. Authentication UI, atomic attachment and production command/lifecycle admission remain the explicit T0003/T0004 handoff; no next-ticket implementation was begun.
- Refreshed `T -p harbor --bin harbor split -- --test-threads=1`: **PASS, 14 passed, 0 failed, 5 ignored** (`target/t0002-resume-focused.log`). This includes the builder boundary test, 12 split-operation unit tests and one pre-existing dialog test.
- Refreshed `T -p harbor --bin harbor split_launch::tests::pty -- --ignored --test-threads=1`: **PASS, 4 passed, 0 failed** (`target/t0002-resume-native.log`). Exact native tests: `local_current_not_initial_quoted_directory_independent_child_and_later_natural_exit`, `native_delayed_readiness_after_source_close_is_cancelled_and_fully_retired`, `native_inherited_failure_then_default_failure_is_honest_and_cleans_up`, `removed_directory_under_noexit_falls_back_after_native_cleanup_with_default_policy` in `src/split_launch/tests/pty.rs`.
- Refreshed `HARBOR_TEST_BASH='D:/Program Files/Git/usr/bin/bash.exe' T -p harbor --bin harbor split_launch::tests::native -- --ignored --test-threads=1`: **PASS, 1 passed, 0 failed**, four local cases (`target/t0002-resume-bash-local.log`). Still only a local Bash quoting/cwd surrogate, **not real WSL/SSH acceptance**.
- Reused the prior workspace **2933 passed / 222 ignored**, core-only **876 passed / 6 ignored**, Host WARP/configuration/PTY **22 passed**, saved-project agent **1 passed**, native PTY **3 passed**, workspace check and strict Clippy evidence above. The resumed production/test tree is unchanged from that evidence; the focused wrapper builds reused its existing artifacts without recompilation. Only this ticket's status/evidence changed, so rerunning those unaffected extensive suites was unnecessary. Original fault-detection evidence remains applicable.
- Refreshed `cargo fmt --all -- --check` and `git diff --check`: **PASS** (`target/t0002-resume-fmt.log` for fmt). Refreshed `python scripts/docs/check_docs.py`: **PASS, 224 working-tree files** (`target/t0002-resume-docs.log`); checked again after the final evidence edit. Loaded the commit skill and its `references/format.md`; staging is a separately authorized task, and the user's explicit confirmation waiver applies to the single displayed message.
- Commit-stage outcome and short hash are reported after the single local commit, rather than embedding a self-referential commit hash in this record. Remote gaps remain pending after that commit.
