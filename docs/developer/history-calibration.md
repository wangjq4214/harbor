# History calibration protocol

This is optional measurement guidance for [Spec 0031 R4–R6](../../.grimoire/spec/0031-bounded-history-and-performance-budgets.md). The former calibration ticket is retired; the [Spec 0031 result summary](../../.grimoire/spec/0031-bounded-history-and-performance-budgets.md#delivery-and-closure-summary) preserves its results and closure disposition. Procedures are not results. The [profiling guide](profiling.md) explains DHAT and lifecycle attribution.

**Closure:** [Spec 0031's result summary](../../.grimoire/spec/0031-bounded-history-and-performance-budgets.md#delivery-and-closure-summary) retains the calibration rationale, implemented values and limited Windows outcomes. The owner adopted ADR 0075 and waived remaining measurements/acceptance for closure. Raw issue captures were deleted; this protocol remains optional future guidance, not proof of results or an outstanding task.

Resource categories and standard commands are documented in the [testing guide](testing.md). `just test-perf` runs only release diagnostics, not desktop/font acceptance.

## Test-only candidates and existing microbenchmarks

From the repository root, build first, then run serially without competing builds, tests or captures:

```sh
cargo build --release -p harbor-terminal --example monospace_prepare
cargo test --release -p harbor-terminal --lib calibration_accounting_controls
cargo test --release -p harbor-terminal --lib calibration -- --ignored --nocapture --test-threads=1
cargo test --release -p harbor-terminal --lib osc_throughput_probe -- --ignored --nocapture --test-threads=1
./target/release/examples/monospace_prepare.exe
```

Repeat the final three commands in that order in three separate processes each, saving stdout/stderr and command start/end UTC, exit status, revision, dirty scope and source hashes. Do not include Cargo build time in any latency statistic. Use release, default terminal features (`renderer`), no `dhat-heap`, no tracing override. These commands do not run Harbor's application host or ConPTY.

The test-only `normal_buf::calibration` module uses the real private candidate ring constructor without exposing shipping settings. Inputs:

- History rows: 1000 (current baseline), 4000, 16000; visible rows 24; columns 80 and 160. Seven fresh blank-ring construction/rectangular-resize observations per combination per process; resize to 30 rows and one extra column. First observations are retained, not certified cold-boot measurements. No primary reflow or PTY resize timing is claimed.
- Owned backing capacities: cells, cell provenance, row metadata and row versions, separately. Their sum excludes the owner/damage storage, suffix heaps, allocator overhead, native/fonts, GPU and temporary allocations. Old-plus-new capacity is a modeled component sum, not an observed process peak.
- Byte candidates: 1/4/16 MiB, with uniform 1-byte ASCII or 3-byte combining source per column. `min(row_capacity, byte_capacity / row_source_bytes)` is an arithmetic retention model, not production eviction or a memory measurement.
- Ordinary units: base plus combining acute, heart plus VS16, and woman-ZWJ-laptop. Growth inputs append 32/128/512/4096 combining acute, VS16 or ZWJ-woman extensions. Every input is finite (largest 28676 bytes). Seven parser+engine feed samples for each input, primary/alternate mode and byte chunks 1/7/4096. Assertions check complete current source retention and a following independent `Z` at the next cell. Chunk size 1 deliberately cuts UTF-8 scalars.
- Unit candidates: 64/256/1024 source bytes including the leading scalar. A **scalar-aligned prefix model for an already identified unit** reports admitted/discarded bytes. It does not recognize units, enforce streaming protection, test recovery after discards, or prove future consumer semantics. Ordinary controls must remain unchanged. Do not install one of these candidates as a shipping constant without the missing calibration.

Preserve raw sample vectors. For small candidate sets, report median/min/max and nearest-rank p95 (`ceil(0.95*n)-1` in sorted zero-based samples); do not describe a 7-sample p95 as precise tail estimation. Preserve first samples separately when analyzing warming. The OSC probe excludes one warmup and prints nine sorted duration samples, rounds and byte counts. Prepare excludes 200 warmups per mode and reports median/p95 of 2000 samples; it does not currently export individual prepare durations. Its hardcoded R8/vertex-size message describes allocations, not measured residency.

## Synthetic host workloads

The standard-library generator emits binary UTF-8 directly (no Windows newline translation), bounded to 200000 rows and 500 columns. These are synthetic inputs only; it neither reads the environment nor captures terminal content.

```sh
python scripts/profiling/workload.py --mode latin --rows 20000 --columns 80 --describe
python scripts/profiling/workload.py --mode colored --rows 20000 --columns 80 --describe
python scripts/profiling/workload.py --mode unicode --rows 20000 --columns 80 --describe
python scripts/profiling/workload.py --mode dirty --rows 20000 --columns 80 --describe
```

Run the matching command **without `--describe` inside the measured Harbor session**, not the coordinator's terminal. Store its description with the capture. Latin/colored/dirty rows use a six-digit ordinal and 79 visible characters at columns=80 to avoid implicit full-width wrap. Unicode uses a fixed short row with ordinary combining/VS/ZWJ, CJK and emoji controls; `--columns` does not pad or truncate that row. Dirty mode moves through rows 1–24 without scrolling and requires an 80x24 grid. Other modes append CRLF, exercising scrollback. Ordinals permit final-output correctness inspection without storing private shell content.

Use the 20000-row burst for history pressure. For sustained output, use the same command with `--rows 60000 --delay-ms 1` and record actual start/end, not an assumed 60-second duration: scheduler/flush/backpressure change the elapsed time. The generator's exit/elapsed time measures producer completion, **not output-to-present throughput**. A terminal capture still needs an observed final marker/present boundary and confirmation of final ordinal/content. Save separate hashes for delayed workloads (`--describe --delay-ms 1`).

## Windows process residency and CPU

PowerShell 7+, in a separate observer shell, with the PID verified to be the measured Harbor process:

```powershell
$ErrorActionPreference = 'Stop'
$HarborPid = 1234 # replace with the exact measured process, not its shell child
$samples = @(& ./scripts/profiling/sample_process.ps1 -TargetPid $HarborPid `
    -DurationSeconds 60 -IntervalMs 250 -Phase idle-warm)
# Publish only after successful completion; errors invalidate the capture.
$samples | Export-Csv -NoTypeInformation -Encoding utf8 idle-warm.csv
```

Repeat with separate phase labels/files, e.g. `output`, `pre-close`, `post-close-0`, `post-close-30`, `post-close-120`. PID reuse is avoided by holding one process object; start UTC identifies it. Target exit/access failure is an incomplete capture, not zero residency. First CPU delta is null. `cpu_machine_percent` is CPU-time delta divided by observed wall-time delta and logical processor count; multiply by that count for one-core-equivalent percent. Private bytes are Windows process private committed bytes; working set is resident pages, neither is live Rust heap. Max sampled private bytes is only a **sampled** peak; 250 ms polling misses shorter spikes. Sampling CPU/GPU/native process peaks requires the relevant external tool, not arithmetic on grid storage.

Sampler checks (this process, **not Harbor evidence**) and workload checks:

```sh
pwsh -NoProfile -File scripts/tests/test_sample_process.ps1
python -B -m unittest discover -s scripts/tests -v
```

## Required host matrix and phases

Execute every row in two topologies when reusing this protocol for future measurements: **A:** one visible session, no hidden sessions; **B:** four retained sessions, one visible/active and three hidden, with each session receiving the declared output. Repeat each complete scenario at least three times, preserving per-run samples rather than only a favorable pooled statistic. Hidden sessions must remain retained, not minimized extra windows or panes. Pane capability remains separate under [#158](https://github.com/wangjq4214/harbor/issues/158); the former deferred pane-measurement ticket was retired after the [owner-approved waiver](../../.grimoire/spec/0031-bounded-history-and-performance-budgets.md#waived-work-and-retained-limitations).

| Scenario | Input/action and phase boundaries |
| --- | --- |
| Cold/idle | Start a fresh uninstrumented release application; record startup/first present separately. Wait 30 s; sample 60 s warm idle, with cursor blink/focus state recorded. |
| Large history | Emit each 20000-row workload; confirm final ordinal, retained rows/source and correct copy/search results; wait 30 s and sample 60 s settled. Candidate capacities require a labeled fixture build, not nonexistent TOML settings. |
| Sustained / dirty | Run delayed Latin, colored and dirty workloads separately. Delimit active output and final content presentation; then idle 30 s. |
| CJK/emoji | Fresh process, first fallback separately from warmed repeated Unicode output; record actual primary/fallback faces and font files/versions. |
| Terminal font reload | Fixed complete configuration: Consolas 16 → 20 → 16, same requested window size. Record actual grid, acceptance/failure, first correct post-reload frame and 30/120 s settled costs. Repeat while alternate is active with saved primary history. |
| Resize | Alternate a verified 80x24 and 160x40 terminal grid, 30 cycles; record actual physical window size and DPI. Measure request → committed geometry and request → correct present separately; idle between cycles. |
| Session lifecycle | Ten cycles opening to four sessions, output in all, switch each active, close back to one. Observe before close, immediately after, after 30 s and after 120 s; distinguish resource retirement from process-lifetime caches. |
| Effects | Repeat output, idle, resize and lifecycle with effects on/off using an explicit recorded fixture/control. Keep terminal opacity, grid and content identical. No public all-effects-off setting is supplied by this ticket; opacity alone does not disable glass/Acrylic. Until a controlled pair exists, mark this comparison BLOCKED. |

For **every scenario/topology/effects variant**, record output bytes-to-final-correct-present throughput, input-to-present and resize distributions where applicable, idle CPU/GPU, frame count/prepare/draw/upload calls+bytes, process private-byte peak/settled/working-set time series, heap churn/global peak, and atlas/native/GPU resource residency through retirement. Mark nonapplicable metrics explicitly with a reason. Keep equivalent workloads/configuration between baseline and later measurements.

### Measurement paths still required

- A correlated monotonic input/output/resize → host/frame → actual presentation capture. Input-to-present cannot be replaced with parser timing, a UI automation call's duration or a CPU `present()` submission timestamp. Define and validate whether presentation means compositor/display or successful surface submission; label them separately.
- WPR/ETW and GPU observations with tool versions, selected providers/profile, adapter/backend, sampling intervals/events and capture permissions. WPR/WPA availability alone establishes no data. A GPU tool must distinguish upload byte counts/texture allocation from physical residency and show retirement endpoints. Do not infer GPU idle from CPU idle or cache byte counters.
- Separate DHAT scenario captures using the [existing guide](profiling.md), normal exit and matching font lifecycle logs. DHAT time is not release latency. Keep global live peak distinct from allocation-site maxima; native and GPU allocations remain separate.

These were unimplemented/unexecuted coverage gaps at closure. The owner waived further work under T0001/T0004. If this protocol is reused for a future task, validate bounded observation paths before selecting native thresholds; do not choose a largest observed value merely to obtain PASS.

## Evidence and confidentiality

Each run needs revision plus dirty/source hashes, exact build/features/configuration (sanitized controlled fixture, not home settings), machine/Windows/runtime ConPTY and application/tool versions, adapter/backend/driver, font files/faces/versions, DPI, actual viewport/grid, topology/active IDs, phase UTC and monotonic boundaries, dwell, sampling interval/count, commands/input hashes, expected/observed outcomes, distributions/variability and threshold applicability. Record shell executable version, not inherited environment values. Unsupported fields must say NOT RUN/BLOCKED, never default to zero.

Store synthetic text logs and summaries in the evidence directory. ETL/DHAT/logs/screenshots can contain paths, payloads, shell arguments and unrelated application data: inspect/redact before sharing, or retain originals in access-controlled storage and record an immutable locator, SHA-256, access owner and retrieval/retention instructions. Do not claim artifacts exist without a retrievable location. Never enable verbose tracing in release timing captures without declaring it. The issue-specific historical capture directory was removed at owner-approved closure; the retained results are summarized in Spec 0031.
