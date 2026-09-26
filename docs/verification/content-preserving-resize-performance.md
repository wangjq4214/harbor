# Content-Preserving Resize Performance Evidence

## Revision and Measurement Scope

- Candidate baseline: `05a4c62e34a7fee776704a3a9e74d0a0e9beed78` on `feat/reflow`, plus the T0007 source/test/workload changes listed in [the automation record](content-preserving-resize-automation.md).
- Measurement target: large retained primary history containing colored output, CJK, explicit blank lines, ordinary trailing spaces, and styled blanks during repeated width/height resize.
- This delivery defines no pass threshold and makes no optimization claim.

## Fixed Scenario to Execute

Record all fields before interpreting results:

| Field | Required value | Recorded value |
| --- | --- | --- |
| Exact revision and dirty-tree scope | Commit plus all built modified/untracked paths | Not recorded for an executed capture |
| Machine / CPU / RAM | Exact model and installed memory | Not recorded |
| OS | Windows build | `10.0.26200.9457` available; no capture performed |
| GPU backend / adapter | Backend and adapter | Not recorded |
| Font family / size | Exact configured values | Not recorded |
| Display scale | Exact percentage/factor | Not recorded |
| Initial and target geometry | Rows/columns for every step | Not recorded |
| History workload | Fixture arguments and resulting retained rows | Default proposed: 1,100 lines, width 96, seed 152170 |
| Timing build | Release profile/features and tracing filter | Not executed |
| Memory build | `dhat` profile with `dhat-heap` | Not executed |
| Warm-up / samples / dwell | Exact counts and duration | Not recorded |

### Latency procedure

Use a release build with debug tracing enabled for the terminal target. After filling history with `scripts/resize_reflow_workload.ps1`, warm up, then repeat one declared geometry sequence for a fixed sample count. Preserve raw `terminal resize transaction` events and report prepare, PTY, commit, and total microseconds with sample count, median, p95, and maximum.

The trace event is emitted at the existing prepare → PTY → commit boundary and includes source/target geometry, separate active and saved-primary retained-history counts, stage outcome, and failed stage when applicable.

### Memory/cost procedure

In a separate run, use the same fixture, geometry sequence, font, scale, and dwell:

```bash
cargo run --profile dhat --features dhat-heap
python scripts/dhat_analyze.py dhat-heap.json
python scripts/dhat_drill.py dhat-heap.json
```

Archive or link the raw `dhat-heap.json`, record its hash, and report total allocated bytes, allocation count, global live-heap peak, and resize/reflow allocation owners. If Windows private bytes are included, name the tool, sampling point, peak, and steady value.

DHAT-instrumented wall-clock time must not be reported as resize latency.

## Results

| Metric | Result |
| --- | --- |
| Release timing samples | Not captured |
| Prepare median / p95 / max | Not captured |
| PTY median / p95 / max | Not captured |
| Commit median / p95 / max | Not captured |
| Total median / p95 / max | Not captured |
| DHAT total allocated | Not captured |
| DHAT allocation count | Not captured |
| DHAT global live-heap peak | Not captured |
| Resize/reflow allocation owners | Not captured |
| Windows private peak / steady | Not captured |

## Expected and Observed Result

- Expected: comparable release latency and separate allocation/memory observations are recorded for the fixed scenario without inventing a threshold.
- Observed: no Harbor GUI timing session or DHAT large-history capture was executed in this agent run.
- Artifacts: none.
- Exclusions: historical font memory captures do not substitute for this scenario; source inspection and passing tests do not establish runtime cost.
- Outcome: **NOT RUN**.

This missing measurement blocks final T0007 acceptance and prevents any performance-regression, budget, or optimization claim.
