# U5 — Native Host Roots, Deadlines, and GPU Outcomes

**Spec ID:** 0042
**Status:** Accepted (owner-confirmed)
**Date:** 2026-10-26
**Sources:** [UI_API_REVIEW.md](../../UI_API_REVIEW.md) U5/F9/F13/F15 and preserved boundaries in §§5–6; the 2026-10-26 refinement conversation approving IntoChildView root factories, non-rendering Host deadlines, persistent shared-GPU fatal reporting with application-owned exit, no automatic device reconstruction.
**Decision record:** [ADR 0088](../adr/0088-native-host-root-deadline-and-shared-gpu-fault-contracts.md).

## Requirements

### R1 — Both native root factories accept the established Component/View protocol (F9)

- WinitWindowHostBuilder::build and build_with_output accept root results implementing IntoChildView + 'static. Ordinary Components, macro-produced Views and existing Views all work through public entry points without an extra wrapper Component.
- Keep the factory one-shot and fallible. Preserve HostInitContext and platform setup availability, startup-stage error attribution, panic handling, and the bootstrap-output contract of build_with_output. Do not publish a successful Host/output pair when construction fails.
- Preserve the existing construction/mount sequence: resource/platform setup before the factory, root conversion through the existing protocol, mounting/building with the Runtime's own Fiber/BuildCx, and the established initial update/focus/redraw behavior. Do not eagerly build Components in the factory adapter or rebuild an existing View to satisfy the old bound.
- Do not implement Component for View as a compatibility workaround or redesign root identity, HMR factories, or Component::build -> View.

Source: F9; ADR 0088 D1; [ADR 0079](../adr/0079-defer-root-construction-and-unify-content-slot-boundaries.md); [spec 0034](./0034-deferred-view-root-and-content-slot-boundary.md).

### R2 — host_deadline retains non-rendering wake semantics (F15)

- A future host_deadline is a Host wake demand even when the window is non-drawable/zero-sized. With no competing eligible demand, the result is WaitUntil(host_deadline); without a future Host demand, an otherwise idle non-drawable window waits.
- Non-drawable windows do not acquire a drawable surface, submit rendering work, run render-animation Poll, or repeatedly request recovery redraws merely to satisfy the deadline. Device-error notification progress under R3 is not rendering animation and must not be disabled by this rule.
- The caller owns due business work and supplies an updated or absent deadline on subsequent turns. The Host does not run business callbacks, return a past WaitUntil, or create a redraw/busy loop because an expired Host deadline is repeatedly supplied.
- On drawable windows, preserve Runtime/recovery/Host deadline arbitration and redraw coalescing. On restoration, resume the established rendering behavior rather than losing pending work or duplicating frame attempts. Host deadlines must not overwrite Runtime deadlines.
- Preserve application-wide earliest-demand arbitration across main/confirmation windows and business deadlines. The existing application's separate notification timer arbitration is not removed merely to exercise the parameter.

Source: F15 and the user's selected non-rendering-deadline interpretation; ADR 0088 D2; [ADR 0031](../adr/0031-widget-winit-adapter-owns-native-host-infrastructure.md).

### R3 — Real GPU notifications reach persistent shared state and wake an idle Host (F13)

- SharedGpu owns device-level fault state. Connect actual Device device-lost and uncaptured-error notification facilities as part of shared-device initialization, covering subsequent GPU use by Host initialization, widget rendering and borrowed external rendering; do not stop at test-only FrameError injection.
- A fatal condition remains observable for every Host sharing the Device. One window's observation/dismissal cannot drain or reset it for another window; closing a secondary window does not restore device health.
- GPU callbacks record diagnostics/fault state and signal UI-thread work through a native-host wake seam. They do not operate on Runtime/Window or decide process exit from a callback thread. The signal must work without a subsequent redraw, visible window, active animation or external business event, including when all sharing windows are non-drawable.
- Explain and verify how backend notifications are progressed and delivered in the actual wgpu/native integration. Registering a callback is not sufficient evidence if delivery still requires unrelated rendering activity. Do not introduce render polling as a substitute for a working error wake path.
- Error callbacks, wake registrations and state must have safe lifetimes through secondary-window closure, normal shutdown, failed construction and optional HMR. A closed wake transport must not panic or keep a dead Host/generation alive. Preserve the error for any still-live sharing Host.

Source: F13; approved SharedGpu ownership and idle/minimized wake recommendation; ADR 0088 D3/D7; ADR 0031.

### R4 — Explicit fatal reporting covers frame and non-frame boundaries (F13)

