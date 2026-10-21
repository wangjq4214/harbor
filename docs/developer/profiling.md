# Profiling procedures

Use comparable captures to identify a bottleneck, change one ownership path, then repeat the workload. This guide contains procedures, not captured results or optimization priorities. The [architecture map](architecture.md) helps locate allocation and lifecycle owners.

For Spec 0031/T0001, use the [history calibration protocol](history-calibration.md) and [current evidence ledger](evidence/0031/README.md). The package includes repeated candidate/parser/prepare probes and [limited native GUI observations](evidence/0031/gui-20261020/README.md), including a Unicode responsiveness issue. Full host/native coverage and all shipping numeric selections remain incomplete; these observations do not unblock numerical shipping gates.

## Prepare a comparison

Record the commit and dirty-tree changes, build profile/features, machine/OS, GPU/backend, fonts, DPI, viewport/grid, session/window counts, configuration, exact workload and dwell time. Use identical settings before and after. Separate cold startup from warmed-cache runs and repeat runs to expose variability.

Establish functional correctness before interpreting savings, using the checks relevant to the change (for example `cargo fmt --check`, Clippy and focused/workspace tests). Passing checks is not latency, memory or visual evidence.

## Heap allocation capture

From the repository root:

```sh
cargo run --profile dhat --features dhat-heap
```

The Cargo `dhat` profile inherits release optimization and retains symbols/line tables. The feature installs the DHAT allocator/profiler. Run the intended scenario, dwell, then close normally so the profiler writes `dhat-heap.json` in the process working directory. Preserve each capture before another run overwrites it; a crash or forced termination is not a completed capture.

Analyze from the repository root:

```sh
python scripts/profiling/dhat_analyze.py dhat-heap.json
python scripts/profiling/dhat_drill.py dhat-heap.json
python scripts/profiling/dhat_drill.py dhat-heap.json prepare
```

The last command filters owners by substring; choose a name from the current summary, not a historical symbol. The scripts attribute allocations to Harbor frames in each backtrace. Inspect stacks as well as owner totals: a caller label is not necessarily the resource's lifetime owner.

- Cumulative allocated bytes and allocation count measure churn, not retained residency.
- Global live-heap peak is distinct from the sum of each program point's separate maximum. The analyzer labels `max-live sum` as a per-program-point upper bound and `gmax-live sum` as a true-peak upper bound; do not report the former as an instantaneous process peak.
- DHAT covers instrumented heap allocations, not all Windows native allocations, process private bytes or GPU residency.
- Do not use DHAT elapsed time as startup/frame/input latency evidence: allocation stack collection changes timing.

Source: [`Cargo.toml`](../../Cargo.toml), [`main.rs`](../../src/main.rs), [`dhat_analyze.py`](../../scripts/profiling/dhat_analyze.py), [`dhat_drill.py`](../../scripts/profiling/dhat_drill.py).

## Font lifecycle scenario

Enable the info-level `harbor.font.lifecycle` target explicitly; release defaults do not include it. For example, in PowerShell before launching the chosen build:

```powershell
$env:RUST_LOG = 'warn,harbor.font.lifecycle=info'
cargo run --profile dhat --features dhat-heap
```

Tracing writes JSON to `harbor.log`; release-derived builds use the user's `.harbor` directory. Save the log with the matching capture and record the filter. For a controlled Latin-only workload using a primary font that covers its text, inspect:

1. `font_init`, including `source=system` or `source=configured`;
2. `first_present`, emitted after the first successful surface presentation;
3. `steady_state`, after at least five seconds of dwell following first present;
4. whether `first_fallback` occurs. Unexpected fallback requires checking actual shell/UI text and font coverage, not assuming a loader regression.

Use a separate scenario to trigger the first successful non-primary-face fallback. Font markers and host first-present/dwell markers have different owners; repeated font initialization across sessions/reloads is not a single startup sequence. A dwell marker records elapsed time, not proof of constant memory.

Source: [`lifecycle.rs`](../../crates/harbor-text/src/lifecycle.rs), [`telemetry.rs`](../../src/telemetry.rs).

## Workloads and tools

Use separate captures for cold Latin startup/idle; sustained Latin and dirty-range-heavy output; first CJK fallback; sustained CJK/symbols/emoji; configured fonts without CJK coverage; heavy scrollback; resize/DPI changes; and confirmation-window text. Do not mix unrelated scenarios when attribution matters.

Exercise repeated tab open/switch/close and font reload to distinguish temporary overlap from retained resources. Compare local glass effects on/off separately to attribute GPU/compositor cost.

Use a separate uninstrumented release build for first-present, frame and input timing. Use Windows private-byte sampling for process residency, ETW/WPR for scheduling/I/O attribution and GPU tooling for texture/upload/compositor cost. Record sampling intervals and phase boundaries; heap, private bytes and GPU memory are different quantities.

## Record and interpret

For each capture retain: tool/version/mode, start/end conditions, total allocated bytes, allocation count, global live-heap peak, private-byte peak/settled values when measured, lifecycle markers, latency distributions when measured and known deviations. Mark unmeasured fields explicitly rather than deriving them from another metric.

Distinguish bounded process-lifetime caches, temporary churn and sustained growth. Compare lifecycle endpoints after resources have had time to retire, not just at the instant a tab closes. Check that rendered output, incremental/full-upload correctness and idle behavior remain unchanged. Re-profile after every optimization and reassess the dominant owner before choosing another; source inspection alone establishes neither cost nor savings.
