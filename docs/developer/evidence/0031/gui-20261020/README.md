# Computer Use native observations — 2026-10-20

**Result:** native GUI measurements add useful evidence and expose a responsiveness problem, but do **not** complete [T0001](../../../../../.grimoire/ticket/0019-bounded-history-and-performance-budgets/T0001-measurement-and-calibration.md) or select shipping numbers. This supplements the [earlier microbenchmark evidence](../README.md). No production fix or history configuration was introduced. Measurement was explicitly requested through Computer Use.

## Setup and boundaries

- Built `cargo build --release` successfully from base `8a92562191cecd1d358dc4c774e31242c78c04e3` plus the previously recorded dirty tree. Rust changes remain test-only; this GUI run adds only a synthetic child fixture and evidence. Default application features, no DHAT, no tracing override, no optimization change.
- Copied the release executable and bundled ConPTY into owned `target/gui-calibration-20261020/`, with adjacent `harbor.portable` and controlled `config.toml`. Config: requested Consolas 16, `python.exe -u D:/Code/harbor/scripts/profiling/gui_probe.py`, working directory `D:/Code/harbor`, child-only `PYTHONUTF8=1`, OSC52 deny; other settings omitted/default. Current shipping history remains 1000 physical rows, with no configurable byte bound/protection. Normal effects remain enabled; no effects-off comparison.
- [Native provenance](native-environment.json) identifies binary/config/fixture hashes, loaded ConPTY **1.24.2607.10001**, Python **3.12.10**, window DPI **144**, Windows/machine and loaded DXGI/D3D12 modules. Actual adapter/backend and primary/fallback font files remain unmeasured; the earlier prepare example's Vulkan metadata must not be reused for this host.
- Computer Use launched PID **4468**, window **1903830**. Requested desktop frame 1100x700 at (100,100), screenshot client 1098x698. The child's `grid` command observed **107 columns × 32 rows** at size 16; this differs from the protocol's 80x24 reference and is an explicit applicability limit, not a silent equivalent. The native title bar is outside the terminal grid.
- Synthetic child source: [`gui_probe.py`](../../../../../scripts/profiling/gui_probe.py), using [`workload.py`](../../../../../scripts/profiling/workload.py). It executes no arbitrary shell commands or profiles. Latin/colored/Unicode bursts each emit 20000 numbered rows; hashes/bytes match the [workload descriptions](../workloads.json). Each row is flushed, so producer measurements include Python generation/encoding, ConPTY writes and backpressure. They are **not** parser-only throughput, correct-present throughput or input latency. Normal cursor blink is disabled by the child (`?25l`), so idle figures are cursor-hidden, not the shipping interactive-shell baseline.
- Portable mode isolates configuration, **not release logs**. No user configuration was changed; no home logs, arbitrary environment values, shell payloads or other applications' screenshots were read/shared. The final tab was closed through GUI normally, and the owned test window disappeared; no unrelated process was terminated.

## Successful single-session bursts

The source's `emit()` duration is printed on screen as `producer_ms`. A final `019999`, complete DONE marker and READY were visually observed for all nine bursts. This checks the visible tail and completion, **not every retained row, copy/search source or presentation latency**. Runs were repeated in one process, not three cold starts; Unicode followed Latin/color, so only the first Unicode run is first-use for this process's Unicode path.

| Mode / source bytes per burst | Run 1 producer seconds | Run 2 | Run 3 | Median |
| --- | --- | --- | --- | --- |
| Latin / 1620000 | 3.800192 | 3.262955 | 3.228219 | 3.262955 |
| Colored / 2051378 | 4.031646 | 3.870950 | 3.862087 | 3.870950 |
| Unicode / 820000 | 50.462832 | 45.994225 | 46.040426 | 46.040426 |

Visual evidence: [Latin 1](one-latin-1.png), [2](one-latin-2.png), [3](one-latin-3.png); [colored 1](one-colored-1.png), [2](one-colored-2.png), [3](one-colored-3.png); [Unicode 1 after recovery](one-unicode-1-recovered.png), [2](one-unicode-2.png), [3](one-unicode-3.png).

**Confirmed responsiveness observation:** during the first Unicode burst, repeated window captures failed with a four-second capture timeout and UIA discovery timeout. Independent Windows `Get-Process` observations reported `Responding=false`. Over a subsequent 10.10-second sample interval Harbor consumed **10.078125 CPU seconds** ([raw](unicode-unresponsive.csv)), approximately one busy logical processor (4.99% of this 20-processor machine). It later recovered and displayed the final marker; no crash or permanent deadlock was observed. Warm Unicode repetitions remained slow, so a cold-only fallback initialization explanation is insufficient by itself. Raw CPU samples for warm rounds show **55.1875** and **55.65625** CPU seconds over approximately 65-second observation windows.

