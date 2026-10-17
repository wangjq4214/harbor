# Performance Profiling Guide

This document defines repeatable performance and memory measurement procedures for Harbor, preserves the [historical memory baseline](#historical-memory-baseline), and tracks the [measurement-gated optimization backlog](#measured-optimization-backlog). Product scope and priority remain in N15 of the [product plan](../next-stage-plan.md) and the [roadmap](../roadmap.md); [current status](../current-status.md) separates implementation from runtime evidence.

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
python scripts/profiling/dhat_analyze.py dhat-heap.json
python scripts/profiling/dhat_drill.py dhat-heap.json
python scripts/profiling/dhat_drill.py dhat-heap.json render_frame
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
| Font family and size       |       |
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

Store durable results in a clearly revision-scoped evidence section or change record, not as completion claims in the active optimization backlog.

## Measurement Rules

- Use DHAT for allocation count, cumulative allocation, and heap peak.
- Do not use DHAT wall-clock timing as startup-latency evidence; stack collection changes timing substantially.
- Measure first-present, first-fallback, frame, and input latency with a release build and render metrics.
- Use ETW/WPR when Windows scheduling or I/O attribution requires finer evidence.
- Compare before and after with the same executable settings, machine, fonts, viewport, workload, and dwell.
- Distinguish process-lifetime resources from temporary churn and from true unbounded growth.
- Re-profile immediately after an optimization; source inspection alone does not prove impact.

## Historical Memory Baseline

These Windows heap captures compare the legacy `fontdb`/`fontdue` path with its DirectWrite replacement. They are historical evidence, not current-tree measurements or startup-latency benchmarks. The captures used different intervals; their timings are not comparable. The DirectWrite capture used the `dhat` profile and `dhat-heap` feature.

| Metric | Legacy font path | DirectWrite path | Change |
| --- | ---: | ---: | ---: |
| Profiled interval | 12.751 s (12.75 s rounded) | 8.87 s | Not comparable |
| Total allocated | 744.45 MiB | 27,743,707 B (26.46 MiB) | −96.4% |
| Allocation count | 1,513,683 | 12,631 | −99.2% |
| Global live-heap peak | 188.29 MiB in 62,009 blocks | 12,484,291 B (11.91 MiB upper bound) | −93.7% |
| Heap live at profiler shutdown | 4.29 MiB in 61 blocks | Not recorded here | — |
| Sum of per-program-point maxima | Not recorded here | 15.15 MiB | Not an instantaneous process peak |

The global maximum, not the sum of per-program-point maxima, is the relevant heap-peak measure.

### Legacy allocation cause

| CJK fast-path metric | Result | Share of legacy capture |
| --- | ---: | ---: |
| Total allocated | 665.75 MiB | 89.4% |
| Allocation count | 1,393,046 | 92.0% |
| Live bytes at global peak | 179.53 MiB | 95.3% |

The path was `load_candidate_fonts -> thread::spawn(load_first_cjk_font_file) -> load_font_file -> fs::read -> fontdue::Font::from_bytes`. Eager reading/parsing of large CJK font collections caused startup and peak-memory cost, not an observed unbounded leak.

### Delivered font boundary

DirectWrite replaced production `fontdb`, `fontdue`, candidate lists, the CJK loader thread and full-file font reads. System selection/fallback uses native APIs; fallback resolves on demand, terminal metrics stay based on the primary face, and Harbor retains no full font-file buffers. Code-point resolution caching is separate from atlas storage; rasterized `GlyphKey` identity includes face ID, glyph ID, size and style.

### DirectWrite allocation owners and limits

Capture-era symbols describe the captured executable, not necessarily today's modules or line numbers.

| Capture-era owner | Total | Live | Calls | Share |
| --- | ---: | ---: | ---: | ---: |
| `harbor::app::App::render_frame` | 11,693,153 B | 813,332 B | 1,877 | 42.1% |
| `harbor::app::impl$2::resumed` | 7,334,233 B | 6,803,368 B | 7,791 | 26.4% |
| `harbor::init_tracing` | 4,133,898 B | 4,133,846 B | 34 | 14.9% |
| `harbor::app::impl$2::window_event` | 3,809,970 B | 3,606,564 B | 840 | 13.7% |
| Terminal pipeline creation and other owners | ~0.77 MiB | ~0.6 MiB | — | 2.9% |

Per-frame vertex builders accounted for about 11.6 MiB of short-lived allocation. The fixed 2048×2048 CPU atlas allocated 4 MiB at startup, with a matching fixed-size GPU texture. The tracing registry retained about 4.1 MiB of bounded process-lifetime overhead. Resize replaced swapchain textures without observed accumulation.

The live footprint was flat after startup; no unbounded growth was observed **in this workload**. That is not evidence for future features or long-running workloads.

| Evidence gate | Recorded result |
| --- | --- |
| Reference Latin DHAT live-heap peak below 40 MiB | Met: ≤ 11.91 MiB |
| No Harbor-owned full font-file buffer | Met |
| No legacy font loader in production | Met |
| Windows private-memory before/after on the reference machine | Not yet recorded |
| Sustained CJK, symbol and emoji scenarios | Additional evidence required |

## Measured Optimization Backlog

The entries below are candidates and constraints, not a new implementation schedule or evidence that savings have been achieved. Follow [validation](../validation.md) and the procedures above.

### Current profile and re-profile gate

Capture comparable Latin and dirty-range-heavy workloads on a recorded current commit **before selecting an optimization**. The historical 11.6 MiB/42.1% render attribution is not a profile of `a53395d` or the current tree. Source inspection can establish remaining allocations, not their current cost or rank.

After any selected optimization, repeat both workloads and compare cumulative bytes, allocation count and live peak; identify the new dominant Harbor-owned path and confirm or revise the next candidate. Do not start another speculative memory refactor before this gate.

### R1 — Renderer scratch reuse (open, measurement-gated)

Full/range vertex builders and dirty-character collection create short-lived vectors. Candidate owners are terminal [text](../../crates/harbor-terminal/src/render/text.rs), [background](../../crates/harbor-terminal/src/render/background.rs) and [decoration](../../crates/harbor-terminal/src/render/decoration.rs) components.

- Keep scratch capacity in its owning render component, growing only for larger terminals/upload ranges; never share it globally across independent renderer lifetimes.
- Preserve incremental versus full-upload decisions and offsets. Consider snapshot optimization separately only if the post-optimization capture makes it material.
- **Acceptance:** lower allocation count and cumulative bytes under the same workload, unchanged rendered output, correct incremental/full uploads, and standard quality gates passing.

### R2 — Atlas residency (open, measurement-gated)

[AtlasStore](../../crates/harbor-text/src/atlas.rs) and the [terminal](../../crates/harbor-terminal/src/render/text.rs)/[widget](../../crates/harbor-widget/src/renderer/widget_text_atlas.rs) adapters use a fixed 2048×2048 atlas: 4 MiB CPU pixels plus a fixed GPU texture **per atlas**, not a measurement of whole-process residency across tabs/windows. Dynamic growth is only warranted if the current and post-optimization profiles justify it; a smaller initial size is not yet an accepted design.

If growth is selected, preserve bounded maximum dimensions and eviction at that maximum, correct repacking/UV invalidation, GPU texture/bind-group replacement and a full vertex upload after repacking; retain incremental tile uploads while dimensions are stable. The former candidate ceiling was 2048×2048.

**Required validation:** placement/used height at each size, UVs after repack, CPU/GPU dimension transitions, texture/bind-group replacement, full-upload invalidation, maximum-size eviction, Latin/CJK/confirmation-window output, resize and DPI changes. Acceptance requires lower Latin-idle CPU/GPU atlas residency without glyph corruption and the reference heap gate still below 40 MiB.

### Delivered identity and accepted bounded costs

`GlyphKey` identity and separate resolution caching are delivered; do not redesign them as prerequisites for R1. Remaining hardening covers deterministic coexistence of fallback faces, emoji presentation/variation selectors, and end-to-end primary/fallback visual evidence.

No Harbor change is planned for the bounded `tracing-subscriber` registry slab, winit/wgpu global caches or swapchain replacement on resize. Reopen these only if a comparable capture shows they materially block a product memory target.

### N15 — Feature budgets and lifecycle evidence

Establish numeric budgets from recorded workloads; no current budget or promised saving is implied here.

| Area | Measure and budget | Lifecycle evidence |
| --- | --- | --- |
| Tabs/panes | Incremental heap/private bytes, atlas/GPU residency, frame/input latency for visible and inactive sessions | Open, switch, split, resize, close; retained resources settle |
| Font reload | Old/new font, atlas and vertex overlap peak; reload latency and settled memory | Repeated reload/DPI changes; old resources become reclaimable |
| Graphics | Decoded CPU bytes, GPU residency, upload volume and frame cost | Replacement, eviction and session close under bounded limits |
| Glass/backdrop | Compositor/GPU cost, resize overhead, idle behavior with effects on/off | Minimize/restore, fallback and window close |

Record session counts with the machine, commit, profile, fonts, viewport and dwell fields above. Use DHAT for allocation evidence, Windows private bytes for process residency, and release/render or GPU/compositor tooling for latency/graphics cost. Keep mixed-feature captures separate from the historical reference scenario. Complex shaping, threading and renderer restructures require a demonstrated bottleneck and a separate scoped proposal.
