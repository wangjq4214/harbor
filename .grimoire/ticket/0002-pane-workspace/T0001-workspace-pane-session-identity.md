# Separate Workspace, Pane and Session Identity

**Ticket ID:** T0001
**Source:** [Spec 0056](../../spec/0056-pane-workspace.md), R1, identity portions of R2/R6–R7, E4/E8–E12; [#158](https://github.com/wangjq4214/harbor/issues/158).
**Status:** Done — T0001 identity migration only; not integrated pane-workspace acceptance

## Goal

Establish explicit workspace/pane/session ownership and routing while the existing single-pane tab remains usable. Several real consumers need this migration before split layout or launch can rely on it.

## Affected Surfaces

- Application tab/session model and Host factories/output/exit events.
- Terminal/bridge/draw identity relationships and lifetime.
- Existing command targets, search/read-model provenance, project capture, clipboard sources, title/cwd metadata and configuration transaction enumeration.

## Approach

Move the one-tab/one-session assumption behind explicit workspace, pane and session relationships, preserving existing terminal-engine session identities where applicable. The concrete storage/ID representation remains an implementation detail. A UI mount/visibility change must not own process destruction.

Do not replace all consumer behavior in a global rewrite: migrate actual source/session bindings and validate the existing single-pane operations. Keep product policy in the Host/application and generic widget/runtime contracts independent.

## Dependencies and Coordination

- **Blocked by:** None within this set.
- **Blocks:** T0002's stable launch source/operation binding and T0003's several-pane geometry/registration.
- **Coordination risks:** Existing consumers use `TabId` for several meanings. Inventory actual events and commands rather than merely renaming the type. Coordinate T0002/T0003 exchange contracts before parallel edits.
- Single-pane migration alone does not deliver the new close defaults or natural-exit policy; those are integrated in T0004.

## Acceptance

- [x] Workspace membership, pane presentation identity and session lifetime have an explicit documented relationship; no pane position or draw ID is treated as session identity.
- [x] Existing New tab, activation, whole-tab close and final-window behavior still operate in the migrated single-pane path without restarting surviving sessions.
- [x] Output/exit wakes, draw callbacks, metadata and existing asynchronous consumers remain source-qualified; delayed events for removed/replaced sessions cannot affect a new one.
- [x] Failed creation cannot reuse an identity in a way that accepts a stale event.
- [x] Hiding/unmounting/replacing the UI root retains the intended session; explicit retirement releases its ownership and pending registrations.
- [x] Search/revision checks, OSC 52 source checks, profile capture and configuration transaction enumeration refer to the actual session rather than the active tab. Expose the explicit identity needed for T0004's new original-session binding for user paste; do not assume pending paste already retains that target.
- [x] Focused model/Host tests cover source removal, late events, resource-release ownership and single-pane regression. Record performed/omitted verification honestly.

## Out of Scope

Split UI, environment-aware connection/fallback, new shortcut defaults and production multi-pane command admission. No new global bus, crate hierarchy or universal renderer abstraction.

## Delivery Evidence

### Scope and execution

- Base: `f969039d15a34982f6060c94c029c1b7e9104717`; evidence is for this ticket's combined production/test diff and this record. Only T0001 is completed. Other tickets, the spec, CONTEXT/ADRs and unrelated untracked `.omnidiff.toml` remain unchanged.
- Explicit **grimoire-loop**, structured inline execution: reuse this ticket's approach → implement → focused tests → joint Code Risk + Intent Check over the entire diff/original criteria → affected verification → ticket-only staging/commit. No separate plan artifact or delegates. Loaded implement, test, review and check skills; recovery reference used for failures and the mutation probe.
- `TabId` now means the rail workspace. Workspaces contain stable `PaneId` leaves bound to separately owned `SessionId` records. Bridges deliberately reference each session's engine; draw IDs remain presentation registrations. Host routing IDs are reserved before creation and are distinct from the unchanged engine read-model identity/revision proof.
- Handoff APIs: `active_binding`, `active_session_id`, `binding_for_session`, `contains_binding`, `session_terminal`, `session_working_directory`; delayed operations capture/revalidate the complete `PaneBinding`, never reconstruct IDs from positions/numbers. T0002 still owns launch-attempt identity/readiness/attachment. T0003 still owns split-tree geometry/visibility.
- Real migrations include Host PTY/EOF wakes and hyperlink events, OSC 52 eligibility, search UI/jobs, palette/profile source targets, capture, metadata and configuration membership. User paste's pending original-session retention is still T0004 work; the source ID/lookup APIs are now available. No split commands, new close defaults or natural-exit auto-close were exposed.

### Criterion-to-evidence assessment

| Acceptance (in original order) | Result and evidence |
| --- | --- |
| 1. Explicit workspace/pane/session relationship | PASS: separate workspace leaves/session-owner collection and typed `PaneBinding` in `crates/harbor-app/src/tab_manager.rs`; identity tests use deliberately divergent workspace/pane/session/draw numbers. Engine content identity is preserved. |
| 2. Single-pane New/activate/whole-tab/final-close behavior | PASS: existing tab-manager, mounted workspace and Host FIFO tests; existing native launch/configuration suites. New native retirement test preserves the surviving engine/content version. Final-window evidence is the existing close outcome/Host path, not a new interactive desktop recording. |
| 3. Source-qualified events/consumers and stale rejection | PASS: session-qualified Host events, explicit session lookup for output/metadata/capture, retired bridge callback gate; removed-session events return the default outcome without affecting replacement. Existing search cancellation/revision, OSC 52 and palette stale-source tests pass. |
| 4. Failed creation identity reuse | PASS: reservation burns all four identities before invoking factories; failed-attempt/late-wake and exhaustion tests. Failed resource validation retires retained clones. |
| 5. Presentation versus retirement lifetime | PASS: existing mounted switch/root/unmount tests plus callback-revocation test; explicit session/resource-owner Drop cancels resize, releases I/O and revokes callbacks even with UI/configuration clones. Runtime registrations disappear at normal reconciliation; retained callbacks cannot draw, schedule, IME or input after retirement. Deterministic writer-drop/weak-reference tests and native retained-presentation test cover ownership. |
| 6. Session-qualified search/OSC 52/capture/configuration/paste seam | PASS for T0001: actual consumers migrated, not type aliases; a model-only two-leaf fixture checks both configuration sessions in one workspace and focused-only clipboard eligibility. Existing engine search proofs remain authoritative. Pending user paste target retention is explicitly NOT claimed here. |
| 7. Focused model/Host regression evidence | PASS: five new identity tests, one bridge test and one native Host test; existing related/default/resource suites below. Retirement mutation is detected by the writer-drop assertion. |

### Executed checks

Run from `D:/Code/harbor`, with existing local dependencies only. `target/t0001-*.log` files are local, ignored execution artifacts, not committed evidence dependencies.

| Exact command | Result |
| --- | --- |
| `cargo check --workspace --all-targets --offline` | PASS, exit 0 (`target/t0001-check.log`). |
| `pwsh -NoProfile -File scripts/dev/test_cargo.ps1 --locked --offline -p harbor-app --lib` | PASS: 244 passed, 6 resource tests ignored (`target/t0001-app-tests.log`). |
| `pwsh -NoProfile -File scripts/dev/test_cargo.ps1 --locked --offline --workspace` | PASS, exit 0: 77 harness summaries total 2920 passed, 0 failed, 217 ignored (`target/t0001-workspace-tests.log`). First attempt exceeded the 180-second command limit; refreshed run completed with a 600-second limit. |
| `pwsh -NoProfile -File scripts/dev/test_cargo.ps1 --locked --offline -p harbor-terminal --no-default-features --lib` | PASS: 876 passed, 6 ignored (`target/t0001-core-tests.log`). |
| `HARBOR_TEST_GPU=dx12-warp pwsh -NoProfile -File scripts/dev/test_cargo.ps1 --locked --offline -p harbor-app --lib gpu -- --ignored --test-threads=1` | PASS: 6 passed (`target/t0001-gpu-app.log`). |
| `HARBOR_TEST_GPU=dx12-warp pwsh -NoProfile -File scripts/dev/test_cargo.ps1 --locked --offline -p harbor --bin harbor configuration_reload_tests::pty -- --ignored --test-threads=1` | PASS: 6 passed (`target/t0001-pty-tests.log`); superseded by the post-format combined run below. |
| `cargo build --locked --offline -p harbor-pty --example launch_probe --message-format=json` | PASS. Resolved example: `D:/Code/harbor/target/debug/examples/launch_probe.exe` (`target/t0001-probe.log`). |
| `HARBOR_TEST_LAUNCH_PROBE=D:/Code/harbor/target/debug/examples/launch_probe.exe HARBOR_TEST_GPU=dx12-warp pwsh -NoProfile -File scripts/dev/test_cargo.ps1 --locked --offline -p harbor --bin harbor configuration_reload_tests -- --ignored --test-threads=1` | PASS: 22 passed (GPU configuration, native ConPTY resize/retirement/launch/profile tests; `target/t0001-host-resources-final.log`). Initial combined/profile-only runs failed because the example path was unavailable; no production defect was inferred. Building the existing probe and setting its documented override resolved both cases. |
| `cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings` | PASS, exit 0 (`target/t0001-clippy.log`). Cargo still reports the unchanged harbor-pty manifest's unused `parking_lot` dependency warning. |
| `cargo fmt --all -- --check` and `git diff --check` | PASS after scoped formatting (`target/t0001-fmt.log`). |
| `python scripts/docs/check_docs.py` | PASS: working-tree documentation checks (224 files); refreshed after this evidence edit. |

Fault detection: temporarily omitting `Terminal::retire_session`'s I/O retirement caused `retirement_drops_io_even_with_retained_ui_and_engine_clones` to fail at the intended writer-drop assertion (0 instead of 1). The mutation was undone and the affected app/default/resource suites passed. An initial `--exact` filter selected zero tests and is not counted as evidence. The two app fixture failures introduced by explicit owner Drop were repaired by retaining their Host owner (`_tabs`); all matching live-session fixtures now retain that owner rather than silently testing retired input.

### Joint inline assessment and limitations

- **Code Risk:** no unresolved confirmed blockers after inspecting the complete production/test diff and source consumers. Checked reservation/failed creation, engine/bridge binding, event provenance, owner-versus-UI teardown, poisoned-lock retirement, shared resize/configuration barriers, and clipboard/search gate preservation. No generic-widget or engine GPU dependency was introduced.
- **Intent Check:** all seven T0001 criteria match the scoped evidence above; no unauthorized split/close-default/preset-policy additions. Requirements were checked against spec 0056 and the original ticket, not inferred from passing tests. The assessment was inline, not a separately delegated reviewer; the parent may independently inspect the actual diff/evidence before global acceptance.
- Host: Windows NT `10.0.26300.0`, cargo `1.101.0` (huacnlee fork), PowerShell `7.6.9`; resource runs used the existing staged x64 ConPTY bundle and DX12 WARP (Microsoft Basic Render Driver). No dependency/integration installation, remote connections, GitHub mutations or CONTEXT/ADR changes.
- NOT RUN: real interactive split/IME/WSL/SSH acceptance, multi-pane geometry/output measurements, process-count/handle-leak soak and complete E1–E12 matrix. These are later-ticket/integration work, not implied by model fixtures or the native retirement assertion. No claim that OS process-reaper completion or all manual desktop paths were measured.
- Obsolete `scripts/check_docs.py` / `scripts/checklist_summary.py` commands were not recreated; the current docs checker was used. No push, amend or hook bypass is authorized.
