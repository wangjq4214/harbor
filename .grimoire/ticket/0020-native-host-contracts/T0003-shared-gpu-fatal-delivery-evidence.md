# T0003 delivery evidence

## Scope and result

Base `9e3433cd50bf8919aa1236e4c5e65b765599320c`, initially clean. Results apply to that revision plus this ticket's working-tree changes (including new `gpu_fault.rs` and `native_gpu_faults.rs`). No commit was created.

Route: plan → implementation/tests → inline review and requirements check → affected fixes → integrated verification → scoped formatting → refreshed affected verification/report. Planning was useful for the device-lifetime worker, public transport and coupled application migration. [Plan](../../plans/0056-shared-gpu-fatal-delivery.md). No delegation, human checkpoint or destructive GPU experiment.

Implementation and selected automated/native gates are complete, with **no confirmed remaining blocker**. Full physical-device-failure and real DLL-release acceptance is **not claimed**. The ticket's status/checkboxes are unchanged rather than promoting partial native evidence to full E3–E5 acceptance; T0004 retains the integrated verdict.

## Delivered behavior

- SharedGpu installs real lost/uncaptured handlers immediately after request_device, before surface/renderer/root use. First categorized diagnostic is persistent; observations and secondary closure cannot drain it.
- GpuWake transports a unit notification through the application's typed EventLoopProxy. Bootstrap requires `.with_gpu_wake(...)`; reuse inherits the original Device's transport. Missing bootstrap transport fails explicitly. No callback captures a Host, Runtime, Window, business state or reloadable root.
- One joined worker per Device performs nonblocking poll every 50 ms, independent of redraw/animation/drawability. DX12 also reads GetDeviceRemovedReason under a temporary HAL guard.
- OOM, unexpected DeviceLost, Validation and Internal are fatal. Normal Destroyed notifications are ignored. Local surface recovery and Device/Queue identity remain unchanged.
- Host frame, event and idle boundaries expose shared fatal; `gpu_fault()` provides explicit non-frame observation. Final observations occur after effect application. Presented means submission/requested presentation, not completion.
- Harbor main/paste/OSC52 wake, idle and event paths use application-owned diagnosis/exit. Shared-fatal cancellation clears pending paste/clipboard approval. Local OSC52 presentation failures retain dialog-only handling. Shortcuts, queued buttons and approved clipboard effects check failure before committing.
- Callers, root compilation contracts, native smoke/deadline tests, README and native recipe migrated together.

## Criterion-to-evidence audit

| Ticket acceptance | Evidence / verdict |
| --- | --- |
| Production callbacks cover subsequent initialization, widgets and borrowed external GPU use; diagnostics survive | Match in inspected implementation and real validation tests: failed bootstrap output is dropped, failed reuse leaves the primary fault observable, external draw validation reaches HostFrameOutcome::Fatal with SharedGpu category/detail. |
| Fatal categories, normal destruction, distinct local recovery, no reconstruction | Classification tests exercise the actual callback handlers for all categories; native Device::destroy produces no fault/wake. Existing acquisition/recovery tests pass. No reconstruction code added. Actual OOM/unexpected hardware loss remains unverified. |
| Shared observation and independent window state | Native two-Host test observes secondary first, closes it, then observes the same primary fault. Existing native lifecycle test preserves shared GPU identity and separate viewport state; input isolation tests pass. |
| Independent progress and UI wake while all windows non-drawable | Native map callback completes with only the production polling worker. Real validation from a delayed worker wakes a ControlFlow::Wait event loop with two zero-size Hosts and no rendering. Both DX12 and Vulkan pass. Native minimize is requested, but zero-size state is injected and OS resize/redraw events ignored: this is not complete physical minimize/restore interaction proof. |
| Encoding, submit/requested-present and asynchronous reporting | Native external encoding validation and destroyed-fixture-buffer validation from the Host's actual Queue::submit both yield explicit shared fatal (submission test asserts Queue::submit diagnostic). A successful requested frame followed later by real validation is reported on a non-frame wake. Zero-size frame attempts become Fatal, not Skipped. Local recovery tests remain green. |
| Main/paste/OSC52 application exit and approval safety | Inspected all application routes to apply_gpu_fatal_policy. Category-matrix tests verify pending clipboard cancellation, paste input-gate release before exit and no late approval. Production decision helpers for queued-button/shortcut approval are overridden by shared fatal and the PTY writer is not called. Existing confirmation, clipboard, command-gating and generation tests pass. No full running-Harbor hardware-failure scenario is claimed. |
| Shutdown, failed construction, closed transport and races | Real bootstrap/reuse failure, repeated native secondary lifecycle, actual normal device destruction, first fault sent to a closed EventLoopProxy, and eight notification/monitor-drop races pass. Weak callbacks cannot retain monitor state; worker joins before GPU fields drop. Existing HMR barriers and Runtime/surface/platform/window ordering preserved. Actual DLL unload tracing remains unverified. |
| Evidence classes distinguished | Deterministic category/policy tests, actual wgpu validation, independent poll progress, native multi-window proxy wake and unavailable hardware/DLL evidence are reported separately here. |
| Usable migration and ownership | All workspace targets/features compile; debug-HMR tests and release-HMR check pass. Documentation examples compile. Application retains EventLoop/business/exit ownership; widget owns native infrastructure. |

