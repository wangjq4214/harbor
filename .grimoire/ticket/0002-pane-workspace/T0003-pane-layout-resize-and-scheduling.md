# Lay Out, Resize and Schedule Independent Panes

**Ticket ID:** T0003
**Source:** [Spec 0056](../../spec/0056-pane-workspace.md), R4–R5, presentation portions of R1/R6, E5–E6/E11–E12.
**Status:** Done — T0003 mounted/component scope only; not production pane commands or full Spec 0056 acceptance

## Goal

Render several independent panes in a mixed-direction split tree with draggable ratios and maximize/restore, correct per-pane grids and visible/hidden scheduling. This is independently verifiable with mounted fixtures before user-facing split commands are admitted.

## Affected Surfaces

- Application workspace/split presentation and allocation observations.
- Generic layout/divider/capture mechanisms where required, without Harbor business types in widgets.
- Terminal bridges/external surfaces, renderer projection, Host resize admission, output wake/scheduling and configuration geometry.

## Approach

Replace one active bridge/allocation and common tab-size broadcast with explicit visible pane geometry and retained hidden grids. Keep visibility distinct from focus. Each visible pane has its correct external registration and schedule demand; hidden sessions remain owned and continue bounded output processing.

Preserve ordered asynchronous resize transactions, shared event dispatch, latest-target merging, fair bounded host-turn progress and coherent font/configuration acceptance. Do not implement a separate resize engine or promise that one expensive operation is preemptible.

## Dependencies and Coordination

- **Blocked by:** T0001, because layout leaves and external registrations need stable session/pane bindings.
- **Blocks:** T0004's real focus/capture/close integration.
- **Parallel with:** T0002 after T0001; fixture creation can validate layout independently of remote launch readiness.
- **Coordination risks:** T0004 must consume the same geometry/visibility observations used for rendering, not a second full-window coordinate model.

## Acceptance

- [x] Horizontal, vertical and mixed-direction several-pane layouts keep each leaf bound to its intended independent session.
- [x] Ratio dragging, current-allocation maximize/restore and close/collapse projections preserve session identity and stored split structure/ratios where applicable.
- [x] Pane-local allocation/DPI conversion and documented minimum-size rules produce independent valid grids; zero/invalid/unchanged targets do not generate invalid or duplicate PTY requests.
- [x] All visible panes can update/present without keyboard focus; hidden panes generate no terminal draws or cursor-driven scheduling.
- [x] Panes hidden by maximize or inactive tabs retain their latest valid grid until shown, continue bounded output and converge to their actual allocation on restoration.
- [x] Output, idle, draw and allocation paths share bounded resize admission; latest targets and background progress are retained without requiring a new byte or activation to finish ready work.
- [x] Existing acknowledged byte ordering, stale-epoch rejection, reader release and resize recovery remain intact.
- [x] Font/color/binding reload enumerates applicable live sessions and preserves coherent acceptance/compensation with unequal pane sizes, including hidden sessions; it neither restarts PTYs nor broadcasts one focused pane's grid.
- [x] Mounted layout, scheduling and appropriate offscreen tests cover simultaneous output, clipping, DPI changes, hidden grids, minimize/restore, continuous drag and configuration rejection. Record actual work bounds/limitations without invented numeric performance claims.

## Out of Scope

Host environment recognition/fallback, clipboard authorization redesign and production close/split shortcut admission. This ticket supplies correct geometry/visibility mechanisms, not standalone permission to expose an unsafe partially integrated split feature.

## Delivery Evidence

### Execution and scope

- Base: `814995740e208285f5a7b7efe3e689d01f7b5fa0` (T0002). Explicit **grimoire-loop**, structured inline route: reuse this ticket's Approach → implementation → focused mounted/offscreen/native tests → joint Code Risk + Intent Check against the complete diff/original criteria → authorized repairs/affected checks → format/docs → separately authorized ticket-only staging and one local commit. No plan artifact or delegates. Loaded implement, test, review, check and commit skills, UI/boundary references, loop handoff/recovery and status/commit-format references. Debug skill used for the introduced focus/fixture failures.
- Only this ticket's status/evidence is updated. T0002 remains In Progress for its real WSL/SSH acceptance; no CONTEXT/ADR/spec/README/other-ticket changes, production split/close shortcuts, GitHub mutations or remote tests. Unrelated untracked `.omnidiff.toml` is excluded.
- Application ownership remains in `TabManager`/Host. `crates/harbor-app/src/pane_layout.rs` supplies the mixed tree projection and binding + draw-registration + visibility-generation allocation mailbox. `crates/harbor-widget/src/widgets/split_pane.rs` only supplies generic two-child layout, ratio state and captured divider interaction. No engine/PTY implementation or transaction protocol changed.
- Production `TabCoordinator::apply_pending_allocation` now submits pane-local observations, never the focused panel's grid to all sessions. Its compatibility boolean remains false because submitting intent is not a commit; the once-per-turn admission path reports visible commits. The old explicit uniform request/completion helpers remain for existing embedders/transaction fixtures, not production window allocation or configuration publication.

