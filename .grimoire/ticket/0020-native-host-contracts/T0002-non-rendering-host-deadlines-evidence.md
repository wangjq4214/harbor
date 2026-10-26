# T0002 delivery evidence

## Tree and scope

Base: `cfbf6de0365fc6d15bae70dafefc8ce2ad03a61d`, initially clean. Results below apply to that base plus this ticket's working-tree changes:

- `crates/harbor-widget/src/winit/scheduler.rs`: ignore due Host input before arbitration; retain future Host wait demand while non-drawable.
- `crates/harbor-widget/src/winit/host.rs`: public deadline contract documentation.
- `crates/harbor-widget/tests/winit_qa.rs`: zero-size transition, deferred work, restoration and independent-window/business wait regression.
- `crates/harbor-widget/tests/native_host_deadlines.rs`: new opt-in native timer-delivery test.
- `docs/widget/recipes.md`: deadline ownership and suspension guidance.
- This evidence file and the T0002 ticket status.

Environment: Windows, `x86_64-pc-windows-msvc`, rustc 1.99.1. Native tests used the default Windows DX12 selection; no Vulkan-native or physical-minimize result is claimed.

Route: implement → targeted tests → inline review/requirements check → broader affected verification → scoped rustfmt → refreshed affected verification/report. No separate planning artifact was needed for this bounded, reversible scheduler change. No delegation or human checkpoint was used.

## Criterion-to-evidence audit

| T0002 acceptance | Result and evidence |
| --- | --- |
| Future Host demand survives non-drawability; no demand stays idle | Match: `suspended_host_deadlines_wake_without_rendering_or_spinning`, presenter regression and actual native timer wakes. |
| Absent/due/repeated expired input does not spin, redraw, Poll or return past WaitUntil | Match: scheduler table covers absent/future/exactly-due/past/repeated/rescheduled input; presenter regression checks normalized effects; native test repeats consumed Host input while the second Host waits. Due business work stays caller-owned. |
| Pending Runtime/recovery demand is suspended; restoration coalesces | Match: `suspended_host_wake_preserves_render_demand_and_coalesces_restoration` covers Poll and Runtime deadlines with deferred recovery, preserving state and one restoration edge. Existing due/recovery suspension tests remain green. Presenter uses the actual zero-size transition. Native Host frame attempts return Skipped while suspended and present after restoration. The production acquisition guard remains before surface acquisition/encoding/submission. |
| Drawable arbitration, independent storage and earliest cross-window wait | Match: new expired-Host regression prevents stale input masking Runtime/recovery waits; existing Runtime/Host and Poll/coalescing tests remain green. Presenter and native tests arbitrate two independent windows; application effects tests verify real merge/mapping helpers. |
| Native wake evidence is distinguished from scheduler assertions | Match: the new native test receives two actual winit ResumeTimeReached notifications for the requested deadlines, not just matching return values. It uses real hidden Hosts sharing a Device, injected zero-size transitions and the real event loop. |
| Migration, ownership and scope | Match: public docs updated; Host outcome/API shapes unchanged. Inspected Host → presenter → scheduler → native effects and application shell/dialog wait propagation. Existing shell notification/business timers and confirmation arbitration remain intact; no application caller migration was needed. No GPU callback/fatal policy or ownership change. |

Inline review of the complete diff, new native test and affected consumers found no confirmed change-induced blocker or requirement gap. A due Host deadline masking a future drawable Runtime/recovery deadline was reproduced and fixed within the ticket's arbitration requirement. Three new scheduler tests failed against the old production implementation and passed after the fix.

## Commands and results

All commands ran from the repository root. Final Rust checks were refreshed after scoped formatting.

- `cargo test -p harbor-widget --features winit --lib --test winit_qa --test winit_contracts --test runtime_effects`: **684 passed** (610 library, 31 QA, 25 public contracts, 18 effects).
- Same test targets with `--features hmr`: **688 passed** (613 library, 31 QA, 26 public contracts, 18 effects).
- `cargo test -p harbor --bin harbor effects::tests::`: **3 passed**, actual application wait arbitration/mapping/optional merge.
- `cargo test -p harbor-widget --features winit --test native_host_deadlines -- --ignored --nocapture`: **1 passed**; repeated successfully after formatting.
- `cargo test -p harbor-widget --features winit --test native_host_smoke -- --ignored`: **1 passed**, existing native presentation/shared-GPU/lifecycle smoke.
- `cargo check --workspace --all-targets --all-features`: **passed**, including debug HMR and backend feature declarations.
- `cargo check -p harbor --features widget-hot-reload --release`: **passed**.
- `cargo clippy -p harbor-widget --all-features --lib --test winit_qa --test winit_contracts --test runtime_effects --test native_host_deadlines -- -D warnings`: **passed**.
- Broader `cargo clippy -p harbor-widget --all-targets --all-features -- -D warnings`: **failed on an unrelated existing lint**, `clippy::chunks_exact_to_as_chunks` at `crates/harbor-widget/tests/quad_renderer.rs:4362`. `git diff --exit-code HEAD -- crates/harbor-widget/tests/quad_renderer.rs` confirmed that file is unchanged. Not a T0002 defect; broad lint is not claimed passing.
- `rustfmt --check --edition 2024 --config skip_children=true` on the four changed Rust files: **passed**.
- `git diff --check`: **passed**.
- `python scripts/docs/check_docs.py`: **passed**.

## Boundaries and follow-up

The native test deliberately injects zero-sized Host geometry and ignores OS startup resize messages so they cannot restore rendering during the timer measurement. It proves native timer delivery while Host rendering is suspended, not physical Windows minimize/restore event delivery. No GPU completion trace, native Vulkan test, full workspace test run, real GPU fault or HMR DLL-release trace is claimed. These are not replaced by scheduler or compile assertions.

T0002's selected gates establish its scoped result. The unrelated broad lint failure remains disclosed. T0003 retains GPU fault delivery ownership, and T0004 still owns the combined U5 acceptance verdict.
