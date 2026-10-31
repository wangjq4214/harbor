# Incremental terminal update verification

## Scope and source snapshot

- Date: 2026-10-27.
- Contract: [spec 0045](../../.grimoire/spec/0045-mutation-revisions-and-incremental-terminal-updates.md), [ADR 0094](../../.grimoire/adr/0094-mutation-revisions-and-incremental-terminal-updates.md).
- Base revision: `ef1cde4`; implementation and verification are uncommitted changes on that base. No commit or release acceptance is implied.
- Dirty implementation scope: `crates/harbor-terminal/src/{lib,model,normal_buf,screen,pointer,update,revision,snapshot,update_tests}.rs`; configuration candidate ownership; existing core/palette/history/terminal and renderer fixture migrations; the pipeline test's adapter diagnostic; `examples/update_projection.rs`. Documentation scope: terminal concepts/embedding/security, session ownership, this record and the linked spec/ADR. The implementation plan is `.grimoire/plans/0058-incremental-terminal-updates.md` (local, ignored by existing repository policy).
- Inline review/check covered the complete production/test diff and new files, update consumers, configuration candidates, cache/overflow mutation paths, public ownership changes and spec R1–R5. No confirmed remaining product blocker was found. Remaining evidence/gate limitations below prevent a claim of complete acceptance.

## Environment

