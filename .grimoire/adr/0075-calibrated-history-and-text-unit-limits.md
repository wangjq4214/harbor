# Calibrated numeric limits for bounded history and text units

**Status:** Proposed
**Date:** 2026-10-21
**Sources:** Owner accepted the recommended contract from the [2026-10-21 calibration capture](../../docs/developer/evidence/0031/calibration/README.md) on 2026-10-21, following [T0001](../../.grimoire/ticket/0019-bounded-history-and-performance-budgets/T0001-measurement-and-calibration.md).
**Refines:** [ADR 0072](./0072-dual-bounded-terminal-history.md) and [ADR 0073](./0073-session-history-accounting-and-text-unit-overflow.md), supplying the numeric values both deliberately left to measurement. Their policy, accounting, reload and protective-exception semantics are unchanged.

## Context

ADR 0072 and ADR 0073 fixed the shape of dual-bounded history — independent physical-row and retained source-content byte ceilings, oldest-first eviction, live budget reload, and a fixed oversized-text-unit exception — but explicitly refused to name numbers without measurement. T0001 then measured the candidate battery on the ADR 0074 stage-1 build: retained-ring backing capacities and construct/resize distributions at 1000/4000/16000 rows and 80/160 columns, bounded text-unit growth through 28 KB, and the OSC/prepare microbenchmarks, three processes per command.

Those measurements show per-retained-row cost is `57 × cols + 48` bytes (56 B `Cell` + 1 B provenance per cell, 40 B metadata + 8 B version per row): 4608 B/row at 80 columns and 9168 B/row at 160 columns, linear construct and rectangular-resize times, and a modeled resize overlap equal to old plus new backing. Text-unit feed cost and per-cell suffix capacity grow with the unit's retained length: ≤ 11 B clusters feed in ~1 µs, 1 KiB in ≤ 51 µs, and 3.6–28.7 KiB in 132–1039 µs with 4–32 KiB suffix per cell.

## Decision

Adopt these values as the shipping contract for T0002/T0003. Applicability is the measured class: release build, default features, 24 visible rows, 80–160 column grids, the environment recorded in the capture (i5-13600KF, Windows 11 10.0.26300). Other hardware must re-derive rather than inherit them.

| Value | Adopted |
| --- | --- |
| Default primary history rows | 4000 |
| Maximum accepted history rows | 16000 |
| Default retained-content bytes | 4 MiB |
| Maximum retained-content bytes | 16 MiB |
| Fixed per-unit protective limit | 1024 source bytes |

Rationale, in the order that decided each value: 4000 rows costs 18.4 MB (80 cols) / 36.7 MB (160 cols) of owned backing with 2.97–5.97 ms construct and 4.22–8.83 ms rectangular-resize medians, while 16000 is the last step before the modeled resize overlap reaches 149–295 MB on a single-threaded path. A 4 MiB content cap never binds uniform ASCII within the row ceiling (52 428 rows) and binds dense 3 B/column text at 17 476 rows — just above the maximum row count — so it protects against short-wide or pathological retention without becoming the operating constraint; 16 MiB keeps that property if a future default raises rows. A 1024-byte unit limit admits every ordinary combining/VS16/ZWJ control and extended cluster through 1 KiB, whereas 64 B truncates a 65-byte combining cluster and 165 bytes of a 228-byte ZWJ unit and 256 B truncates 387 B and beyond — both would discard realistic clusters, not only pathological input.

Adopted regression bounds, applicable to the same machine class and explicitly microbenchmark-scope: ingest of a 2000-row Unicode burst at 107×32 with one-row feed cadence median ≤ 60 ms and p95 ≤ 70 ms; OSC throughput median ≥ 90 MiB/s; `monospace_prepare` clean median ≤ 6 µs and one_cell/full median ≤ 2.6 ms with p95 ≤ 3.4 ms; ring construct ≤ 7 ms and rectangular resize ≤ 11 ms at 4000×160, ≤ 26 ms and ≤ 37 ms at 16000×160.

## Consequences

- T0002 and T0003 gain the numbers they were blocked on; neither may substitute a different value without a superseding record.
- The default becomes 4000 rows rather than the historical 1000, quadrupling retained-ring storage to the 18–37 MB class at typical widths. Users who want the old footprint set the row cap explicitly.
- These bounds say nothing about native output-to-present latency, resize-to-correct-frame, idle CPU/GPU, process/GPU residency or atlas pressure. T0004's numerical integrated acceptance remains blocked on those measurements and must not infer them from these microbenchmarks.
- The 1024-byte limit rests on a scalar-aligned prefix model, not on implemented unit recognition, streaming admission or discard recovery; T0002 owns proving the real policy keeps the approved semantics.
- Values describe the measured machine class. Running the recorded battery on materially different hardware is the calibration, not assume-and-ship.