### Concrete geometry and work bounds

- Horizontal means left/right; vertical means top/bottom. New splits start at 0.5. Finite stored ratios clamp to **0.05..0.95**; invalid setters are ignored (invalid constructor input starts at 0.5). Each generic split reserves a **6 logical-pixel divider**, shrinking it to the available axis if necessary. Each immediate child side has a **24 logical-pixel minimum along that split** when possible; below 48 pixels of remaining space both receive half. This is a local side-allocation rule, not a recursive minimum-window-size guarantee. Compression does not rewrite stored ratios.
- Each leaf uses the same surface-clipped logical→physical conversion as its external draw scissor, with that pane's origin/allocation and current DPI. Existing grid policy subtracts **16 physical pixels of padding on each edge**, divides by accepted physical cell/line metrics and floors, then clamps to **at least 1 row × 2 columns**. A tiny allocation may clip this minimum grid; it does not enlarge its parent or overlap siblings. Nonfinite/nonpositive allocation, scale or metrics, zero surface/intersection, and grid extents above signed-16-bit PTY capacity are rejected before resize admission. Equal intent never generates another native transaction.
- Maximize projects only one existing leaf and preserves the stored tree/controllers; restore remeasures the current viewport/DPI. Close removes only the intended leaf and collapses its redundant ancestor, retaining surviving controllers and engines. These are model/presentation seams, not production command admission or completed focus/confirmation policy.
- Hidden maximize/inactive leaves have no mounted terminal external draw or schedule provider. Their pending ordinary allocation resize is cancelled on hiding; their most recent committed valid grid is retained. Bounded output continues through the existing session-qualified path; restoring publishes fresh generation-qualified geometry. Font/color/binding acceptance still enumerates all live sessions; hidden sessions receive applicable resources without a common visible grid.
- Ordinary resize admission remains **at most 2 attempts per Host turn**, counting failed attempts: one focused-session priority opportunity, then persistent rotating background opportunities (all visible unfocused leaves are eligible). Output/input/draw dispatch cannot admit an extra transaction; ready remainder/deadline progress does not need another byte or activation. Geometry/visibility bookkeeping still visits live sessions; this is not a constant-total-CPU-work claim.
- Existing per-session output channel capacity is **32 events**, reader chunks at most **4096 bytes**, and one ordinary drain consumes at most **16 events** (at most 64 KiB of byte payload, fewer with control events), rearming work under backpressure. These are existing per-session bounds, not a fixed aggregate byte allowance for arbitrarily many producers. No new hidden blink loop, full-history frame scan, timing/latency promise, benchmark or process-leak soak result is claimed. One reflow/native call is not preemptible.

### Criterion mapping

