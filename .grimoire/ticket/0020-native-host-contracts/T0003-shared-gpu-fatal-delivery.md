# Complete Shared GPU Fatal Delivery

**Ticket ID:** T0003
**Source:** [spec 0042](../../spec/0042-native-host-roots-deadlines-and-gpu-outcomes.md) R3–R6, E3–E5; [ADR 0088](../../adr/0088-native-host-root-deadline-and-shared-gpu-fault-contracts.md) D3–D7; review F13.
**Status:** Todo

## Goal

Actual GPU device notifications reach persistent SharedGpu fault state, wake an idle/minimized UI thread, produce explicit Host fatal reporting and reach Harbor's consistent exit policy across main, paste and OSC52 windows.

## Affected Surfaces

- `crates/harbor-widget/src/winit/{gpu,presenter,host,mod}.rs` and the necessary generic notification-progress/wake/outcome seam.
- `src/{event,shell,dialog}.rs`, shared-GPU initialization/reuse and main/paste/OSC52 confirmation consumers.
- Callback-path, frame-policy, multi-window/native lifecycle and application confirmation-safety tests; affected public callers and native-host docs/examples.

## Approach

Keep the end-to-end chain together: actual wgpu device-lost/uncaptured-error notifications → persistent device state → UI wake independent of redraw → explicit frame/non-frame Host report → application diagnosis/exit. Establish how backend notifications progress even when all sharing windows are non-drawable. Concrete type names and transport/storage details are not frozen; they must not substitute test-only notification injection or redraw dependence for production delivery.

Preserve local surface recovery and asynchronous reporting truth. Normal device destruction is not an unexpected fault. Callback threads record/wake only. Registrations follow native/shared-GPU lifetimes and do not retain unloaded-generation callbacks or dead Hosts.

## Dependencies and Coordination

- **Blocked by:** None.
- **Blocks:** T0004's integrated E3–E5 verdict until actual notification wiring and consumers exist.
- **Coordination risks:** T0001 shares builder/initialization tests; T0002 shares presenter/idle/wait consumers. Agree those contracts or integrate serially. Do not split this ticket into a delivered callback-only library change with unconverted application consumers.

## Acceptance

- [ ] Device-lost and uncaptured-error facilities are connected in production shared-device initialization, covering subsequent initialization, widget and borrowed external GPU use; category/diagnostics survive through Host reporting. (R3/R4)
- [ ] OOM, unexpected DeviceLost and uncaptured Validation/Internal are fatal; expected normal destruction is not. Existing surface recovery remains distinct; no automatic Device/Queue reconstruction occurs. (R4/E4/E5)
- [ ] One Host's observation/closure cannot consume or reset the Device fault for another sharing Host. Independent Runtime/input/surface state is preserved. (R3/R5/E3/E5)
- [ ] The actual notification/progress/wake path reaches the UI thread with no pending redraw/animation, including all sharing windows non-drawable; callback threads never manipulate Runtime/Window or exit. (R3/E3)
- [ ] Encoding, submit/requested-present and later asynchronous notifications reach explicit frame/non-frame fatal reports. An observable fatal is not hidden behind skipped/recovery/wait-only output; submit success is not advertised as GPU completion. (R4/E4)
- [ ] Main, paste and OSC52 confirmation consumers diagnose shared-device fatal and apply application-owned exit, including wake/idle paths. OSC52 does not merely dismiss its dialog; pending PTY/clipboard actions are not confirmed through failure. Non-device dialog behavior is preserved. (R5/E3)
- [ ] Secondary closure, startup failure, normal shutdown, closed transport and notification/teardown races have safe registration lifetimes, with no deadlock/panic/ownership cycle and no retained unloaded-generation captures. Preserve native destruction order. (R3/R6/E5)
- [ ] Callback-path, shared-observation and policy tests distinguish actual production wiring from direct FrameError injection. Report real OOM/device-loss and real DLL-release evidence separately when unavailable. (Verification boundaries)
- [ ] Actual callers, features, docs/examples and tests are migrated as one usable delivery; widget still owns native infrastructure and application still owns exit/EventLoop. (R6)

## Out of Scope

Automatic device recreation/resource replay, global business-event infrastructure, mandatory synchronous GPU error capture/completion, U6 renderer ordering, destructive device-failure experiments without separate authorization and unrelated HMR redesign.
