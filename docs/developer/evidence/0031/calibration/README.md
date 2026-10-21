# T0001 calibration capture — 2026-10-21 (post-fix build)

**Source:** [T0001](../../../../../.grimoire/ticket/0019-bounded-history-and-performance-budgets/T0001-measurement-and-calibration.md), [Spec 0031 R4–R6](../../../../../.grimoire/spec/0031-bounded-history-and-performance-budgets.md), [ADR 0072](../../../../../.grimoire/adr/0072-dual-bounded-terminal-history.md), [ADR 0073](../../../../../.grimoire/adr/0073-session-history-accounting-and-text-unit-overflow.md), protocol [history-calibration.md](../../../history-calibration.md).
**Capture date:** 2026-10-21 (UTC 03:43–03:45). **Result:** deterministic candidate measurements complete and repeatable; a recommended numeric contract is derived below. Native/GPU/presentation thresholds remain **not supplied** — see [Gaps](#gaps-that-still-block-numeric-acceptance).
**Adopted by the owner on 2026-10-21** as [ADR 0075](../../../../../.grimoire/adr/0075-calibrated-history-and-text-unit-limits.md). Nothing here changes shipping behavior — T0002/T0003 implement the values.

## Provenance

- Revision `8a92562` plus the dirty tree recorded during the fix. Rust scope measured:
  `content_anchor.rs`, `io.rs`, `normal_buf.rs`, `parser/tests.rs`, `screen.rs` modified (+61/−11);
  `examples/output_ingest_probe.rs`, `src/history_calibration.rs`, `src/projection_diagnostics.rs` untracked.
  The only behavioral change is [ADR 0074](../../../../../.grimoire/adr/0074-demand-driven-content-anchor-tracking.md)
  stage 1 (demand-driven content-anchor tracking); the rest is test-only instrumentation.
- Build: `cargo build --release -p harbor-terminal --example monospace_prepare` and
  `cargo test --release -p harbor-terminal --lib --no-run`, default features, no `dhat-heap`,
  no tracing override. Build time excluded from every statistic.
- Machine/tool environment unchanged from [`../environment.json`](../environment.json):
  Windows 11 `10.0.26300`, i5-13600KF (14C/20T), 34183016448 B RAM, AMD RX 7900 XT,
  driver `32.0.31041.1004`, Vulkan backend, Consolas cell 9×19, 96 DPI.
- Commands run serially, three separate processes each, in this order per the protocol:

```sh
cargo test --release -p harbor-terminal --lib calibration_accounting_controls -- --nocapture --test-threads=1
cargo test --release -p harbor-terminal --lib calibration -- --ignored --nocapture --test-threads=1   # ×3
cargo test --release -p harbor-terminal --lib osc_throughput_probe -- --ignored --nocapture --test-threads=1  # ×3
./target/release/examples/monospace_prepare.exe                                                        # ×3
```

Raw stdout for all nine processes: [`raw/`](raw/). Statistics below pool 3 processes × 7 observations
per case (n=21) except the microbenchmarks, which report their own 2000-sample median/p95 per process.

## 1. Retained-history storage (`calibration_history_candidates`)

Owned backing arrays only (cells, cell provenance, row metadata, row versions). Excluded: inline owner,
damage storage, suffix heaps, allocator overhead, native/font/GPU, temporaries. Sizes:
`Cell` 56 B, `CellState` 1 B, `RowMetadata` 40 B, row version 8 B.
Capacity is `rows + 24` visible rows.

| History rows | Cols | Backing bytes | Modeled resize old+new | Construct median / p95 | Rectangular resize median / p95 |
| --- | --- | --- | --- | --- | --- |
| 1000 | 80 | 4.72 MB | 9.5 MB | 0.74 / 1.00 ms | 1.12 / 1.39 ms |
| 4000 | 80 | 18.4 MB | 37.3 MB | 2.97 / 3.25 ms | 4.22 / 4.78 ms |
| 16000 | 80 | 73.3 MB | 149 MB | 11.1 / 12.3 ms | 17.7 / 19.8 ms |
| 1000 | 160 | 9.39 MB | 18.9 MB | 1.61 / 1.88 ms | 2.48 / 2.91 ms |
| 4000 | 160 | 36.7 MB | 74.1 MB | 5.97 / 6.66 ms | 8.83 / 10.1 ms |
| 16000 | 160 | 147 MB | 295 MB | 22.1 / 26.5 ms | 34.0 / 36.9 ms |

Per-row cost is `57 × cols + 48` bytes: 4608 B/row at 80 columns, 9168 B/row at 160 columns.
Construction and rectangular resize are linear in retained rows; neither is primary reflow or a PTY resize.

## 2. Bounded text units (`calibration_text_unit_candidates`)

Grid 80×2, parser + engine feed timing only (no PTY, render or present), chunk 7, primary buffer.
Alternate-screen samples were within noise of these. Complete current source retention and a following
independent `Z` were asserted for every sample.

| Case | Source bytes | Suffix capacity | Feed median / p95 |
| --- | --- | --- | --- |
| combining control `e◌́` | 3 | 8 B | 0.9 / 2.9 µs |
| selector control `❤️` | 6 | 8 B | 1.1 / 1.8 µs |
| ZWJ control `👩‍💻` | 11 | 8 B | 1.5 / 3.6 µs |
| combining growth ×32 | 65 | 64 B | 4.1 / 5.2 µs |
| selector growth ×128 | 99 | 128 B | 5.4 / 8.1 µs |
| ZWJ growth ×128 | 228 | 256 B | 10.0 / 12.1 µs |
| combining growth ×128 | 257 | 256 B | 12.7 / 15.1 µs |
| selector growth ×512 | 387 | 512 B | 14.3 / 17.5 µs |
| ZWJ growth ×512 | 900 | 1 KiB | 35.3 / 38.3 µs |
| combining growth ×512 | 1025 | 1 KiB | 46.5 / 50.8 µs |
| ZWJ growth ×4096 | 3588 | 4 KiB | 132 / 145 µs |
| combining growth ×4096 | 8193 | 8 KiB | 349 / 354 µs |
| selector growth ×4096 | 12291 | 16 KiB | 393 / 404 µs |
| ZWJ growth ×4096 | 28676 | 32 KiB | 1039 / 1134 µs |

Chunk 1 (deliberately splitting UTF-8 scalars) costs ≤ 1.4× the chunk-7 median for the largest unit.
The `prefix_model` lines in [`raw/`](raw/) report the scalar-aligned admitted prefix for 64/256/1024 B:
controls are wholly admitted at every candidate; 64 B truncates a 65 B combining cluster (2 B lost) and
would discard 165 B of a 228 B ZWJ unit; 256 B truncates 387 B and larger; 1024 B admits everything up
to and including the 1025 B combining growth except its final 2 bytes.

These timings supersede the pre-fix numbers: before ADR 0074 stage 1, each fed chunk also paid a
retained-content projection build (~1 ms at a 1000-row history), so single-unit feed costs in the earlier
ledger were dominated by that rebuild, not by text-unit handling.

## 3. Existing microbenchmarks

| Metric | Process 1 | 2 | 3 | Unit |
| --- | --- | --- | --- | --- |
| OSC throughput median (9 samples, 1 warmup dropped) | 98.18 | 98.21 | 98.57 | MiB/s |
| `monospace_prepare` clean, 2000 samples | median 4.7 / p95 4.9 | 4.7 / 4.9 | 4.7 / 5.3 | µs |
| `monospace_prepare` one_cell | median 2350.7 / p95 2602.3 | 2354.5 / 2634.5 | 2371.2 / 2706.9 | µs |
| `monospace_prepare` full | median 2349.8 / p95 2639.1 | 2419.9 / p95 3113.0 | 2375.9 / p95 2819.9 | µs |

`monospace_prepare` excludes draw, PTY and readback; its R8/vertex message describes allocations, not
measured residency.

## Recommended numeric contract

Derived from the tables above. Applicability: same machine, release build, default features, 80/160-column
grids, 24 visible rows — not a promise for other hardware.

| Value | Recommendation | Rationale |
| --- | --- | --- |
| Default primary history rows | **4000** | 18.4 MB (80 cols) / 36.7 MB (160 cols) of owned backing; 2.97–5.97 ms construct and 4.22–8.83 ms resize medians are imperceptible next to a PTY resize; 4× the current 1000-row depth without crossing the 100 MB class. |
| Maximum accepted history rows | **16000** | 73.3 / 147 MB backed storage is the last step before the modeled resize overlap (149 / 295 MB) becomes a material single-threaded allocation spike; observed resize stays ≤ 36.9 ms p95. |
| Default retained-content bytes | **4 MiB** | Binds at 52 428 uniform ASCII rows (1 B/col) — i.e. never within the row cap — and at 17 476 dense 3 B/col rows, just above the maximum row cap. It is a safety net for short-wide or pathological rows, not the operating constraint. |
| Maximum retained-content bytes | **16 MiB** | 4× the default; at 3 B/col it admits 69 905 rows, so it never becomes the effective limit for ordinary text while bounding worst-case retained source if a future default raises rows. |
| Fixed per-unit protective limit | **1024 source bytes** | Admits every ordinary control (≤ 11 B) and growth clusters through 1 KiB at ≤ 50.8 µs p95; the next step (3.6–28.7 KiB) costs 132–1039 µs and holds 4–32 KiB suffix per cell. 64 B truncates a 65 B combining cluster and 165 B of a 228 B ZWJ unit; 256 B truncates 387 B and beyond, so both discard realistic extended clusters rather than only pathological ones. |
| Ingest regression threshold (2000-row Unicode burst, 107×32, one-row feed cadence) | **median ≤ 60 ms, p95 ≤ 70 ms** | Measured 41.1–41.7 ms median with ≤ 0.7 ms spread after ADR 0074 stage 1; a bound at ~1.45× the median catches a regression back toward projection rebuilds (pre-fix 4080–4420 ms) while tolerating scheduler noise. |
| OSC throughput threshold | **median ≥ 90 MiB/s** | Measured 98.18–98.57 MiB/s across three processes (± 0.4 %). |
| `monospace_prepare` threshold | **clean median ≤ 6 µs; one_cell/full median ≤ 2.6 ms, p95 ≤ 3.4 ms** | Measured 4.7 µs and 2.35–2.42 ms median, worst p95 3.11 ms. |
| Ring construct / rectangular resize threshold | **construct ≤ 7 ms and resize ≤ 11 ms at 4000×160; ≤ 26 ms / ≤ 37 ms at 16000×160** | Measured maxima at those sizes: 5.97/8.83 ms and 22.1/34.0 ms medians, p95 6.66/10.1 and 26.5/36.9 ms. |

The three performance rows are **microbenchmark-scope regression bounds for this class of machine**, not
native acceptance: they say nothing about input-to-present latency, resize-to-correct-frame time, idle CPU/GPU,
process residency or atlas pressure.

## Gaps that still block numeric acceptance

1. **Native output/presentation metrics** — 20000-row host bursts, output-to-present throughput, sustained
   delayed output. The 2026-10-20 GUI captures exist but their producer timings are a recorded negative
   result (buffering, not ingest), and they predate ADR 0074 stage 1.
2. **Resize latency** — request → committed geometry and request → correct frame, 80×24 ↔ 160×40, 30 cycles.
3. **Idle CPU/GPU, frame/upload work, process residency, atlas/GPU residency** — needs the PowerShell sampler
   in a real session plus GPU tooling; `monospace_prepare` messages are allocation facts.
4. **Text-unit protection semantics** — the 1024 B recommendation rests on a scalar-aligned *prefix model*,
   not on unit recognition, streaming admission, discard recovery or future consumer behavior (T0002).
5. **Effects on/off, font reload, session lifecycle (10× four sessions)** — host-matrix rows still BLOCKED as
   the protocol records; the lifecycle/font captures in the ledger predate the fix and do not cover the full matrix.

Until these exist, T0004's numerical integrated acceptance stays blocked even if the recommended capacity and
per-unit values are accepted for T0002/T0003.

## Reproduce

```sh
cargo build --release -p harbor-terminal --example monospace_prepare
cargo test --release -p harbor-terminal --lib --no-run
cargo test --release -p harbor-terminal --lib calibration -- --ignored --nocapture --test-threads=1
cargo test --release -p harbor-terminal --lib osc_throughput_probe -- --ignored --nocapture --test-threads=1
./target/release/examples/monospace_prepare.exe
```

Verify the revision and dirty scope match this record before comparing values; the ingest threshold in
particular changes meaning if a build predates ADR 0074 stage 1.