| Original acceptance | Result / evidence |
| --- | --- |
| 1. Horizontal, vertical and mixed independent leaves | PASS: `pane_layout::tests::mounted_mixed_layout_has_independent_bindings_grids_dpi_and_minimize_retention`; three mounted mixed leaves and an eight-leaf fairness fixture; model attachment binds the reserved session/bridge, not the active position. |
| 2. Drag, maximize/restore, collapse preserve identity/structure | PASS: mounted captured continuous drag, maximize/inactive restore and close/collapse tests in `crates/harbor-app/src/pane_layout/tests.rs`; generic both-axis capture/cancel/unmount tests in `crates/harbor-widget/tests/split_pane.rs`. Actual source resources survive, ancestor ratio remains 0.6, and stale close/output cannot affect survivors. |
| 3. Independent valid grids/DPI/minimums/no invalid or duplicate native work | PASS: mounted allocation geometry, 1.5× DPI, zero/minimized/NaN, tiny 1×2 clamp, signed-16-bit rejection and unchanged zero-attempt assertions; generic compression/normalization tests. Existing allocation/draw conversion regressions also pass. |
| 4. Visible output/scheduling independent of focus; hidden no draws/blink demand | PASS: all visible schedule providers counted in mounted fixtures; real WARP readback presents three independent colored terminals, clips corners/divider at 1×/1.5×/2×, and counts draws without focus. Hidden output with enabled hidden cursors produces neither hidden draw/schedule calls nor a visible redraw/deadline loop. |
| 5. Hidden retained grids and actual restoration | PASS: mounted maximize and inactive-tab viewport/DPI restoration; offscreen hidden damage replays on restore; native ConPTY sibling continues output while hidden and converges after restoration without new bytes. |
| 6. Shared fair bounded resize admission/latest intent/quiet completion | PASS: eight-leaf changing-target test, existing multi-session admission/output/input/draw tests and real native quiet-completion test. Native two-pane unequal allocation/restore also asserts each batch <=2 and no pending work on completion. |
| 7. Byte ordering/epochs/release/recovery unchanged | PASS: engine/PTY implementation untouched; default workspace and renderer-free core suites, native asynchronous ACK/group/recovery tests, plus four local split ConPTY regressions. No replacement resize engine. |
| 8. Coherent unequal visible/hidden configuration | PASS: `src/configuration_reload_tests/panes.rs` exercises GPU rejection, unequal native-effect/compensation failure, per-session retry, inactive and maximized hidden grids, missing/stale projection geometry rejection, live renderer/font updates and stable engines; actual native cmd/pwsh reload/output/restore test. Existing live color/binding/history/factory, watcher and native group tests pass after replacing obsolete common-grid assertions. |
| 9. Mounted/offscreen/native evidence and honest bounds | PASS for T0003 component scope: focused fixtures plus broad checks below. Native-window pointer/IME/desktop-DPI observation and sustained lifecycle/resource soak are NOT RUN; production commands/input/exit integration belong to T0004 and full native acceptance/measurements to T0005. |

### Checks and fault detection

All commands run from `D:/Code/harbor`. `T` below expands exactly to `pwsh -NoProfile -File scripts/dev/test_cargo.ps1 --locked --offline`. Generated logs/API output under `target/` are ignored local artifacts, not committed source dependencies.

| Exact command (with T expansion above) | Result |
| --- | --- |
| `T -p harbor-app --lib pane_layout -- --test-threads=1` | PASS: 7 passed, 1 GPU case ignored; refreshed in final workspace run. |
| `T -p harbor-widget --test split_pane` | PASS: 2 passed; also in final workspace run. |
| `T --workspace` | PASS: **2942 passed, 0 failed, 225 ignored**, 78 summaries (`target/t0003-workspace.log`). |
| `T -p harbor-terminal --no-default-features --lib` | PASS: **876 passed, 6 ignored** (`target/t0003-core.log`); subsequent app/widget/Host-only changes do not affect this renderer-free core. |
| `HARBOR_TEST_GPU=dx12-warp T -p harbor-app --lib gpu -- --ignored --test-threads=1` | PASS: **7 passed** (`target/t0003-app-gpu.log`); the final multiline formatting-only edit is additionally refreshed by the focused pane GPU test. |
| `HARBOR_TEST_GPU=dx12-warp T -p harbor-app --lib pane_layout::tests::gpu -- --ignored --test-threads=1` | PASS: **1 passed** after final test formatting (`target/t0003-pane-gpu-final.log`). |
| `HARBOR_TEST_LAUNCH_PROBE=D:/Code/harbor/target/debug/examples/launch_probe.exe HARBOR_TEST_GPU=dx12-warp T -p harbor --bin harbor configuration_reload_tests -- --ignored --test-threads=1` | PASS: **24 passed** (12 GPU configuration/pane cases plus native PTY/profile/launch/group regressions) (`target/t0003-host-resources-final.log`); final source-based audit includes the unequal-size compensation mutation below. |
| `T -p harbor --bin harbor split_launch::tests::pty -- --ignored --test-threads=1` | PASS: **4 passed** (`target/t0003-split-native.log`); no real WSL/SSH inference. |
| `cargo check --locked --offline --workspace --all-targets` | PASS (`target/t0003-check-final.log`). |
| `cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings` | PASS (`target/t0003-clippy-final.log`); unchanged manifest-level unused `parking_lot` warning, no hook/lint bypass. |
| `RUSTDOCFLAGS='-D warnings' cargo doc --locked --offline --workspace --all-features --no-deps` | PASS (`target/t0003-rustdoc.log`); generated API at `D:/Code/harbor/target/doc/harbor/index.html` and crate indexes. |
| `cargo fmt --all -- --check`; `git diff --check`; `python scripts/docs/check_docs.py` | PASS: format/whitespace clean; documentation language/local-link/fragment checks pass for 224 working-tree files (`target/t0003-fmt-final.log`, `target/t0003-docs.log`); refreshed after the final evidence edit. No obsolete checklist script recreated. |

