# T0001 measurement evidence — partial, numeric gates blocked

**Source:** [T0001](../../../../.grimoire/ticket/0019-bounded-history-and-performance-budgets/T0001-measurement-and-calibration.md), [Spec 0031 R4–R6](../../../../.grimoire/spec/0031-bounded-history-and-performance-budgets.md), ADRs [0072](../../../../.grimoire/adr/0072-dual-bounded-terminal-history.md)/[0073](../../../../.grimoire/adr/0073-session-history-accounting-and-text-unit-overflow.md). **Capture date:** 2026-10-20. **Result:** partial tooling/probe delivery; T0001 is **not complete**. No shipping numeric choices or current-scope performance acceptance are established. T0002/T0003/T0004 numeric gates remain **BLOCKED**. No production protection or configuration was implemented. One production optimization was later made against the measured bottleneck — [ADR 0074](../../../../.grimoire/adr/0074-demand-driven-content-anchor-tracking.md) stage 1, verified in [fix-0074-ingest.md](unicode-diagnosis/fix-0074-ingest.md); it supplies no numeric gate.

## Unicode responsiveness: cause located

The later [Unicode diagnosis](unicode-diagnosis/README.md) locates the burst's cost in the ingest path: a full retained-content projection rebuild per accepted PTY feed, and two per width-promotion event. VS16 promoting a width-1 glyph to width 2 makes the measured workload pay that on every row (pure-model median 3.997 s versus 0.041 s with the promotion removed; 91–92 % of the time inside the rebuild). Stage 1 of the fix is applied later in [fix-0074-ingest.md](unicode-diagnosis/fix-0074-ingest.md). This supersedes the "root cause unconfirmed" wording below; the GUI timings themselves remain valid observations but their producer-side attribution is a recorded negative result.

## Calibration capture (2026-10-21, post-fix build)

The [deterministic calibration capture](calibration/README.md) repeats the candidate battery on the ADR 0074
stage-1 build and supplies retained-history storage (1000/4000/16000 rows at 80/160 columns, with construct and
rectangular-resize distributions), bounded text-unit growth through 28 KB, and OSC/prepare microbenchmark
medians across three processes with raw stdout preserved. It derives a recommended numeric contract — default
4000 / maximum 16000 history rows, default 4 MiB / maximum 16 MiB retained content bytes, a 1024-byte per-unit
protective limit and microbenchmark regression bounds — and the owner adopted that contract on 2026-10-21 ([ADR 0075](../../../../.grimoire/adr/0075-calibrated-history-and-text-unit-limits.md)); nothing here changes shipping behavior yet.

Native output/presentation, resize latency, idle CPU/GPU, process/GPU residency and the effects/lifecycle
host-matrix rows remain unsupplied, so T0004's numerical integrated acceptance stays blocked independently of
that value decision.

## Native GUI supplement

The subsequent [Computer Use observations](gui-20261020/README.md) add real release/ConPTY output, limited active/hidden-session, font reload/resize and 0/30/120-second process-retirement evidence. They also record a long Unicode responsiveness interruption and invalid concurrent-output automation. Shipping numeric gates remain blocked. Sections below describe the **original microbenchmark pass**, not the latest GUI coverage; consult the supplement before treating an original NOT RUN entry as the complete current evidence ledger.

## Provenance and retrieval

Base revision `8a92562191cecd1d358dc4c774e31242c78c04e3`, initially clean. Measured dirty Rust scope: test-only `normal_buf.rs` module declaration plus new `history_calibration.rs`. No production code path differs. Workload/sampler scripts and documentation were added in this run but are not used inside the measured parser/prepare boundaries. The persisted local plan is `.grimoire/plans/0040-history-measurement-calibration.md` (repository-ignored; this report is the durable shared handoff).

