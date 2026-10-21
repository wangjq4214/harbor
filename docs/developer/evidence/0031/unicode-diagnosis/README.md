# Unicode responsiveness: located cause

**Question:** why does the measured 20,000-row Unicode burst take ~46 s when Latin/colored take ~3–4 s, and why did the first run report `Responding=false`?

**Answer:** the terminal's ingest path performs a **full retained-content projection rebuild** of the entire scrollback, once per accepted PTY feed and twice per width-promotion event. A width-1 glyph followed by VS16 (`U+FE0F`) is promoted to width 2, so the measured workload triggers one promotion on every row. That work scales with retained history, not with the incoming bytes, so a burst that mostly escapes it runs ~100× faster.

This is a **located cause with a measured mechanism**, not a regression against a calibrated budget. Stage 1 of the fix is now applied under [ADR 0074](../../../../../.grimoire/adr/0074-demand-driven-content-anchor-tracking.md); every number below is pre-fix and superseded by [`fix-0074-ingest.md`](fix-0074-ingest.md).

## Reproduction and evidence chain

Base `8a92562191cecd1d358dc4c774e31242c78c04e3` plus the previously recorded dirty tree. All numbers are release builds, grid 107×32 (the measured window), default 1000-row history, 2000 rows of input, three samples per case unless noted. Environment and hashes: [`provenance.json`](provenance.json).

### 1. The problem survives without PTY, fonts, GPU or presentation

`crates/harbor-terminal/examples/output_ingest_probe.rs` feeds a file straight into the parser or the headless engine and times nothing but ingestion. Raw files: `parser-<case>-2000.txt` and `engine-<case>-<chunk>.txt` beside this report, plus [`initial-timeouts.txt`](initial-timeouts.txt) (the two over-budget first attempts, kept as partial data).

| Input (2000 rows) | Pure parser + `Screen`, median | Bytes |
| --- | --- | --- |
| `original` (Unicode workload) | **3.997 s** | 82000 |
| `no_vs16` (remove `U+FE0F`) | 0.0413 s | 76000 |
| `wide_vs16` (heart→emoji, keep VS16) | 0.0414 s | 84000 |
| `heart_vs16` (only `❤️` per row) | 1.498 s | 30000 |
| `combining`, `zwj`, `cjk`, `emoji` alone | 0.038–0.039 s | 24000–40000 |

Removing a single scalar — VS16 — changes cost by ~97×. Keeping VS16 but removing only the *width promotion* it causes (heart is width 1, emoji is width 2) restores the fast path. Combining marks, ZWJ sequences, CJK and emoji are each cheap, so generic "Unicode is slow" and generic fallback/emoji-rendering explanations are refuted: **the trigger is promoting an already-printed narrow glyph to width 2**, which is what `U+FE0F` does to `❤`.

### 2. The promoted path rebuilds the whole projection, twice per event

`crates/harbor-terminal/src/projection_diagnostics.rs` (test-only, thread-local, absent from production builds) counts and times every `ContentProjection::build` inside `ContentProjection::build`. Raw output: [`projection-cost-verified.txt`](projection-cost-verified.txt).

| Case | Projection builds | Build time | Total | Share |
| --- | --- | --- | --- | --- |
| `original` | **4000** (2/row) | 3.65–4.16 s | 3.95–4.54 s | **91.6–92.5 %** |
| `no_vs16` | 0 | 0 | 0.041–0.047 s | 0 % |
| `wide_vs16` | 0 | 0 | 0.041–0.042 s | 0 % |

Mechanism in code: `Screen::write_char` → `CellWriter::promotion_base` detects the width promotion → `capture_edit_anchor` calls `self.content_projection()` → `ContentProjection::build` walks **every retained row** and `logical_content::decode`s it; the promotion branch then calls `record_deletion_delta`, which builds it again. No promotion → neither call runs.

### 3. A second, independent full rebuild happens once per accepted PTY feed

The engine path (`Terminal::process_output` → `TerminalIo::feed_parser_bytes`) calls `screen.finish_anchor_mutations(...)` for every parser iteration, and that function always starts with `self.content_projection()?` — even when no selection, saved cursor or review anchor exists. Raw output: [`ingest-reconciliation-cost.txt`](ingest-reconciliation-cost.txt).

| Same `no_vs16` input, engine path | Feeds | Projection builds | Build time | Total |
| --- | --- | --- | --- | --- |
| 4096-byte chunks | 19 | 19 | 0.0317–0.0326 s | 0.080–0.081 s |
| one row per feed | 2000 | 2000 | 1.95–2.29 s | 2.13–2.57 s |

Exactly one build per feed, 89–91 % of the time. A real ConPTY delivers many small reads, so this cost is paid at the reader's cadence — it is invisible to the 4096-byte parser microbenchmark and to the existing `monospace_prepare` example.

### 4. Native GUI run

A second isolated portable fixture (`target/unicode-gui-diagnosis-20261020/`, user configuration untouched) ran 3×2000 rows of `original`/`no_vs16`/`wide_vs16` after a verified 1100×700 window; the child waits on a gate file so the frame is settled before any input. Raw producer timings and hashes: [`gui-producer.json`](gui-producer.json), summary screenshot [`gui-summary.png`](gui-summary.png), process sampling [`gui-process.csv`](gui-process.csv).

