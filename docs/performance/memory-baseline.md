# Memory Baseline

This document preserves measured Windows heap evidence before and after Harbor replaced the legacy `fontdb`/`fontdue` path with DirectWrite. It is historical evidence, not an active implementation plan.

## Summary

| Metric                | Legacy font path | DirectWrite path |                                       Change |
| --------------------- | ---------------: | ---------------: | -------------------------------------------: |
| Profiled interval     |          12.75 s |           8.87 s | Different captures; timing is not comparable |
| Total allocated       |       744.45 MiB |        26.46 MiB |                                   **−96.4%** |
| Allocation count      |        1,513,683 |           12,631 |                                   **−99.2%** |
| Global live-heap peak |       188.29 MiB |      ≤ 11.91 MiB |                                   **−93.7%** |

The reference Latin live-heap gate is below 40 MiB. The DirectWrite capture met it with an 11.91 MiB upper bound.

## Legacy Baseline

The pre-DirectWrite capture ran for 12.751 seconds:

| Metric                         |                      Result |
| ------------------------------ | --------------------------: |
| Total allocated                |                  744.45 MiB |
| Allocation count               |                   1,513,683 |
| Global live-heap peak          | 188.29 MiB in 62,009 blocks |
| Heap live at profiler shutdown |       4.29 MiB in 61 blocks |

CJK fast-path loading dominated the capture:

| Metric                    |   CJK path | Share |
| ------------------------- | ---------: | ----: |
| Total allocated           | 665.75 MiB | 89.4% |
| Allocation count          |  1,393,046 | 92.0% |
| Live bytes at global peak | 179.53 MiB | 95.3% |

The allocation path was:

```text
load_candidate_fonts
  -> thread::spawn(load_first_cjk_font_file)
    -> load_font_file
      -> fs::read
      -> fontdue::Font::from_bytes
```

The process eagerly read and parsed large CJK font collections. The capture showed a startup and peak-memory problem, not an unbounded leak.

## Delivered DirectWrite Design

The replacement follows these constraints:

1. system font selection and fallback use native APIs;
2. fallback fonts are resolved on demand rather than parsed at startup;
3. terminal metrics remain based on the primary face;
4. code-point resolution is cached separately from rasterized atlas entries;
5. rasterized glyph identity includes face, glyph ID, size, and style;
6. Harbor does not retain full font-file buffers.

The delivered backend removed the production `fontdb`, `fontdue`, candidate-list, CJK-loader-thread, and full-file font-read paths.

## DirectWrite Capture

The Windows DirectWrite capture used the `dhat` profile and `dhat-heap` feature:

| Metric                          |                   Result |
| ------------------------------- | -----------------------: |
| Profiled interval               |                   8.87 s |
| Total allocated                 | 27,743,707 B (26.46 MiB) |
| Allocation count                |                   12,631 |
| Global live-heap peak           | 12,484,291 B (11.91 MiB) |
| Sum of per-program-point maxima |                15.15 MiB |

The per-program-point sum is not an instantaneous process peak. The global maximum is the relevant 11.91 MiB upper bound.

### Allocation owners at capture time

| Capture-era owner                           |        Total |        Live | Calls | Share |
| ------------------------------------------- | -----------: | ----------: | ----: | ----: |
| `harbor::app::App::render_frame`            | 11,693,153 B |   813,332 B | 1,877 | 42.1% |
| `harbor::app::impl$2::resumed`              |  7,334,233 B | 6,803,368 B | 7,791 | 26.4% |
| `harbor::init_tracing`                      |  4,133,898 B | 4,133,846 B |    34 | 14.9% |
| `harbor::app::impl$2::window_event`         |  3,809,970 B | 3,606,564 B |   840 | 13.7% |
| Terminal pipeline creation and other owners |    ~0.77 MiB |    ~0.6 MiB |     — |  2.9% |

Symbol names and source line numbers in DHAT captures reflect the executable used for that capture and may differ from the current tree.

### Remaining measured costs

- Per-frame vertex builders produced about 11.6 MiB of short-lived allocations during the capture.
- The fixed 2048×2048 CPU atlas allocated 4 MiB at startup, with a matching fixed-size GPU texture.
- The `tracing-subscriber` registry retained about 4.1 MiB as bounded process-lifetime library overhead.
- Surface resize replaced swapchain textures; the capture did not show accumulation.

## Leak Assessment

The DirectWrite capture showed a flat live footprint after startup. Remaining live allocations were bounded startup resources, external runtime caches, resize resources, or temporary per-frame vectors. No unbounded growth was observed in this scenario.

This assessment applies only to the recorded workload. Future features and long-running workloads still require their own evidence.

## Evidence Status

| Gate                                           | Result                                    |
| ---------------------------------------------- | ----------------------------------------- |
| Latin DHAT live-heap peak below 40 MiB         | Met: ≤ 11.91 MiB                          |
| No Harbor-owned full font-file buffer          | Met                                       |
| No legacy font loader in production            | Met                                       |
| Windows private-memory before/after comparison | Not yet recorded on the reference machine |
| Sustained CJK, symbol, and emoji scenarios     | Additional evidence required              |

See [`profiling-guide.md`](profiling-guide.md) to reproduce or extend these measurements and [`optimization-plan.md`](optimization-plan.md) for open work.