- [Source/tool versions, source hashes and dirty Rust diff](source.txt). Git blob hashes list `Cargo.lock`, `crates/harbor-terminal/src/history_calibration.rs`, `crates/harbor-terminal/src/normal_buf.rs`, `crates/harbor-terminal/examples/monospace_prepare.rs`, `crates/harbor-terminal/src/parser/osc_benchmark.rs`, `scripts/profiling/workload.py`, `scripts/profiling/sample_process.ps1`, in that order. Later edits only changed documentation and Python test formatting; measured source hashes remain applicable.
- [Allowlisted Windows/machine/tool environment](environment.json): Windows 11 build 26300 (`10.0.26300`), i5-13600KF, 14 cores/20 logical processors, 34183016448 bytes RAM. AMD RX 7900 XT Windows driver `32.0.31041.1004`; virtual display adapter also installed. No machine name, environment values or home configuration were collected.
- Rust `1.97.1`, Cargo `1.97.1`, host `x86_64-pc-windows-msvc`, Python `3.12.10`, PowerShell `7.6.6`. Release uses opt-level 3, LTO, one codegen unit; terminal default features (`renderer`), no DHAT or tracing override. Cargo lock/source hashes identify dependency versions (wgpu 30.0.0, terminal 0.1.0).
- [Run manifest](runs.json): exact commands, process start/end UTC and zero exit codes for three serial sets of candidate → OSC → prepare. Compilation was completed before these captures; no concurrent builds/tests were launched during them. Other machine activity was not controlled; no reboot/cache-flush or power-mode claim.
- Candidate raw samples/model output: [run 1](candidates-1.txt), [run 2](candidates-2.txt), [run 3](candidates-3.txt). [Derived summaries](candidate-summary.json) preserve scenario keys, median/min/max/nearest-rank p95 across 21 observations (7 per process).
- OSC raw duration vectors: [run 1](osc-1.txt), [run 2](osc-2.txt), [run 3](osc-3.txt).
- Prepare reports: [run 1](prepare-1.txt), [run 2](prepare-2.txt), [run 3](prepare-3.txt). These contain reported median/p95, not individual prepare samples.
- [Synthetic workload descriptions and SHA-256](workloads.json): 20000 rows, columns=80, no delay; Latin 1620000 bytes, colored 2051378, Unicode 820000, dirty 1732495. This pass generated descriptions only; Latin/color/Unicode were subsequently executed through Harbor/ConPTY in the [GUI supplement](gui-20261020/README.md). Dirty mode remains unexecuted there.
- [Verification record](checks.json) lists final gate commands and outcomes.