- OutOfMemory, unexpected DeviceLost and uncaptured Validation/Internal errors are fatal and retain enough category/detail for application diagnosis. Expected normal Device destruction is not reported as an unexpected fault. Public error-type/variant spelling is not fixed by this spec.
- Complete reporting across acquisition, encoding, submission and requested presentation, including synchronous notifications and later asynchronous ones. An already-observable shared fatal cannot be hidden behind a Skipped/retry/wait-only outcome merely because no drawable frame can be attempted.
- Expose an explicit fatal observation/report at native Host frame and non-frame/UI-wake boundaries. Do not require the application to obtain a raw Runtime, fabricate a redraw, or poll raw wgpu handles to discover a fatal condition.
- Preserve asynchronous truth: successful submission/requested presentation is not GPU completion. A fault reported later is delivered at an appropriate subsequent Host boundary; do not promise synchronous capture of every error or retroactively claim a previously requested frame never submitted.
- Recoverable surface lost/outdated/suboptimal handling remains local to its surface and retains its existing retry policy. It is not a shared Device fatal by default. Conversely, a device fatal is not cleared by reconfiguring one surface. No automatic Device/Queue reconstruction or resource replay is included.

Source: F13 and approved fatal/no-reconstruction policy; ADR 0088 D4/D5; ADR 0031.

### R5 — Application consumers apply the shared-fatal exit policy consistently (F13)

- Harbor diagnoses a shared-device fatal report and exits through its application-owned EventLoop policy. harbor-widget reports the outcome and does not exit the process or take over the EventLoop.
- Update main-window, paste-confirmation and OSC52 confirmation consumers, including frame-independent wake/idle handling. A shared-device fatal from an OSC52 confirmation cannot be downgraded to closing that dialog and continuing with the same failed Device.
- Preserve paste/clipboard approval, cancellation, input gating and generation safety. A fatal/cancelled confirmation must not execute a pending PTY write or approved clipboard action as if the user had confirmed it.
- Sharing the Device does not merge window Runtime/input/surface state. Ordinary per-surface recovery and established non-device dialog behavior are not globally reclassified solely to unify handlers.

Source: approved application-owned exit policy and shared-device multi-window recommendation; ADR 0088 D6; ADR 0031; the observed OSC52 Fatal handling seam in src/shell.rs.

### R6 — Preserve established ownership, migration and lifecycle contracts

- Host retains Window/Surface/Runtime/native resource ownership; the application retains EventLoop, business state, multi-window and exit policy. GPU notification transport is generic host infrastructure, not a new business message bus or automatic Store/Dispatcher wake contract.
- Preserve factory errors/bootstrap output, native resource destruction order, shared Device/Queue identity during ordinary secondary-window lifecycle, HMR generation gating/destruction barriers, and borrowed external GPU access.
- New GPU/wake registrations must not retain callbacks/captures from an unloaded HMR generation. Root-factory widening must not move build/hook ownership or change existing HMR teardown ordering. Existing native focus/IME/HMR obligations are not discharged by this spec's static or injected tests.
- Migrate affected public callers, native examples/docs, compilation contracts, tests and application consumers together. Preserve U1–U4 child, event, private-boundary and naming/validation contracts; do not repeat their delivered implementation or expand into U6 renderer-order redesign.

Source: review §§4–6; ADR 0088 D1/D7; ADR 0031 and ADR 0079; [spec 0012](./0012-widget-winit-native-host-consolidation.md).

## Solution and Integration Seams

Continue the existing native Host design. Broaden only the root-description bounds, separate Host wake demand from render eligibility, and complete the SharedGpu notification → persistent state → UI wake → explicit Host report → application policy path. Exact fault-state storage, error/wake type names, registration APIs and backend-progress mechanism are implementation choices within R1–R6, not new architectural requirements.

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Root construction | application Component/View factory → Host builder → Runtime | one-shot fallible factory, setup resources, existing IntoChildView semantics | both build forms accept the same roots; errors/output/mount timing preserved |
| Deadline scheduling | application deadline + drawable state → presenter/scheduler → cross-window wait arbitration | future Host wake is not contingent on rendering | wake for business work without non-drawable render polling or acquisition |
| GPU notification | actual wgpu Device callbacks/progress → SharedGpu fault state | callback-thread safety, early registration, persistent shared visibility | diagnosable fatal state and UI wake independent of redraw |
| Host reporting | shared state + frame execution + UI wake/idle turn → Host outcomes | asynchronous errors, explicit non-frame observation, local surface recovery | fatal report without requiring a drawable frame; no false GPU-completion guarantee |
| Application policy | main/paste/OSC52 Host consumers → application EventLoop | distinguish shared-device fatal from window-local outcomes | consistent diagnosis/exit and safe cancellation, not dialog-only dismissal |
| Lifetime | Host/SharedGpu/wake transport/HMR generations → destruction | no stale native/generation ownership or destructive shared-error drain | safe closure/shutdown and preserved old-generation release obligations |