Windows 11, version `10.0.26300`; Intel64 Family 6 Model 183 Stepping 1 (20 logical processors). `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, `cargo 1.99.1 (5f94df478 2026-08-27)`.

Native pipeline verification selected AMD Radeon RX 7900 XT, discrete GPU, Vulkan backend, AMD proprietary driver `26.10.1 (LLPC)`. The OS driver query reported `32.0.32015.7029`. The CPU benchmark uses no GPU/backend, fonts, DPI, PTY or ConPTY process; those parameters are not measurements in the benchmark.

## Delivered mechanism

- Acknowledgement checks session/epoch, checked state-owner mutation provenance, presentation revision and exact shared snapshot identity; it does not compare or hash visible cells. Selection/auto-scroll tracks its actual owner, not the outer mutable pointer argument passed through an empty scheduling poll.
- `TerminalUpdate::snapshot` is an `Arc<TerminalSnapshot>`; a private matching reference rejects detached/modified synthetic snapshots without grid equality. Exposed damage and presentation values remain validated.
  - API update (2026-10-31): engine-issued updates now expose this data immutably through `TerminalProjection`. Clone `update.projection()` for synthetic rendering; configuration candidates have only the projection type and cannot be acknowledged. The runtime identity/provenance checks remain as defense in depth. See the current embedding contract.
- `SnapshotCells` shares a visible row directory and immutable row payloads. The cache checks row versions/generations and copies changed rows only, regardless of older updates remaining alive. Unchanged payloads/suffixes are not copied. There is O(visible rows) metadata work, not a claim that every read or renderer layer is O(1).
- Nonsemantic text-overflow bookkeeping invalidates only its cached row without advancing independent semantic content versions or adding visual damage. All ordinary/bulk semantic cell mutation paths continue to use row versions.

See [embedding](../terminal/embedding.md#projection-ownership-and-copying) for compatibility: flat indexing/iteration remains; arbitrary Vec slicing/growth/removal does not. Flat materialization is explicit. Detaching an update snapshot is for synthetic rendering, not engine acknowledgement.

## Requirement-to-evidence check

| Requirement | Evidence and result |
| --- | --- |
| R1: revision acknowledgement, including A → B → A | PASS: cell/cursor/visibility/input-mode/synchronized-output/palette/buffer/preedit/backdrop cases; independent selection A → B → A; session mismatch, tampered snapshot/damage and repeated ack rejection. Production ack has no cell walk. |
| R2: no ordinary unconditional grid copy | PASS: copy-count tests with old updates retained at 2×8, 24×80 and 160×400; unchanged read copies 0 cells, a bounded edit copies exactly 1 row (8, 80 or 400 cells). Synthetic mutation also shares untouched rows. CPU measurements below exercise the real read/ack path. |
| R3: coherent retained projections and non-cell coverage | PASS: authoritative Screen cell/metadata oracle across Unicode/local/structural/buffer/reset/resize/history changes; retained old contents stay stable; metadata-only updates share all rows; overflow refresh preserves semantic-reader validity. |
| R4: explicit consumption and recovery | PASS: existing skipped/hidden update, failed/invalid projection, polling, configuration rejection/rollback, independent reader/search tests, plus current token/replay tests. Empty polling regression was exposed and fixed without weakening its assertion. |
| R5: existing presentation and terminal behavior | PASS in automated scope: full terminal and workspace suites, including native renderer integration and GPU recovery/encoding/submission. Full interactive application/ConPTY acceptance was NOT RUN. |

## CPU measurements

Command, from repository root, before production edits and again after implementation:

```sh
cargo run -p harbor-terminal --release --no-default-features --example update_projection
```

One headless session, 0 history rows/bytes, blank grid, 200 timed iterations per workload, release profile (workspace opt-level 3/LTO), one old update retained throughout (the initialized update also remains in scope). Unchanged iterations do no mutation. Local iterations alternately feed `\rA` / `\rB`, read and acknowledge each update. Durations are per-iteration means in microseconds, split by mutation/read/ack; construction and final old-update release are not timed. Both runs use the same example/workload and machine. Before-production-edit dirty scope was only the new benchmark and refinement artifacts.

| Grid | Workload | Mutation before → after (µs) | Read before → after (µs) | Ack before → after (µs) |
| --- | --- | ---: | ---: | ---: |
| 24×80 | Unchanged | 0.023 → 0.018 | 5.051 → 0.299 | 6.274 → 0.421 |
| 24×80 | Local | 0.422 → 0.296 | 8.035 → 0.900 | 8.684 → 0.389 |
| 80×240 | Unchanged | 0.030 → 0.020 | 229.745 → 0.432 | 70.989 → 0.382 |
| 80×240 | Local | 0.568 → 0.231 | 211.201 → 2.067 | 73.678 → 0.394 |
| 160×400 | Unchanged | 0.062 → 0.019 | 689.372 → 0.701 | 328.129 → 0.425 |
| 160×400 | Local | 1.452 → 0.314 | 680.476 → 3.362 | 320.100 → 0.471 |

These are CPU microbenchmark observations, not whole-frame latency, process-memory budgets, multi-session throughput, statistical confidence intervals or native application responsiveness. No numeric acceptance threshold was invented. Deterministic copy counts and production code-path review establish the requested work removal independently of timing noise.

## Executed gates

All commands below ran from the repository root.

| Command | Result / scope |
| --- | --- |
| `cargo check --workspace --all-targets` | PASS; migrated in-tree producers and consumers compile. |
| `cargo test -p harbor-terminal --no-default-features --quiet` | PASS on final code: 787 unit tests, 41 integration tests and 2 doctests; 5 unit diagnostics and 1 integration diagnostic remain ignored by their existing policy. |
| `cargo test -p harbor-terminal --quiet` | PASS on final code: 1114 unit tests, 89 integration tests and 2 doctests; 9 unit diagnostics and 1 integration diagnostic remain ignored. |
| `cargo test --workspace --quiet` | PASS, including application/widget integration and UI compile fixtures. After the later test-only adapter diagnostic and formatting, the affected terminal suites were rerun as above; unrelated workspace suites were not redundantly rerun. |
| `cargo test -p harbor-terminal --lib gpu_pipeline_prepares_replayed_updates_and_invalidates_projection -- --nocapture` | PASS on the named native Vulkan adapter, not the no-adapter early-return path. Exercises incremental replay, palette/viewport changes, selection/preedit, resize, invalidation/full reconstruction, highlights and GPU draw encoding/submission. Not an OS-window presentation or live-shell test. |
| `cargo tree -p harbor-terminal --no-default-features -e normal --prefix none` | PASS inspection: no wgpu, winit, arboard or harbor-widget normal dependency in core-only graph. |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | FAIL: Rust 1.99's `chunks_exact_to_as_chunks` lint at unchanged existing renderer/text test sites. These lint sites are unchanged by this work; the original tree was not separately rebuilt for a Clippy baseline. No unrelated lint cleanup or repository lint suppression was applied. |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings -A clippy::chunks_exact_to_as_chunks` | PASS supplemental check; explicitly not a strict-gate pass. |
| `cargo fmt --all -- --check`, `git diff --check` | PASS; write formatting was limited to changed/new Rust files. |
| `python scripts/docs/check_docs.py` | PASS; links/language/fragments checked, rerun after final evidence documentation. |
| `python scripts/docs/checklist_summary.py` | UNAVAILABLE: this historical helper does not exist in the current checkout; no replacement evidence is inferred. |

Some test builds emitted incremental-cache access-denied notes while exiting successfully; they did not represent test failures. Local raw test logs are temporary execution artifacts, not required links to understand this summary.

## Remaining limitations

- Strict all-target/all-feature Clippy is not green until the existing lint debt is separately resolved or policy is explicitly changed.
- Full interactive Harbor/ConPTY application acceptance under sustained output, tab hide/show, native device-loss and window presentation was not performed. Automated headless/native-GPU evidence does not assert those checks ran.
- This work does not optimize all renderer-internal whole-grid preparation loops, remove all row metadata allocations or establish a constant-time whole frame.
- Snapshot retention can retain superseded row payloads; this is not a process-memory ceiling or permission to archive unbounded updates.