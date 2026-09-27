# T0001 GPU-free terminal engine boundary

Intermediate verification at base revision `b055408a13c0e40e136d5d31e354562575d34638` with uncommitted scoped changes in `crates/harbor-terminal/`, `crates/harbor-app/src/terminal_view.rs`, `.grimoire/plans/0011-gpu-free-terminal-engine-update.md` and `docs/verification/`. This is not the final T0005 integrated application/Windows acceptance.

## Dependency proof

From repository root:

```sh
cargo check -p harbor-terminal --no-default-features
cargo test -p harbor-terminal --no-default-features --lib
cargo tree -p harbor-terminal --no-default-features -e all --prefix depth > docs/verification/terminal-core-only-dependency-tree.txt
rg -i '(wgpu|arboard|winit|harbor-widget)' docs/verification/terminal-core-only-dependency-tree.txt
```

`cargo check` PASS; core library test PASS (630 passed, 1 ignored), including parser, screen, input, pointer/selection, damage and timing/update cases. The graph artifact has 604 lines; the excluded-name search has **zero matches** across normal, build and dev dependency edges. Default `cargo check -p harbor-app` PASS. The pre-change default graph and performance baseline are recorded in [terminal-boundary-pre-migration-baseline.md](terminal-boundary-pre-migration-baseline.md).

The `renderer` feature is enabled by default and owns optional wgpu and the concrete render module. The GPU-free build retains terminal I/O and PTY dependencies, but does not own GPU/widget dependencies. The transitional default facade still owns PTY I/O and the renderer; session lifetime migration belongs to T0002/T0004. The host still owns the GPU device, surface and presentation. `read_update` owns a snapshot, damage, selection, preedit, effective appearance and frame demand. Its first update (or one after invalidation) is full; later updates provide accumulated dirty ranges until `acknowledge_update` validates unchanged state after renderer preparation. Reading never clears visual damage, and a failed/skipped projection must not be acknowledged. Existing rendered facade still prepares directly; migration of its renderer to consuming the new update is T0003.

## CPU-side proxy, intermediate comparison

Same ignored release-mode integration probe and host described in the baseline document, run from repository root:

```sh
cargo test -p harbor-terminal --release --test session_baseline -- --ignored --nocapture
```

| Sessions | Pre-change median updates/s | T0001 median updates/s | T0001 sorted seconds/sample |
| --- | ---: | ---: | --- |
| 1 | 1,581.4 | 1,630.0 | 0.2349631, 0.2373954, 0.2426665, 0.2454038, 0.2538896, 0.2572206, 0.3087997 |
| 8 | 1,062.7 | 1,047.6 | 2.9884177, 3.0244339, 3.0362718, 3.0544778, 3.0649661, 3.0771402, 3.0933102 |

Probe PASS; values vary by ~3% or less from pre-change median, with no threshold or speedup claim. This is serial CPU-side headless processing, **not** actual GPU frames, PTY throughput, concurrent sessions or final post-T0004 performance. Interactive Windows/ConPTY/GPU create/resize/hide/close/failure/sustained-output cases are NOT RUN under this ticket; T0005 owns them.

## Gates

- `cargo fmt --all --check`: PASS.
- `cargo clippy --all-targets --all-features -- -D warnings`: PASS (Windows incremental-cache permission note did not affect exit status).
- `cargo test --workspace --quiet`: PASS after updating app tests for headless blink scheduling.
- `python scripts/check_docs.py`: PASS.
- `python scripts/checklist_summary.py`: PASS.
- `git diff --check`: PASS.

Renderer and application still build, and focused core/full tests pass; GPU runtime visuals and PTY teardown were not manually exercised. This document records executable evidence for T0001, not package completion.
