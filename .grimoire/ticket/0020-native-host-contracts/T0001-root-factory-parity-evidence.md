# T0001 root-factory parity — execution evidence

## Scope and verdict

T0001's five acceptance criteria match the implementation and evidence below. No confirmed change-induced blocker was found in inline review/check. This is a T0001 verdict, not integrated acceptance of spec 0042 or T0002–T0004.

Route: implement → focused compilation/native tests → inline review/check → formatting and broader/final verification → report. No separate plan or delegation: the reversible API change uses the existing Runtime mount path, without consequential approach changes. Tests, fixes, formatting and reporting were enabled; no human checkpoint was requested.

## Execution tree

- Executed on 2026-10-26, Windows, `x86_64-pc-windows-msvc`, Rust/Cargo 1.99.1.
- Base revision: `a6da35c5b892eff62f660d4ea0c8f769d7219e87`, with this task's changes to:
  - `crates/harbor-widget/src/winit/host.rs`
  - `crates/harbor-widget/tests/winit_contracts.rs`
  - `crates/harbor-widget/tests/native_host_smoke.rs`
  - `docs/widget/recipes.md`
- ADR 0088, spec 0042 and the ticket directory were already untracked at task start; they are source inputs, not newly delivered GPU/deadline implementation. No other production changes were present.
- This evidence document and T0001's status update are bookkeeping after behavioral verification; they do not change the compiled tree.

## Criterion-to-evidence check

| T0001 criterion | Result and evidence |
| --- | --- |
| Both builders accept ordinary Component, macro View and existing View | Match. Six explicit public type-check bodies in `winit_contracts.rs`; corresponding native matrix in `native_host_smoke.rs`. Temporarily restoring the old bounds makes the compile contract fail with E0277 on all four View cases; restored widened bounds pass. |
| One-shot factory, setup ordering and Runtime-owned build/Fiber/hooks | Match. Native trace asserts window/surface setup → one factory call → one build/hook initialization. Descriptions are not built before setup. A hook Signal write rebuilds the mounted root and preserves hook state. Production still calls `runtime.set_root(root)`; existing `IntoChildView for View` passes the description through unchanged. |
| Factory errors/panics, root panic attribution and atomic output | Match. Native cases cover factory Err, factory panic and deferred root-build panic through both builders, checking one invocation, RootConstruction stage and diagnostic provenance. Successful output is checked; a drop sentinel proves failed mounting releases output rather than returning it. |
| Bootstrap/reuse, initial update/focus/redraw and actual callers | Match. Existing native primary/secondary lifecycle checks remain; added matrix bootstraps and reuses GPU resources, routes IME commit through initial focus without an explicit focus request, and presents each root. Main/confirmation/static/HMR callers compile and their relevant suites pass. ApplicationRoot/MountedRoot and confirmation closure adapters remain valid type-erasure boundaries; no root identity/HMR redesign was made. |
| Migrated tests/docs, bound evidence and honest native limitations | Match. Native guide now constructs a macro View directly and documents both factories. Guide examples compile/run. Native evidence is Windows/DX12 construction/presentation with synthetic routed IME, not physical interaction or actual HMR unload tracing. |

## Commands and results

All commands ran from the repository root. Detailed local logs are under `target/T0001-*.log` (build artifacts, not committed evidence).

Passing:
- `cargo check -p harbor-widget --features winit --tests`
- `cargo test --workspace` — default workspace suites, macro compile fixtures and doctests; existing ignored tests remain ignored.
- `cargo check --workspace --all-targets`
- `cargo check --workspace --all-targets --features backend-dx12`
- `cargo check --workspace --all-targets --features backend-vulkan`
- `cargo test -p harbor-widget -p harbor --features harbor/widget-hot-reload`
- `cargo check -p harbor --features widget-hot-reload --release`
- `cargo test -p harbor-widget --features winit --test native_host_smoke -- --ignored --nocapture` — one real native/GPU test passed before formatting.
- `cargo test -p harbor-widget --features hmr,backend-dx12 --test native_host_smoke -- --ignored --nocapture` — final formatted tree, one real native/GPU test passed. Panic-sentinel output is expected and caught by Host startup.
- `cargo clippy -p harbor-widget --features hmr --lib --test winit_contracts --test native_host_smoke -- -D warnings`
- `python scripts/docs/check_examples.py --include-readmes` — 24 examples passed, including the native no_run recipe.
- `python scripts/docs/check_docs.py` — 178 documentation files passed.
- `cargo fmt --all -- --check`
- `git diff --check`

Broad lint limitation:
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` fails on `clippy::chunks_exact_to_as_chunks` at unchanged `crates/harbor-text/src/backend/dwrite/sequence/tests.rs:172,175`.
- `cargo clippy -p harbor-widget --features hmr --tests -- -D warnings` fails on the same lint at unchanged `crates/harbor-widget/tests/quad_renderer.rs:4362`.
- Both files were verified identical to HEAD with `git diff --quiet HEAD -- <path>`. These are pre-existing-source lint failures on Rust 1.99.1, not a demonstrated T0001 defect. Full repository lint is not claimed green; scoped changed-target lint passes. No unrelated lint fixes were made.

## Remaining evidence boundaries

- Vulkan was compile-checked, not exercised as a native presentation backend.
- Native focus/IME assertions use synthetic routed events, not user typing, actual IME composition or fractional-DPI interaction.
- No real DLL generation-release/unload ordering proof is supplied. Existing HMR lifecycle obligations remain; debug tests and release compilation do not discharge them.
- This change does not implement or verify non-rendering deadlines, production GPU fault callbacks, minimized/shared-fatal delivery, device loss/OOM or application fatal-exit policy. Those remain with T0002/T0003 and integrated T0004.
