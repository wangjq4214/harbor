# Performance Profiling Guide

This document defines repeatable performance and memory measurement procedures for Harbor. Historical results live in [`memory-baseline.md`](memory-baseline.md); open work lives in [`optimization-plan.md`](optimization-plan.md).

## Quality Gates

Before interpreting a profile, verify the executable passes:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -p harbor-text
cargo test --workspace
```

## DHAT Heap Capture

```bash
cargo run --profile dhat --features dhat-heap
```

The `dhat` Cargo profile retains symbols and line tables. Exit after the intended workload and dwell period. The process writes `dhat-heap.json`.

Analyze it with:

```bash
python scripts/dhat_analyze.py dhat-heap.json
python scripts/dhat_drill.py dhat-heap.json
python scripts/dhat_drill.py dhat-heap.json render_frame
```

Interactive viewer: <https://nnethercote.github.io/dh_view/dh_view.html>

## Font Lifecycle Scenario

Filter logs on target `harbor.font.lifecycle`. A cold Latin run that dwells for at least five seconds after first present should emit:

1. `font_init` with `source=system` or `source=configured`;
2. `first_present`;
3. `steady_state` with `dwell_ms >= 5000`;
4. no `first_fallback` for a Latin-only workload.

`first_fallback` should appear once when the first character successfully resolves through a non-primary face.

## Reference Latin Gate

| Gate             | Criterion                                                                                       |
| ---------------- | ----------------------------------------------------------------------------------------------- |
| Live-heap peak   | Global DHAT live-heap peak below 40 MiB                                                         |
| Full-file buffer | No Harbor-owned allocation equal to a complete font file                                        |
| Forbidden path   | No production `fontdb`, `fontdue`, candidate-list, CJK loader thread, or `fs::read` font loader |
| Private memory   | Peak and steady Windows private bytes lower than the equivalent legacy scenario                 |

The recorded DirectWrite capture met the heap and forbidden-path gates. Private-memory comparison remains to be recorded.

## Workload Matrix

Use separate captures for:

- cold Latin startup and idle dwell;
- sustained Latin output;
- first CJK fallback;
- sustained CJK, symbols, and emoji;
- configured primary font without CJK coverage;
- heavy scrollback;
- resize and DPI transitions;
- paste confirmation text rendering.

Do not combine unrelated workloads into one capture when attribution matters.

## Recording Template

| Field                      | Value |
| -------------------------- | ----- |
| Date                       |       |
| Commit                     |       |
| Build profile and features |       |
| Machine and OS             |       |
| GPU backend and adapter    |       |
| Font set / `HARBOR_FONT`   |       |
| Window and terminal size   |       |
| Workload                   |       |
| Dwell time                 |       |
| Profiling tool and mode    |       |
| Total allocated            |       |
| Allocation count           |       |
| Global live-heap peak      |       |
| Private peak / steady      |       |
| Lifecycle markers          |       |
| Known deviations           |       |

Store durable results in an evidence document or change record, not in the active optimization plan.

## Measurement Rules

- Use DHAT for allocation count, cumulative allocation, and heap peak.
- Do not use DHAT wall-clock timing as startup-latency evidence; stack collection changes timing substantially.
- Measure first-present, first-fallback, frame, and input latency with a release build and render metrics.
- Use ETW/WPR when Windows scheduling or I/O attribution requires finer evidence.
- Compare before and after with the same executable settings, machine, fonts, viewport, workload, and dwell.
- Distinguish process-lifetime resources from temporary churn and from true unbounded growth.
- Re-profile immediately after an optimization; source inspection alone does not prove impact.