Relevant baseline (`a6da35c`, source inspection, not a U5 execution result): build bounds in `crates/harbor-widget/src/winit/host.rs`; missing device notifications in `winit/gpu.rs`; frame execution/reporting in `winit/presenter.rs`; non-drawable wait branch in `winit/scheduler.rs`; consumers in `src/{shell,dialog,event}.rs`. Existing evidence entry points include `crates/harbor-widget/tests/{winit_contracts,winit_qa,native_host_smoke}.rs`, native-host unit tests and application confirmation-policy tests. These are locations to extend, not proof of the new requirements.

## End-to-End Acceptance

### E1 — Public root declarations construct the same native Host (R1, R6)
- **Given:** an ordinary Component, a macro-produced View and an existing View, plus both builder forms and fallible factories.
- **When:** public compilation contracts are checked and roots are installed through native Host construction.
- **Then:** all root forms work without wrapper Components; factories run once at the established setup boundary; build_with_output returns the intended application output only with a successfully constructed Host. Factory errors/panics retain their startup attribution; mounting/build/hooks remain Runtime-owned. Cover bootstrap and shared-GPU reuse and preserve static/HMR call compatibility.

### E2 — A non-drawable window wakes for business work without rendering (R2, R6)
- **Given:** a zero-size/non-drawable Host, with no deadline, a future deadline, an expired deadline, and pending render/recovery work in separate cases.
- **When:** the event loop computes its wait, the future deadline arrives, the caller handles/reschedules the work, and the window becomes drawable again.
- **Then:** future Host work still wakes the UI thread; no-deadline/expired-deadline cases do not spin; suspension causes no surface acquisition, rendering submission or animation Poll; restored rendering is coalesced and Runtime/Host deadlines are not overwritten. Include main/secondary cross-window earliest-wait arbitration.

### E3 — An idle shared-device fault wakes and reaches application policy (R3–R6)
- **Given:** main and secondary Hosts sharing a Device, including all-windows-non-drawable and no-pending-redraw states.
- **When:** the production notification handler receives a fatal notification and signals host work.
- **Then:** the UI thread wakes and observes an explicit fatal without forcing a frame; observing/closing the secondary Host first does not erase the main Host's fault. Harbor diagnoses and exits rather than merely dismissing an OSC52 dialog. No confirmation action is committed through the failure path.

### E4 — Frame errors and asynchronous notification timing are truthful (R3–R5)
- **Given:** acquisition, encode, submit/requested-present paths and a later asynchronous fatal notification.
- **When:** faults occur or become observable at those boundaries.
- **Then:** the actual production path reports fatal conditions rather than only classifying injected FrameError values. Already-reported shared fatal is not hidden as a skipped/recovery-only frame; later notification is delivered on a subsequent boundary without claiming synchronous execution completion. A local surface-recovery case remains recoverable and does not poison an otherwise healthy shared Device.

### E5 — Closure, startup failure and generation changes retain safe ownership (R1, R3, R6)
- **Given:** repeated secondary-window creation/closure, normal shutdown, failed Host construction, a closed wake transport and optional HMR root replacement.
- **When:** registrations are created/released, a normal destruction notification occurs, or notification delivery races with teardown.
- **Then:** normal destruction is not a false fatal, callbacks do not manipulate dead UI state, no closed-transport panic/deadlock or ownership cycle keeps resources/generations alive, and another live Host can still observe its Device's fatal state. Preserve native destruction order and the existing release-before-DLL-unload contract; headless tests cannot prove real DLL unload order.

## Decisions and Evidence Boundaries

| Choice | Source |
| --- | --- |
| IntoChildView for both factories, preserving factory/output/mount contracts | F9; approved refinement; ADR 0088 D1 |
| host_deadline is also a non-rendering wake demand | user selected this over rendering-only timing; ADR 0088 D2 |
| persistent per-SharedGpu fault, thread-safe record/wake, non-frame reporting | F13; approved refinement; ADR 0088 D3/D4 |
| OOM/unexpected device loss/uncaptured validation/internal are fatal; expected destruction is not | approved refinement; ADR 0088 D5 |
| Harbor exits on shared-device fatal, widget never owns exit; no automatic device rebuild | approved refinement; ADR 0088 D5/D6; ADR 0031 |
| production wiring, injection, multi-window and real device evidence are separate | review §6; approved refinement; ADR 0088 D7 |