This is a demonstrated long responsiveness interruption on the measured workload, **not a proven regression, root cause or violation of an already calibrated numeric threshold**. The source of CPU cost was subsequently located without native tracing: [the ingest path rebuilds the full retained-content projection once per accepted feed and twice per VS16 width promotion](../unicode-diagnosis/README.md). The native split between those two paths and rendering is still unmeasured. No speculative performance fix was attempted. Tool automation problems described below are tracked separately from this independently observed unresponsive interval.

## Multi-session, reload and resize observations

- Opened four retained sessions, one active and three hidden, IDs 1/3/4/5. [Four-session screenshot](05-four-sessions.png). Tab 1 retained the Unicode output; other sessions used separate synthetic Python children. [Initial four-idle samples](four-idle.csv) were captured before a later invalid fifth-session excursion.
- **Invalid attempt:** the planned all-four concurrent `sustain` pass did not reliably submit commands. Background modifier dispatch produced literal letters/no-ops, repeated foreground text/Return delivery was inconsistent, and an unintended fifth tab appeared. `four-sustain.csv`, `sustain-start-tab-*.png` and `sustain-done-tab-*.png` are retained as **invalid workload/topology evidence**, not successful concurrent-output measurements. The misleading `done` filenames indicate capture intent only: their images show READY/old output, not completion. The extra tab was subsequently closed. [Action records](actions.json) explicitly label these attempted submissions invalid.
- One later **active-tab-only** `sustain` run was actually submitted and completed in tab 5 with three hidden retained sessions: colored 6000 rows, 615360 bytes, requested 5 ms row delay, **32.439744 producer seconds**. [Final marker](sustain-tab5-done.png). The [35-second process capture](sustain-tab5-verified-attempt.csv) began after submission/failed capture attempts, so it is a partial active-output-plus-settling window, not a full output peak capture. Sustained output in all hidden sessions remains unverified.
- Changed only the owned portable configuration font 16 → 20 → 16 through the real watcher. [Font 20](07-font20-four.png), [restored 16](08-font16-restored.png), [tab 1's retained Unicode after reload/resize](09-retained-unicode-tab1.png). Active content changed scale and the same child PID/content remained observable. This is a limited success observation, not full reload transaction/failure or all-hidden-font/consumer acceptance.
- At font 20, executed **three cycles / nine geometry changes**: 1500x900 → 900x600 → 1100x700 at (100,100). The Computer Use frame setter independently confirmed each requested geometry; final snapshots: [cycle 1](resize-cycle-1.png), [2](resize-cycle-2.png), [3](resize-cycle-3.png). [Requests/readbacks](resize.json) record UTC and tool outcomes. Visible terminal rendering/READY remained available after each cycle. These are desktop window dimensions, not exact 80x24/160x40 grids. API request/readback durations are **not resize-to-correct-present latency**. Font-size-20 narrow geometry visibly reflowed/clipped portions under the existing policy; full content/anchor correctness was not audited.

No fixed exact input-to-present or resize latency is reported. Computer Use launch/RPC duration includes transport, focus, accessibility and screenshot overhead; neither that nor the first successful later screenshot isolates display timing.

## Process residency, idle CPU and retirement

Sampler: PowerShell **7.6.6**, `sample_process.ps1`, exact application PID 4468, nominal **250 ms** interval. Actual samples are approximately 260–270 ms apart due to loop overhead; UTC and actual elapsed values are retained. First CPU interval is unknown; CPU means below use cumulative CPU delta divided by actual wall span and **20 logical processors**. Private bytes measure committed process-private memory, **not physical RAM, live heap or GPU residency**. Working set is separately retained. All peaks are sampled, not instantaneous maxima.

[Derived process summary](process-summary.json) contains counts, UTC ranges, min/max/end private bytes and working set, weighted CPU means, and nearest retirement samples. All raw CSVs are alongside it: `one-idle*.csv`, `one-{latin,colored,unicode}-{1,2,3}.csv`, `unicode-{unresponsive,recovered}.csv`, `four-{idle,font20,resize-font20,preclose}.csv`, `sustain-tab5-verified-attempt.csv`, `retirement.csv`. The invalid `four-sustain.csv` is excluded from conclusions.

| Phase | Samples / observed span | Private committed memory | Average CPU, whole machine |
| --- | --- | --- | --- |
| One session, confirmed idle, before bursts | 39 / 10.05 s | 327.48 MiB, constant during window | 0.0078% |
| One session, after warm Unicode | End of warm round 3 | 347.70 MiB | Not an idle-only CPU window |
| Four retained sessions, initial idle | 39 / 10.10 s | 441.84 MiB, constant during window | 0.0232% |
| Four sessions, font 16 → 20 | 39 / 10.04 s | Sampled peak 679.13 MiB; end 553.87 MiB | 0.0622% |
| Four sessions, after restore to 16 / before closing | 39 / 10.08 s | Sampled peak 754.41 MiB; end 610.14 MiB | 0.0078% |
| Retirement through 140 s | 532 / 140.24 s | Sampled range 351.30–600.30 MiB; end 351.30 MiB | 0.0067% |

Closed active tabs three times through native Ctrl+W, leaving original tab 1. Immediate screenshots can show stale/transitional tab rows; [final 140-second screenshot](retirement-140s.png) confirms one retained session. [Close request timestamps](retirement-actions.json) are automation request/return boundaries, not exact model-commit timestamps. The following samples are nearest to the **last close request's return** (errors 102/30/37 ms respectively), not proof of instantaneous resource retirement at close:

| Offset | Process private bytes, MiB | Working set, MiB |
| --- | --- | --- |
| Approximately 0 s | 580.59 | 340.06 |
| Approximately 30 s | 356.14 | 244.06 |
| Approximately 120 s | 351.30 | 239.83 |

There is substantial post-close process-resource reduction; immediate tab close is not the settled endpoint. Remaining residency is above initial idle after Unicode/font/resize warming. One cycle cannot establish a leak, bounded long-term caches or leak freedom; do not divide these totals into a claimed per-tab budget or infer GPU retirement from process-memory reduction.

## Reproduction and artifact status

1. Build release and create a fresh owned portable directory with the executable, matching `conpty/`, marker and the setup configuration above. Do not reuse/change home configuration. Use the hashed `gui_probe.py`/`workload.py` source.
2. Launch through Computer Use, inspect the exact PID/window, set frame 1100x700 and verify `grid` output. The original preparation ran on PID 4468/window 1903830; discover new IDs on repetition. Capture only the owned window. Use a separate observer shell for explicit-PID CSV sampling.
3. Submit Latin three times, colored three times and Unicode three times, confirming actual command execution and DONE/final row each time. Start 10-second sampler windows before Latin/colored; allow at least 65 seconds for Unicode (first-run 10-second window here was incomplete, followed by a separate unresponsive capture/recovery screenshot). Preserve unfavorable outcomes and timeouts.
4. Create the A/B topology using GUI. Validate actual children/command markers rather than trusting a successful input-tool response. Do not count failed background/foreground delivery as product latency or successful output. Repeat the validated active-only sustain/reload/resize/retirement steps as above; repair and verify the all-hidden workload automation before claiming its coverage.
5. Record process/capture UTC/monotonic boundaries and matching config/binary hashes. Summarize CPU from cumulative first/last deltas; for CSV parsing, the first CPU field is an **unquoted empty value**. The shared summary was checked to avoid interpreting it as zero or shifting subsequent columns.

The first long automation script aborted when Unicode capture timed out; precise key-dispatch timestamps for the first six bursts and first Unicode run were not durably saved. Their raw CSV UTC windows, final screenshots, source/inputs and producer durations remain retrievable. `actions.json` contains later Unicode timing and explicitly invalid concurrent attempts; it is not a complete nine-run event trace. A mistaken draft CSV parser was corrected before reporting; the final summary contains finite statistics and aligns with raw data. Preparation's initial tool launch wait is not a cold-start metric. No ETL, DHAT, GPU or high-speed display capture was recorded.

## Acceptance disposition

- **Observed success, limited scope:** nine completed output tails; Latin/color/Unicode visual controls; four retained sessions and tab switching; font watcher changes; nine verified desktop geometry changes; normal close and post-retirement process sampling.
- **Issue requiring investigation:** Unicode output's repeated long producer duration and first-run independently confirmed unresponsiveness. Root cause and a calibrated pass/fail budget remain unresolved.
- **Invalid/unverified:** concurrent output in all hidden tabs, some attempted synthetic command delivery, precise input/output/resize-to-present latency, every retained row/copy/search/source policy, alternate-screen matrix, effects-off comparison, cold-start phases, native/font allocation attribution, GPU activity/residency/retirement, frame/upload counts and DHAT global heap peak.
- **Still blocked:** all default/max history capacities, fixed protection and numeric acceptance budgets remain unselected. This native baseline meaningfully extends the earlier subset but is not the complete R5 matrix or T0001's usable numerical contract.

Checks: release build successful; four workload unit tests passed; synthetic child compiled and exercised; docs validation and diff checks recorded after this supplement. Prior Rust fmt/Clippy/workspace evidence remains applicable because no Rust production or test source changed during this GUI pass. No benchmark-related optimization or shipping fix was performed.