Observed producer times (ms): `original` 31.3 / 83.1 / 96.4; `no_vs16` 1121.4 / 2948.5 / 3223.5; `wide_vs16` 34.8 / 31.9 / 35.0.

**These contradict the pure-model result and must be labelled as invalid for attribution.** `no_vs16` is the cheap path in the model yet is the slowest here, while `original` — 4000 projection builds in the model — returns in 31–96 ms. The only consistent explanation is that the child's writes are not synchronous end-to-end: it measures how long `write()`+`flush()` take, and ConPTY/pipe buffering lets it run ahead, so a case inherits the backlog of whatever preceded it. The ordering (fast, slow, fast) follows the sequence of cases, not their cost. Therefore this fixture measures **producer-side buffering behaviour, not terminal ingest**; it is retained as a negative result and is not used to select or refute a cause. The earlier 20,000-row GUI timings in [`../gui-20261020/`](../gui-20261020/README.md) are consistent with the model in *direction* (Unicode ≫ Latin/colored) but their absolute values also include this buffering effect.

### 5. Remaining gap: exact share in the native process

The `Responding=false` + ~1 busy core interval during the first 20,000-row GUI burst is consistent with the located work (CPU-bound, main-thread, no I/O wait), but no ETW/CPU-sample capture was taken, so the *native* split between the promotion path, the per-feed reconciliation and any renderer time remains unmeasured. A WPR CPU capture of the relocated workload is the next discriminating check if native attribution is required for T0001's R5 evidence.

## Fix applied (ADR 0074, stage 1)

Content-anchor bookkeeping is now demand-driven: `Screen` records edit-anchor mutations and both
ingest paths build a `ContentProjection` only when an anchor consumer exists — a pointer selection,
a saved cursor anchor, or the review anchor. `TerminalIo::feed_parser_bytes` registers the pointer's
selection state with the screen before parsing, so `Screen` still never queries the pointer.

Same-session A/B on the identical workload and boundary (`row` cadence, 2000 rows):

| Case | Before | After |
| --- | --- | --- |
| `original` parser + `Screen` | 4.08–4.42 s | 41.1–41.7 ms |
| `original` engine path | 5.75–6.12 s | 41.3–43.1 ms |
| `no_vs16` parser control | 41.1–41.3 ms | 40.7–41.5 ms |
| `no_vs16` engine control | 2.05–2.42 s | 41.1–41.7 ms |

The Unicode workload is now indistinguishable from the projection-free control, and a test guards it
by asserting zero projection builds for untracked ingestion. Least-friendly case unchanged by design:
with a consumer registered, promotion still costs 4000 builds / 3.57–4.19 s, so streaming output
during an active selection or review stays as expensive as before. Raw output, method, tracked-path
measurement, gates and limits: [`fix-0074-ingest.md`](fix-0074-ingest.md).

## What this does and does not establish

- **Established:** ingestion cost is dominated by full-history `ContentProjection` rebuilds; the promotion path costs two rebuilds per promoted row, and the ingest path costs one per accepted feed. Both are triggered by ordinary, in-contract input, and both grow with retained history — the same quantity T0001/T0003 are about to make configurable.
- **Established:** the burst is CPU-bound on the UI thread for tens of seconds, which explains the observed `Responding=false` and the failed window captures.
- **Not established:** a numeric acceptance budget, native call-stack attribution, or that any existing documented behaviour is violated. No threshold was previously calibrated, so this is a located bottleneck awaiting a budget, not a proven regression.
- **Fixed since (stage 1 only):** anchor tracking is now demand-driven, so this workload no longer projects at all; see [`fix-0074-ingest.md`](fix-0074-ingest.md). Stage 2 (a maintained line-scoped projection) is still not attempted: streaming output while a selection or review anchor is active keeps its pre-fix cost and needs its own measured decision.

## Diagnostics added (all non-shipping)

- `crates/harbor-terminal/examples/output_ingest_probe.rs` — parser-only / engine-only ingestion timing with bounded file reads (≤16 MiB) and a selectable boundary, so one case can be sampled without the other's cost.
- `crates/harbor-terminal/src/projection_diagnostics.rs` — `#[cfg(test)]`-only build counter/timer plus the two `#[ignore]`d measurements above.
- `crates/harbor-terminal/src/content_anchor.rs` — one `#[cfg(test)]` timer call at the top of `ContentProjection::build`. This is the only change to an existing source file, and it compiles out of production builds.
- `scripts/profiling/unicode_diagnosis_input.py`, `test_unicode_diagnosis.py` — bounded `original`/control reductions with equality tests proving the controls differ from the workload only in the intended member.
- `scripts/profiling/gui_unicode_diagnosis.py` — gated native producer with hashed inputs; kept with its negative result.

## Verification

`cargo clippy --all-targets --all-features -- -D warnings` clean; `cargo test -p harbor-terminal --lib` 1071 passed / 7 ignored; `python -m unittest discover -s scripts/profiling` 8 passed; `cargo fmt --check` clean. Details: [`verification.json`](verification.json). The ignored diagnostics are manual, not part of any gate. This covered the diagnosis build, which changed no product behaviour. The fix and its own gates are in [`fix-0074-ingest.md`](fix-0074-ingest.md).
