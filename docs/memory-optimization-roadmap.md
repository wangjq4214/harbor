# Memory Optimization Roadmap

> One document tracking Harbor's memory optimization work end to end: the pre-DirectWrite `fontdue` baseline, the delivered native font backend, the current measured state, and the remaining work. Supersedes `font-memory-optimization.md` and `font-memory-verification.md`.

## Contents

- [1. Status at a Glance](#1-status-at-a-glance)
- [2. Historical Baseline (Pre-DirectWrite)](#2-historical-baseline-pre-directwrite)
- [3. Current Measured State](#3-current-measured-state)
- [4. Before / After](#4-before--after)
- [5. Verification Checklist](#5-verification-checklist)
- [6. Remaining Roadmap](#6-remaining-roadmap)
- [7. Tooling and Measurement Rules](#7-tooling-and-measurement-rules)
- [8. Delivery Order and Acceptance](#8-delivery-order-and-acceptance)

## 1. Status at a Glance

| Item | Status | Evidence |
| --- | --- | --- |
| Native DirectWrite font backend (spec [0003-windows-system-native-font-backend](../.grimoire/spec/0003-windows-system-native-font-backend.md)) | ✅ Done | `perf(font): drop fontdue and use DirectWrite for all glyph paths`; live-heap peak gate met |
| Eager CJK fallback parsing eliminated (old plan Phase 1) | ✅ Done | Delivered by the DirectWrite path — no `fontdb`/`fontdue`/candidate-list/CJK-thread loader remains |
| Live-heap peak gate `< 40 MiB` | ✅ Met | Current capture upper bound **11.91 MiB** (8.87 s Latin run) |
| Per-frame vertex scratch reuse (old plan Phase 3b) | 🔜 Next | `render_frame` churn is now **42.1%** of all allocations — the dominant remaining cost |
| Dynamic glyph atlas growth (old plan Phase 3a) | ⬜ Open | Fixed 2048×2048 atlas holds 4.19 MiB from process start |
| Stable glyph identity (`GlyphKey`) | ⬜ Open | Atlas still keys by `char`; fine today, needed for multi-face coexistence |
| Tracing-subscriber slab (4.1 MiB) | ⚪ Accepted | Library overhead of `tracing-subscriber` JSON registry; no harbor-side change planned |

## 2. Historical Baseline (Pre-DirectWrite)

Recorded from a Windows `dhat` profile run before the native font backend (legacy `fontdb`/`fontdue` path).

| Metric | Result |
| --- | ---: |
| Profiled interval | 12.751 s |
| Total allocated | 744.45 MiB |
| Allocation count | 1,513,683 |
| Global live-heap peak | 188.29 MiB in 62,009 blocks |
| Heap live at profiler shutdown | 4.29 MiB in 61 blocks |

The end-of-profile value was not evidence of a leak: its largest allocation was the process-lifetime tracing configuration (3.91 MiB from `harbor::init_tracing`), with winit and wgpu global caches accounting for most of the remainder.

### Dominant Allocation Path

CJK fast-path loading accounted for the overwhelming majority of the capture:

| Metric | CJK fast-path result | Share of total/global peak |
| --- | ---: | ---: |
| Total allocated | 665.75 MiB | 89.4% of total allocations |
| Allocation count | 1,393,046 | 92.0% of all allocations |
| Live bytes at global peak | 179.53 MiB | 95.3% of global peak |

The relevant path was:

```text
load_candidate_fonts
  -> thread::spawn(load_first_cjk_font_file)
    -> load_font_file
      -> fs::read
      -> fontdue::Font::from_bytes
```

`fontdue::Font::from_bytes` eagerly parsed a large CJK font collection — approximately 363 MiB, 118 MiB, 98 MiB, and 51 MiB of cumulative allocations reached from the parser invocation, plus an ~18.79 MiB raw font-file read. On Windows its candidates included `msyh.ttc`, `msyh.ttf`, `simhei.ttf`, `simsun.ttc`, and `Deng.ttf`.

This was primarily a startup allocation and peak-memory problem, not a leak.

### Comparator Designs (Context)

- **Windows Terminal (AtlasEngine)** — resolves missing glyphs through DirectWrite `IDWriteFontFallback::MapCharacters`, delegating system font selection and collection handling to Windows.
- **Alacritty (`crossfont`)** — isolates platform font engines behind a common interface (DirectWrite / CoreText / FreeType); text rendering and glyph caching do not depend on one parser.
- **WezTerm** — explicit ordered fallback chain: primary face first, configured faces next, cached missing-character outcomes.

### Architecture Principles

1. Do not parse fallback fonts at startup unless needed.
2. Keep terminal metrics primary-font based; a fallback glyph must not alter cell geometry.
3. Cache fallback resolution; a missing glyph must not trigger repeated scans or parsing.
4. Separate font selection from rasterization and atlas placement.
5. Measure before replacing a parser; a swap may only move allocation cost.
6. Keep platform work optional behind a well-defined backend boundary.

All six principles are satisfied by the shipped DirectWrite backend.

## 3. Current Measured State

Capture produced by the `dhat` profile + `dhat-heap` feature on Windows with the DirectWrite backend.

| Metric | Result |
| --- | ---: |
| Profiled interval | 8.87 s |
| Total allocated | 27,743,707 B (26.46 MiB) |
| Allocation count | 12,631 |
| Global live-heap peak (upper bound) | 12,484,291 B (11.91 MiB) |
| Max-live sum (per-pp upper bound) | 15.15 MiB |

`gmax-live sum` is an upper bound on the true instantaneous peak (each program point's own max); the real peak is at most 11.91 MiB.

### Allocation by Owner

An owner is the deepest harbor frame in an allocation backtrace.

| Owner | Total | Live | Calls | Share |
| --- | ---: | ---: | ---: | ---: |
| `harbor::app::App::render_frame` | 11,693,153 B | 813,332 B | 1,877 | 42.1% |
| `harbor::app::impl$2::resumed` | 7,334,233 B | 6,803,368 B | 7,791 | 26.4% |
| `harbor::init_tracing` | 4,133,898 B | 4,133,846 B | 34 | 14.9% |
| `harbor::app::impl$2::window_event` | 3,809,970 B | 3,606,564 B | 840 | 13.7% |
| `TerminalRenderPipeline::new` (+ others) | ~0.77 MiB | ~0.6 MiB | — | 2.9% |

### 3.1 `render_frame` — per-frame churn (42.1% of all allocations)

`Text::prepare_with_dirty` → vertex builders allocate a fresh `Vec` every dirty frame and drop it after the GPU upload:

| Site | Total | Live | Calls | Pattern |
| --- | ---: | ---: | ---: | --- |
| `build_all_vertices` (text.rs:427) | ~7.4 MiB | ~687 KiB | 426 | `Vec::with_capacity(rows*cols*6)` ≈ 196,992 B per full rebuild |
| `build_range_vertices` (text.rs:369) / `collect_unique_chars_from_dirty` (text.rs:332) | ~3.9 MiB | ~76 KiB | 730 | `Vec::with_capacity((end-start)*6)` ≈ 10 KiB per range upload |
| `ScreenReader::terminal_snapshot` (reader.rs:27) | 71,424 B | 17,184 B | 20 | snapshot Vec rebuilt per call |
| `Runtime::dispatch` (runtime.rs:370) | 20,706 B | 770 B | 186 | widget event routing |

The same `Vec::with_capacity` pattern exists in `background.rs` (`build_all_vertices` line 122, `build_background_range_vertices` line 86) and `decoration.rs` (lines 22/55/241-242).

> Note: backtraces in this area can show a `naga::front::wgsl::lower::Lowerer::finalize_type` frame. This is an LTO/ICF symbol-merge artifact — `create_shader_module` is called only once in `Text::new` (text.rs:272). Shaders are **not** re-lowered per frame.

### 3.2 `resumed` — startup allocations, process-lifetime (26.4%)

| Site | Total = Live | Calls | Notes |
| --- | ---: | ---: | --- |
| `GlyphAtlas::new` (atlas.rs:149) | 4,194,304 B | 1 | `vec![0; 2048*2048]` fixed 4 MiB atlas |
| `Screen::new` (screen.rs:64) | 881,400 B | 4 | `NormalBuf` cell storage |
| `Text::new` (text.rs:300) | 393,984 B | 20 | initial atlas/vertex work |
| `Background::new` (background.rs:57) | 295,488 B | 20 | |
| `Decoration::new` (decoration.rs:107/113/122) | 3 × 147,744 B | 1 | |

### 3.3 `init_tracing` — library overhead (14.9%)

4,096,905 B live across 8 calls at main.rs:61: the `tracing-subscriber` JSON registry's `sharded_slab` shard plus glow GL function-table loading during tracing initialization. This is library-固有 overhead, not harbor code; accepted as a process-lifetime cost (~15.3% of peak).

### 3.4 `window_event` — resize surface textures (13.7%)

`Terminal::resize_if_changed` (lib.rs:287) → `resize_gpu` (lib.rs:159) allocates wgpu swapchain/back-buffer textures on resize: 3,547,952 B live across 4 resize events (~887 KiB each). Expected GPU-renderer behavior; textures are replaced on the next resize, not accumulated.

### 3.5 Leak assessment

No unbounded growth: every live program point is either a one-time startup allocation (atlas, screen, tracing slab, resize textures) or a bounded per-frame scratch Vec. Live footprint is flat after startup.

## 4. Before / After

| Metric | Pre-DirectWrite (12.75 s) | DirectWrite (8.87 s) | Change |
| --- | ---: | ---: | ---: |
| Total allocated | 744.45 MiB | 26.46 MiB | **−96.4%** |
| Allocation count | 1,513,683 | 12,631 | **−99.2%** |
| Global live-heap peak | 188.29 MiB | ≤ 11.91 MiB | **−93.7%** |
| Heap live at shutdown | 4.29 MiB | — | n/a (capture ended mid-run) |

The dominant 665.75 MiB CJK fast-path allocation is gone entirely: no font file is read or parsed by harbor at startup, and missing-glyph resolution is delegated to DirectWrite on demand.

## 5. Verification Checklist

> Replaces the T0005 checklist from the merged `font-memory-verification.md`. The reference Latin gate is **met** by the 2026-08-01 capture (11.91 MiB peak upper bound < 40 MiB).

### Lifecycle markers

Filter logs on target `harbor.font.lifecycle`. Expected phases for a cold Latin run that dwells ≥ 5 s after first present:

1. `font_init` (`source` = `system` or `configured`)
2. `first_present`
3. `steady_state` (`dwell_ms` ≥ 5000)
4. `first_fallback` — **absent** on Latin-only; appears once on first successful missing-glyph map

### Commands

```bash
# Quality gates (Windows)
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -p harbor-text
cargo test --workspace

# Latin DHAT (reference machine; exit after steady_state appears in logs)
cargo run --profile dhat --features dhat-heap
# Analyze: python scripts/dhat_analyze.py dhat-heap.json
# Inspect interactively in https://nnethercote.github.io/dh_view/dh_view.html
```

### Acceptance gates (reference Latin scenario)

| Gate | Criterion | Current result |
| --- | --- | --- |
| Live-heap peak | Global DHAT live-heap peak < **40 MiB** | ✅ 11.91 MiB upper bound |
| Full-file buffer | No Rust allocation equal to a complete font file; no Harbor-owned complete font-file `Vec<u8>` at first present / steady state | ✅ DirectWrite reads nothing eagerly |
| Private memory | Peak and steady Windows private bytes both lower than the identical pre-change executable scenario | ⬜ To record on reference machine |
| Forbidden paths | Production `harbor-text` has no `fontdb` / `fontdue` / candidate / CJK-thread / `fs::read` font loader | ✅ None remain |

### Recording template

Fill when running on the reference machine:

| Field | Value |
| --- | --- |
| Date | 2026-08-01 (last capture) |
| Machine / OS | (fill) |
| Commit / build | post `perf(font): drop fontdue and use DirectWrite for all glyph paths` |
| Font set / `HARBOR_FONT` | unset (system primary) |
| Screen size | (fill) |
| Dwell | ≥ 5 s after `first_present` |
| Backend | DirectWrite |
| Profiling mode | `dhat` profile + `dhat-heap` |
| DHAT live-heap peak | 11.91 MiB upper bound (must be < 40) |
| Private peak / steady | (fill; must be < pre-change) |
| Markers observed | `font_init`, `first_present`, `steady_state` |

Additional scenarios (evidence, no hard Latin heap gate): sustained Latin; first CJK; sustained CJK/symbol/emoji; configured primary lacking CJK.

## 6. Remaining Roadmap

Priorities shifted: with the font path eliminated, the per-frame render churn the old plan ranked third is now the dominant cost. Order R1 → R2 → R3; R4 is a deliberate architecture project only if evidence demands it.

### R1 — Reuse Vertex Scratch Buffers (highest value)

**Where:** `crates/harbor-terminal/src/render/text.rs`, `background.rs`, `decoration.rs`.

`build_all_vertices` / `build_range_vertices` / `collect_unique_chars_from_dirty` (and their background/decoration analogues) allocate short-lived `Vec`s on every dirty frame — 42.1% of the capture's allocations, ~11.6 MiB in an 8.87 s run.

**Design:** store reusable scratch vectors in each render component; `clear()` + `resize`/`extend` per update, growing only when a larger terminal or dirty range requires it. Preserve the current dirty-range upload behavior. The same treatment applies to `ScreenReader::terminal_snapshot` if its call frequency justifies it (currently 71 KiB / 20 calls).

**Acceptance:** render-frame allocation count declines without regressing dirty-range uploads; identical pixels before/after.

### R2 — Dynamic Glyph Atlas Growth

**Where:** `crates/harbor-text/src/atlas.rs` (`MAX_ATLAS_SIZE = 2048`, fixed `vec![0; 2048*2048]` = 4.19 MiB at `GlyphAtlas::new`).

**Design:**

1. allocate a small initial texture (e.g. 512×512);
2. grow 512 → 1024 → 2048 only when required;
3. on growth, repack glyphs, recreate texture and bind group, force a full UV/vertex upload;
4. preserve incremental tile upload when the texture does not grow;
5. retain eviction only at the configured maximum size.

Tests must cover placement, UV correctness after repacking, texture/bind-group replacement, CJK glyph rendering after growth, and the maximum-size eviction path.

**Acceptance:** Latin-idle CPU/GPU atlas residency measured before/after; growth produces no glyph corruption after resize, DPI change, dialog rendering, or eviction.

### R3 — Stable Glyph Identity (`GlyphKey`) Follow-up

**Where:** `crates/harbor-text` atlas keying.

The atlas keys cached glyphs by `char`. This holds while rendering is primary-face + DirectWrite system fallback, but does not generalize to multiple explicit fallback faces, emoji variation selectors, or per-face style variants:

```rust
struct GlyphKey {
    face_id: FaceId,
    glyph_id: u32,
    size: FontSizeKey,
    style: FontStyleKey,
}
```

Keep a code-point → resolution cache separate from the atlas; the atlas then stores rasterized glyphs by `GlyphKey`.

**Acceptance:** multiple fallback faces coexist without atlas corruption; emoji presentation and fallback-face selection testable independently of atlas placement; observable rendering unchanged for primary-only sessions.

### R4 — Tracing Slab (assessed, accepted)

The 4.1 MiB `sharded_slab` registry shard is `tracing-subscriber`'s JSON-registry overhead (14.9% of allocations, ~15% of peak, process-lifetime). No harbor-side change is planned; revisit only if a lighter event pipeline (e.g. `tracing-core` custom subscriber or reduced registry usage) is wanted.

### Accepted costs (no action)

- `window_event` resize surface textures (~887 KiB per resize, replaced not accumulated) — wgpu swapchain behavior.
- winit / wgpu global caches — external, bounded.

## 7. Tooling and Measurement Rules

### DHAT capture

```bash
cargo run --profile dhat --features dhat-heap
# writes dhat-heap.json; exit the app when steady_state appears in harbor.log
```

The `dhat` profile (release + `debug = 1`, `strip = "none"`) preserves symbols/line tables for allocation backtraces.

### Analysis scripts (`scripts/`)

```bash
python scripts/dhat_analyze.py dhat-heap.json   # totals + owner breakdown
python scripts/dhat_drill.py dhat-heap.json render_frame   # drill a specific owner
python scripts/dhat_drill.py dhat-heap.json     # top four owners
```

`dhat_analyze.py` prints total allocations, per-pp max-live and gmax-live upper bounds, and the by-owner table used in this document. `dhat_drill.py` groups an owner's program points into call-site clusters (deepest harbor frames + allocation location).

Interactive inspection: https://nnethercote.github.io/dh_view/dh_view.html

### Measurement rules

- Use DHAT for allocation count, total allocation, and heap peak. Do **not** use DHAT timing as startup-latency evidence: stack collection adds substantial overhead on Windows.
- Measure first-present / first-CJK latency with a release build and render metrics; use ETW/WPR if finer Windows timing is needed.
- Each scenario records the executable, backend, font set, screen size, dwell time, and profiling mode, so startup work is distinguishable from first-CJK work.

## 8. Delivery Order and Acceptance

Implement and verify each independently reviewable unit:

1. `perf(render): reuse vertex scratch buffers` (R1) — biggest remaining cost; re-profile immediately after.
2. `perf(text): grow the glyph atlas on demand` (R2).
3. `refactor(text): stable glyph identity` (R3) — only as fallback-chain work begins.
4. (Optional, evidence-gated) DirectWrite shaping/complex-script work — separate project; not a memory prerequisite.

Each unit must pass focused tests, `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and the DHAT scenarios in §5 before this roadmap's status table is updated. Keep the Latin gate (< 40 MiB peak) green at every step — the current capture is at 11.91 MiB upper bound, leaving ~70% headroom for future feature work.