## Review findings addressed

1. wgpu-hal 30 DX12 get_fence_value returns GetCompletedValue directly, including the removal sentinel. Poll alone did not establish idle removal detection. Added a read-only native GetDeviceRemovedReason check feeding the same DeviceLost state; no resource mutation or raw handle escape.
2. Moved final fault observations after native effects, which can drive further Runtime work; HMR no-op returns also retain observable fatal state.
3. Expanded native evidence beyond direct validation injection: actual Host Queue::submit failure, failed shared initialization, a first notification after proxy closure and callback/teardown races.

Review included the full tracked diff, both new Rust files, native/public callers, relevant wgpu 30 poll/HAL implementation, and application approval/exit consumers. No unrelated architecture, HMR redesign or renderer-order work was included.

## Environment and commands

Windows `x86_64-pc-windows-msvc`, rustc 1.99.1. Cargo lock: wgpu 30.0.0, wgpu-core 30.0.1, wgpu-hal 30.0.0.

Native adapters:
- DX12: AMD Radeon RX 7900 XT, driver 32.0.32015.7029.
- Vulkan: AMD Radeon RX 7900 XT, AMD proprietary driver 26.10.1 (LLPC).

Commands ran from the repository root:

- `cargo test --workspace --quiet`: **2,844 passed, 0 failed, 21 ignored**. Non-failing incremental-cache access-denied notes and existing macro-fixture parentheses warnings appeared. Ran before formatting; affected targets were refreshed afterward. The subsequent added submission case is in an opt-in ignored native test and was explicitly executed below.
- `cargo test -p harbor-widget --features hmr --lib --test winit_qa --test winit_contracts --test runtime_effects --quiet`: **690 passed, 1 ignored** (615 library, 26 public contracts, 31 QA, 18 effects), after formatting.
- `cargo test -p harbor --features widget-hot-reload --bin harbor --quiet`: **147 passed**, after formatting. Default application suite earlier: 146 passed.
- `cargo test -p harbor-widget --features winit --lib production_callbacks_and_idle_progress -- --ignored --nocapture`: **1 passed**, including real callback/progress/normal-destruction/teardown-race coverage.
- `cargo test -p harbor-widget --features winit --test native_gpu_faults --test native_host_smoke --test native_host_deadlines -- --ignored`: **3 passed**. Final expanded native_gpu_faults submission case also rerun separately and passed.
- Repeat callback/progress test and `--test native_gpu_faults -- --ignored` with `--features winit,backend-vulkan`: **2 passed**, including final expanded submission case.
- `cargo check --workspace --all-targets --all-features`: passed.
- `cargo check -p harbor --features widget-hot-reload --release`: passed.
- `cargo clippy -p harbor-widget --all-features --lib --test winit_qa --test winit_contracts --test runtime_effects --test native_gpu_faults --test native_host_smoke --test native_host_deadlines -- -D warnings`: passed.
- `cargo clippy -p harbor --all-features --bin harbor -- -D warnings`: passed.
- `python scripts/docs/check_docs.py`: passed.
- `python scripts/docs/check_examples.py`: **17 passed**, native example compile-only.
- Scoped `rustfmt --check --edition 2024 --config skip_children=true` on all 12 changed/new Rust files and `git diff --check`: passed. All-feature/release/lint/docs checks refreshed after final Rust writes.

## Remaining evidence boundaries

No GPU exhaustion, driver reset or unexpected physical device loss was induced. Actual lost-callback wiring and DX12 status-query implementation are established by source; category behavior is deterministic-handler evidence, not a hardware failure trace. Vulkan physical loss detection timing remains driver/backend dependent.

No real hot-reloaded DLL release trace or full Harbor main/paste/OSC52 hardware-failure session was collected. Static lifetime analysis, HMR feature tests and policy tests are not substitutes for those traces. Physical minimize/restore interaction, non-Windows backends and full release runtime behavior remain outside these executed checks.

The plan directory is ignored by this repository's Git configuration, so the plan is saved locally. The new ticket evidence file is untracked and available for normal inclusion. No ignore rules were changed and nothing was force-added.