No blocking product or architecture choice remains. Low-level API/helper spellings and notification-progress mechanics are not assumed facts; implementations must establish that their selected mechanism meets R3 rather than invent a different wake/error contract.

## Verification and Completion Boundaries

- Cover both public root factory forms and root kinds, including factory failure/panic, bootstrap output and mounting timing. Check affected in-repo callers, native examples and compilation fixtures.
- Use deterministic scheduler/adapter tests for future/absent/expired Host deadlines, suspended rendering/recovery, redraw coalescing, drawable restoration and cross-window arbitration. Add native event-loop evidence that WaitUntil actually wakes while rendering is suspended.
- Cover all fatal categories, normal destruction, callback-to-state-to-wake-to-report delivery, shared observation, frame-independent delivery, closed transport, initialization and teardown races. Policy-only FrameError injection does not establish production notification wiring. An intentionally generated real wgpu validation error can test that production seam, but is not proof of real OOM/device loss.
- Exercise main/paste/OSC52 fatal consumers and verify that pending clipboard/PTY actions are not accepted through a fatal path. Check local surface recovery separately from shared-device failure.
- Deterministic/injected results, actual production callback-path results, native multi-window/minimized behavior and real device-failure results are separate evidence classes. Hardware/driver faults may not be safely reproducible: injected results do not prove real OOM/device loss, and GPU exhaustion or driver reset requires separate authorization.
- Real HMR generation-release evidence remains distinct from interaction smoke, static lifetime analysis and headless tests; successful reload alone does not prove release-before-unload.

## Out of Scope

- Automatic Device/Queue recreation, terminal GPU-resource replay, or a new device-recovery framework.
- Moving EventLoop, business timers, multi-window or fatal/exit ownership into harbor-widget; a new business event bus or automatic Store wakeup.
- Changing root identity/HMR ABI, preserving Fiber state across arbitrary root/type changes, or weakening release-before-unload/native destruction barriers.
- Redoing U1–U4, U6 renderer ordering, or unrelated widget styling/layout migrations.

## Retired Execution Evidence

Stable retired execution aliases retain these contracts and historical evidence boundaries:

### T0001 — Root-factory parity

`0020-native-host-contracts/T0001` = R1/R6, E1/E5. [Root compilation contracts](../../crates/harbor-widget/tests/winit_contracts.rs) and [Native Host smoke](../../crates/harbor-widget/tests/native_host_smoke.rs) retain regression coverage. Historical Windows/DX12 native evidence used synthetic routed IME; it did not establish physical typing/IME, fractional-DPI interaction, native Vulkan presentation or real DLL release/unload order. Deadline/GPU delivery was outside that slice.

### T0002 — Non-rendering Host deadlines

`0020-native-host-contracts/T0002` = R2/R6, E2. [Scheduler regressions](../../crates/harbor-widget/src/winit/scheduler.rs), [adapter QA](../../crates/harbor-widget/tests/winit_qa.rs) and [Native deadline smoke](../../crates/harbor-widget/tests/native_host_deadlines.rs) retain regression coverage. Historical native evidence established winit timer delivery for hidden shared-Device Hosts with injected zero-size geometry and restored presentation, not physical minimize/restore or GPU completion. That slice did not claim full workspace, native Vulkan, real GPU-fault or DLL-release execution.

### T0003 — Shared GPU fatal delivery

`0020-native-host-contracts/T0003` = R3–R6, E3–E5. [Native GPU-fault regressions](../../crates/harbor-widget/tests/native_gpu_faults.rs) retain regression coverage. Historical evidence (recoverable from commit `86ed01aba5edc873fc2733927cb92d22135b29f3`) distinguishes deterministic categories/policy, actual wgpu validation, independent progress and native two-Host wake/observation on DX12/Vulkan from unavailable hardware-failure and DLL traces.

### Shared limitations and remaining owners

Historical broad strict lint failed on unchanged `clippy::chunks_exact_to_as_chunks` findings in `crates/harbor-text/src/backend/dwrite/sequence/tests.rs:172,175` and `crates/harbor-widget/tests/quad_renderer.rs:4362`; scoped changed-target lint passed. Those findings are not claimed fixed or classified as root/deadline regressions.

Real device failure, full physical interaction, non-Windows/full release behavior and actual HMR release-before-unload were not established by the scoped T0001–T0003 results; these are historical evidence limits, not active acceptance followups.