Original microbenchmark artifacts reside beside this report and contain synthetic probe output and allowlisted provenance. The [GUI supplement](gui-20261020/README.md) adds owned-window screenshots and explicit-PID CPU/private-byte CSVs in its subdirectory. No ETL, DHAT, home host logs, real shell payloads or GPU-residency captures are claimed. Future native captures need the confidentiality/storage controls in the [protocol](../../history-calibration.md#evidence-and-confidentiality).

## Repeat the measured subset

Use the exact commands/build/inputs, warmups and sampling boundaries in the [protocol](../../history-calibration.md#test-only-candidates-and-existing-microbenchmarks). Run each set three times serially after building, preserve outputs and metadata, and compare hashes/configuration before reusing this evidence.

Candidate grid: 24 visible rows, 80/160 columns, one blank primary ring, history candidates 1000/4000/16000. No windows, hidden tabs, PTY, fonts, DPI or GPU are involved in those storage observations. Unicode probe: one headless 80x2 engine per sample, primary and alternate, chunks 1/7/4096 bytes, seven fresh-engine samples each. Construction and alternate entry occur outside feed timing; assertions and `Z` recovery check occur afterward. All first samples are retained; allocator/process warming is uncontrolled, not a cold/warm host comparison.

OSC: headless 80x4 parser/screen; exact synthetic sequence is in the hashed probe source; 113664 input bytes × 30 rounds = 3409920 bytes/sample. One warmup then nine sorted samples per process. No shell/application host.

Prepare: RX 7900 XT, **Vulkan** (AMD driver info `26.8.1 (LLPC)`), requested Consolas 16, cell 9x19/ascent 15, 96 DPI, 80x24, surface bookkeeping 800x600, one engine/pipeline, repeating Latin `abcdefghijklmnopqrst` input. Actual selected font files/version were not captured (request/metrics only). Initial prepare precedes the clean/one-cell/full modes; each mode discards 200 warmups, then reports 2000 samples. Timing wraps `pipeline.prepare` only; queue submit/device wait occur outside that interval. No window, draw, PTY, readback or display-present timing; no effects or hidden-tab comparison. Hardcoded texture/vertex bytes are not residency observations.

ConPTY was not exercised by any measured subset. `conhost.exe` file version `10.0.26100.8875` is recorded as environment information, **not proof of a loaded ConPTY runtime version**. Host application/shell versions, display refresh/HDR, actual UI fonts and active/hidden-session configuration are NOT RUN, not assumed defaults.

## Observed results (not shipping thresholds)

### Candidate ring components

Expected: preallocated arrays grow with history rows × columns even for empty source. Observed: all assertions passed; on this ABI `Cell=56`, provenance=1, row metadata=40 and row version=8 bytes. This explains why a source-byte cap cannot be a process-memory budget; it does not establish a bottleneck for optimization.

| History rows | Columns | Four owned backing arrays, bytes | Modeled old+new arrays, bytes | Construction median ms | Rectangular resize median ms (min–max) |
| --- | --- | --- | --- | --- | --- |
| 1000 | 80 | 4718592 | 9523542 | 0.6760 | 1.0867 (0.9523–1.3679) |
| 4000 | 80 | 18542592 | 37342542 | 2.7497 | 4.2618 (3.8233–5.4079) |
| 16000 | 80 | 73838592 | 148618542 | 11.4916 | 16.3327 (15.4906–18.0836) |
| 1000 | 160 | 9388032 | 18889782 | 1.4475 | 2.3216 (2.0921–3.0252) |
| 4000 | 160 | 36892032 | 74068782 | 5.9238 | 8.8003 (8.2512–11.1750) |
| 16000 | 160 | 146908032 | 294784782 | 21.9844 | 34.1141 (30.1042–37.4998) |

These are 21 samples/combination. The overlap column is arithmetic on old/new capacities, not an instantaneous heap/private-byte peak. Blank storage excludes suffix heaps, damage/owner storage, allocator overhead, live alternate/saved buffers beyond this single ring, primary reflow preparation, native/fonts and GPU. Four-session costs were not measured; multiplying by four would remain a component model, not active/hidden residency.

The byte-candidate arithmetic includes a case where bytes dominate: 16000 rows × 160 columns × 3 source bytes/column would retain 2184 rows under 1 MiB, 8738 under 4 MiB, all 16000 under 16 MiB. No real dual eviction is executed. No candidate is selected from this calculation.

### Bounded text-unit growth

Expected on the **current unprotected baseline**: controls and finite exceptional sequences retain complete original source under all chunkings/modes, followed by an independent unit. Observed: all 90 combinations passed per process, seven samples each (1890 feed observations total). Controls are 3/6/11 source bytes. Largest growth inputs are 8193 combining, 12291 selector and 28676 ZWJ bytes, with suffix capacities 8192/16384/32768 bytes respectively. No display/font assertion is made.

For the largest ZWJ input on primary, median feed time is 15.9389 ms with 1-byte chunks (min–max 15.6956–16.2933) versus 0.9931 ms with 4096-byte chunks (0.9811–1.0758), 21 samples each. This is parser+engine feed cost, not input latency or PTY throughput. Full distributions and alternate cases remain in the raw vectors/derived summaries.

Every modeled 64/256/1024-byte limit retains the three ordinary controls. The 28676-byte ZWJ model admits 63/256/1022 bytes respectively. Scalar alignment can leave a prefix ending with ZWJ; the model inserts no synthetic text. It does **not** prove streaming discard/recovery or consumer agreement. A three-control corpus plus finite growth curves is insufficient to select a legitimate general Unicode safety boundary, so a fixed protective value remains unsupplied.

### Existing parser and prepare microbenchmarks

| Boundary | Run 1 | Run 2 | Run 3 |
| --- | --- | --- | --- |
| OSC median MiB/s | 100.49 | 101.69 | 98.89 |
| Prepare clean median / p95 ms | 0.0047 / 0.0049 | 0.0047 / 0.0047 | 0.0047 / 0.0051 |
| Prepare one-cell median / p95 ms | 1.6801 / 2.0123 | 1.6498 / 1.9658 | 1.6892 / 2.0322 |
| Prepare full median / p95 ms | 1.6256 / 1.9770 | 1.6266 / 1.9555 | 1.6274 / 1.9797 |

OSC duration range across the 27 post-warmup samples is 0.0308983–0.0337724 s/sample. Prepare's three-run variation is visible above; raw per-iteration durations are unavailable from the existing example. No measured value is turned into an acceptance threshold. Native sequence caches remain zero in this Latin-only prepare scenario; this is not an emoji/fallback or residency result.

## R5 coverage ledger

This table records the **original microbenchmark pass's** host/native gaps. Both host topologies were unexecuted at that point; the [subsequent GUI supplement](gui-20261020/README.md) supplies partial real-session observations and unresolved issues, not complete R5 acceptance.

| Scenario | One visible | One active + three hidden | Missing coverage |
| --- | --- | --- | --- |
| Cold/idle | NOT RUN | NOT RUN | Host first present, warm idle CPU/GPU, native/private-byte time series |
| Large history | NOT RUN | NOT RUN | Candidate probe only; no comparable host output/presentation or residency capture |
| Sustained/colored/dirty output | NOT RUN | NOT RUN | Synthetic generator available; no final-correct-present boundary or host frame/upload counters |
| CJK/emoji, cold/warm fallback | NOT RUN | NOT RUN | Parser controls only; no font/native/GPU observation |
| Terminal-font reload, saved primary | NOT RUN | NOT RUN | No old/new overlap, resize/present latency or settled resources |
| Resize | NOT RUN | NOT RUN | Rectangular blank-ring probe is not reflow/ConPTY/window latency |
| Effects on/off | BLOCKED | BLOCKED | Controlled effects pair and GPU capture path not supplied; opacity is not effects-off |
| Open/switch/close/retirement | NOT RUN | NOT RUN | No 0/30/120-second retirement series or hidden-session residency |
| Simultaneously visible panes | BLOCKED/deferred | BLOCKED/deferred | [T0005/#158](../../../../.grimoire/ticket/0019-bounded-history-and-performance-budgets/T0005-deferred-visible-pane-acceptance.md), not a substitute for active/hidden acceptance |

| Required metric | Current evidence | Gate |
| --- | --- | --- |
| Output throughput | OSC parser only; synthetic producer byte hashes only | BLOCKED for end-to-end |
| Input-to-present | None; correlated host/presentation capture still required | BLOCKED |
| Resize latency | Blank-ring rectangular microprobe only | BLOCKED for native/primary reflow |
| Idle CPU/GPU | Explicit-PID CPU sampler smoke-tested on PowerShell, not Harbor; no GPU data | NOT RUN |
| Frame/upload work | Prepare-only timing and static allocation message; no host frame/upload series | BLOCKED |
| Process/private bytes, peak/settled | Sampler available; no Harbor capture | NOT RUN |
| Heap churn/count/global live peak | No DHAT captures | NOT RUN |
| Native/font and atlas/GPU residency | No residency/retirement captures; prepare counters/texture sizes insufficient | BLOCKED |

WPR and WPA commands are discoverable, PresentMon is not. WPR status/profile inventory was queried (no recording active; GPU/DesktopComposition/ResidentSet profiles available; see `wpr-tools.txt`); actual WPR/WPA captures were not configured or executed; their presence alone is not a validated latency/GPU capture path. No profiling tools were installed and no user settings were changed. Missing input/present correlation, controlled effects comparison and residency capture implementation are remaining **T0001 work**, not newly imposed user decisions or a claim that automation is impossible.

## Required numerical outcome ledger

| Choice required by R4 | Candidate evidence | Selected value / downstream effect |
| --- | --- | --- |
| Default physical rows | 1000/4000/16000 blank-ring observations | **UNSUPPLIED**; T0003 blocked. Existing 1000 is a baseline fact only. |
| Default history UTF-8 bytes | 1/4/16 MiB arithmetic comparisons | **UNSUPPLIED**; T0003 blocked. No host/source/residency workload basis yet. |
| Maximum accepted rows | Same limited grid observations | **UNSUPPLIED**; T0003 blocked. Larger grids, multiple sessions and temporary/native costs uncalibrated. |
| Maximum accepted content bytes | Source model only | **UNSUPPLIED**; T0003 blocked. Cannot infer suffix/allocator/process/GPU cost. |
| Fixed unit protection bytes | 64/256/1024 prefix models, controls and bounded current growth | **UNSUPPLIED**; T0002 acceptance blocked. No broad ordinary-unit applicability or measured protected-path behavior. |
| Performance/resource thresholds | Parser/prepare/blank-ring subset only | **UNSUPPLIED**; T0004 numerical acceptance blocked across R5. |

**Rationale for not selecting:** the required comparable R5 metrics and intended-workload applicability are missing. Selecting one candidate merely because it fits this machine or these controls, or using observed maxima as passing budgets, would violate R4. This report is not the promised usable numerical contract and does not unblock dependent shipping work.

## Acceptance assessment and remaining work

1. **Partial match:** measured subset has repeatable commands, source/build/machine/tool metadata, raw candidate/OSC samples and prepare reports. Actual font-file and host/ConPTY provenance remain gaps.
2. **Gap:** complete R5 comparable baseline procedures/captures are not supplied. A phase/workload matrix and CPU/private-byte sampler exist; correlated end-to-end timing, upload/effects and GPU capture paths still require implementation/validation and execution.
3. **Partial match:** repeated observations expose variation without mislabeling DHAT/site maxima; cold/warm host and peak/settled residency distinctions are not measured.
4. **Gap:** all required numerical selections remain unsupplied. T0001 cannot be marked done.
5. **Partial match:** ordinary combining/VS/ZWJ and exceptional probes cover primary/alternate and UTF-8 fragmentation. Prefix modeling is not final text admission or copy/search/render/reflow protection evidence.
6. **Gap:** downstream numeric gates remain visibly incomplete, not delegated away as successful calibration.
7. **Partial match:** safe retrievable subset and standard gates are recorded. Missing capture types/scenarios are explicit.

Next work remains within T0001: implement/validate the missing bounded observation paths and controlled effects pair; capture the protocol's A/B host matrix with separate release, DHAT, private-byte and GPU runs; establish ordinary-unit/workload applicability; then select and justify each numeric choice and threshold with its supported environments. Preserve this baseline for comparable reruns. Do not begin numeric-dependent shipping work based on this partial package.