- Fault detection: temporarily replacing per-session recovery intent with the faulted current model grid made the unequal visible/hidden reload test fail at the intended restored-grid assertion: the affected visible session remained 8×24 rather than its prior 9×28 (`target/t0003-mutation-recovery.log`). The mutation was undone; that exact resource test passed on restored source. Setup/compilation failures and zero-test filters are not counted as fault detection.
- Repairs during execution: preserved the existing direct Focus→ExternalSurface delegation instead of moving Focus outside decoration; corrected old fixture assumptions about a global post-reload size; established actual pane geometry before native marker production; retained per-session geometry intent for accepted-generation recovery; protected live engine/registration owners from malformed rejected attachments; repaired strict-Clippy issues and a formatter range truncation caught at compilation. Initial failures were introduced by this work and repaired, not relabeled baseline or environment failures. Final applicable gates supersede those attempts.
- Runtime/version-only probes: Windows **10.0.26300.0**, PowerShell **7.6.9**, bundled x64 ConPTY **1.24.2607.10001**, cargo **1.101.0** (huacnlee fork). Offscreen selection: DX12 WARP / Microsoft Basic Render Driver. Native tests used dedicated local cmd/pwsh children and synthetic markers; no credentials, remote integration, unrelated terminal captures or installation.

### Joint inline Code Risk + Intent Check

- **Code Risk:** separate audit pass over original R1/R4/R5 constraints, the complete tracked/new production/test diff and unchanged Host turn, bridge, external scheduler, allocation/scissor, configuration barrier/compensation and engine resize consumers. No remaining demonstrated correctness/security/compatibility blocker in T0003's inspected scope. Rejected attachments cannot retire a published survivor, stale observer epochs cannot republish old visibility, actual ordinary admission remains singular and bounded, and configuration publication remains an all-session transaction. The deliberate recovery mutation proves the unequal-grid regression assertion is causal. This is an inline review, not a delegated independent-agent verdict or a latency/resource benchmark.
- **Intent Check:** all nine T0003 criteria match the component code and evidence above; no production pane shortcuts, new clipboard permission, PTY/engine resize redesign, context/ADR mutation or other-ticket completion was admitted. Defensible implementation choices are the generic two-child split/controller and the documented local minimum rule. Component completion does not establish E1–E12 or owner visual acceptance.

### T0004 handoff and remaining integration

- Consume `TabManager::active_layout`, `visible_bindings` and `WorkspaceUiController::sync_panes` / `pane_allocations`; each observation carries the exact `PaneBinding`, draw ID and mounted logical rect. Host resize and rendering share the scissor/DPI conversion. Do not reconstruct a second full-window coordinate model or replace captured bindings with current focus. The older outer `latest_terminal_allocation` is panel/chrome information, not a split grid broadcast source.
- Use `TabCoordinator::attach_split(ready, expected_operation, axis)`: it claims T0002's one-shot result and validates/publishes its reserved original source/target/resources/metadata in the same Host turn. Model rejection validates reservation, original live binding, independent engine, draw ID and bridge-engine/registration lifetime. Switching tabs does not retarget; source removal or stale registration rejects and retires only unpublished resources. Cancellation must also discard its reserved target via `cancel_split_reservation`; IDs are never reused.
- Maximize/close are presentation/model APIs only. T0004 must coordinate intended focus (including maximizing a nonfocused leaf), per-pane focus handles, Host input/capture cancellation, local coordinates, search/paste/OSC 52 lifetimes, launch-authentication UI and natural-exit policy before admitting commands. Existing production close-tab meaning/defaults are unchanged here. The model deterministically chooses the first surviving leaf when its focused leaf closes; T0004 owns full eligible-focus integration.
- T0005 still owns real desktop multi-pane drag/DPI/IME/clipboard/exit observations and sustained workload/lifecycle resource measurements. T0002's missing real WSL/SSH environment/cwd/fallback evidence remains explicitly pending under the existing authorization to continue; it is neither passed nor silently waived by this ticket.
- The single local ticket commit is created only after final applicable checks and full cached-diff inspection. Its resulting short hash is returned outside this self-referential record. No push, amend or hook bypass.
