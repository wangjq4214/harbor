# Font Memory and Startup Optimization Plan

## Status

**Proposal.** No implementation work is included in this document.

## Goal

Reduce Windows startup heap pressure and allocation churn while preserving Harbor's custom WGPU renderer, stable terminal cell metrics, and CJK/emoji fallback behavior.

The immediate goal is not to replace the text stack. The first high-value change is to defer fallback-font parsing until a missing glyph actually requires it. A native DirectWrite backend is an optional later architecture phase.

## Measured Baseline

The `dhat-heap.json` capture was produced by a Windows `dhat` profile run.

| Metric | Result |
| --- | ---: |
| Profiled interval | 12.751 s |
| Total allocated | 744.45 MiB |
| Allocation count | 1,513,683 |
| Global live-heap peak | 188.29 MiB in 62,009 blocks |
| Heap live at profiler shutdown | 4.29 MiB in 61 blocks |

The end-of-profile value is not evidence of an application leak. Its largest allocation is the process-lifetime tracing configuration: 3.91 MiB from `harbor::init_tracing`. Winit and WGPU global caches account for most of the remainder. Font, terminal, and render allocations were released before the profiler shut down.

### Dominant Allocation Path

CJK fast-path loading accounts for the overwhelming majority of the capture:

| Metric | CJK fast-path result | Share of total/global peak |
| --- | ---: | ---: |
| Total allocated | 665.75 MiB | 89.4% of total allocations |
| Allocation count | 1,393,046 | 92.0% of all allocations |
| Live bytes at global peak | 179.53 MiB | 95.3% of global peak |

The relevant current path is:

```text
load_candidate_fonts
  -> thread::spawn(load_first_cjk_font_file)
    -> load_font_file
      -> fs::read
      -> fontdue::Font::from_bytes
```

In `crates/harbor-text/src/font.rs`, `load_candidate_fonts` starts the CJK loader before determining whether the selected primary font requires fallback. On Windows, its candidates include `msyh.ttc`, `msyh.ttf`, `simhei.ttf`, `simsun.ttc`, and `Deng.ttf`.

`fontdue::Font::from_bytes` eagerly parses a large CJK font collection. The profile attributes approximately 363 MiB, 118 MiB, 98 MiB, and 51 MiB of cumulative allocations to internal calls reached from this parser invocation. The raw font-file read alone is approximately 18.79 MiB.

This is primarily a startup allocation and peak-memory problem, not a leak.

## Comparator Designs

### Windows Terminal

Windows Terminal's Atlas renderer uses DirectWrite. DirectWrite can resolve missing Unicode characters through `IDWriteFontFallback::MapCharacters`, which maps a text range to an appropriate fallback font. This delegates system font selection, script-aware fallback, and collection handling to Windows instead of manually reading a small list of TTC files.

Relevant references:

- [Windows Terminal AtlasEngine](https://github.com/microsoft/terminal/blob/main/src/renderer/atlas/AtlasEngine.cpp)
- [IDWriteFontFallback::MapCharacters](https://learn.microsoft.com/en-us/windows/win32/api/dwrite_2/nf-dwrite_2-idwritefontfallback-mapcharacters)
- [IDWriteFactory2::GetSystemFontFallback](https://learn.microsoft.com/en-us/windows/win32/api/dwrite_2/nf-dwrite_2-idwritefactory2-getsystemfontfallback)

### Alacritty

Alacritty's `crossfont` isolates platform font engines behind a common interface: DirectWrite on Windows, CoreText on macOS, and FreeType elsewhere. The important architectural lesson is the boundary: text rendering and glyph caching do not directly depend on one parser implementation.

- [crossfont platform backend selection](https://docs.rs/crossfont/latest/src/crossfont/lib.rs.html)

### WezTerm

WezTerm exposes an ordered fallback chain. It tries the primary face first and checks subsequent configured faces when a glyph is absent. This is a useful policy model for Harbor: an explicit primary face, a fallback chain, and cached outcomes for missing characters.

- [WezTerm `font_with_fallback`](https://wezterm.org/config/lua/wezterm/font_with_fallback.html)
- [WezTerm font configuration](https://wezterm.org/config/fonts.html)

## Architecture Principles

1. **Do not parse fallback fonts at startup unless needed.** A Latin-only session must not open or parse a CJK font.
2. **Keep terminal metrics primary-font based.** A fallback glyph may not alter the configured cell width, height, baseline, or line spacing.
3. **Cache fallback resolution.** A missing glyph must not trigger repeated candidate scans or parsing.
4. **Separate font selection from rasterization and atlas placement.** This keeps a future DirectWrite backend feasible.
5. **Measure before replacing a parser.** Replacing `fontdue` with another parser without measuring TTC behavior may only move the allocation cost.
6. **Keep platform work optional.** The default path remains portable; Windows-native DirectWrite is introduced behind a well-defined backend boundary.

## Phase 0: Establish Reproducible Evidence

### Work

Add structured, low-volume metrics around:

- primary face resolution start/end;
- CJK fallback resolver start/end;
- selected face name and input file size;
- first window presentation;
- first missing-glyph resolution;
- atlas allocation, growth, eviction, and GPU upload size.

Use these fixed scenarios:

1. cold launch with Latin-only content;
2. high-throughput Latin output;
3. first rendering of a CJK character;
4. sustained CJK and emoji output;
5. configured primary font that already contains CJK glyphs.

### Measurement Rules

Use DHAT for allocation count, total allocation, and heap peak. Do not use DHAT timing as a startup-latency result: stack collection can impose substantial overhead on Windows. Measure first-present and first-CJK latency with a release build and the application's render metrics; use ETW/WPR if finer Windows timing is needed.

### Acceptance

Each scenario records the executable, backend, font set, screen size, dwell time, and profiling mode. A report can distinguish startup work from first-CJK work.

## Phase 1: Make Fallback Loading Lazy

### Scope

Target files:

- `crates/harbor-text/src/font.rs`
- `crates/harbor-terminal/src/render/text.rs`
- callers that currently pass an immutable `FontBook`

### Design

Remove the unconditional nested CJK loader from `load_candidate_fonts`. Startup should load only the primary face.

Replace the eager `FontBook { fonts: Vec<LoadedFont> }` behavior with a primary face plus deferred fallback state:

```rust
enum FallbackState {
    Unresolved(FallbackResolver),
    Ready(Vec<LoadedFont>),
    Unavailable,
}
```

`FallbackResolver` holds candidate metadata rather than parsed font objects. On the first glyph absent from all loaded faces, it should:

1. test the ordered fast-path candidate list;
2. parse only enough candidates to find a usable fallback;
3. optionally invoke the current `fontdb` fallback path if the fast-path list has no match;
4. retain the selected face in `Ready`;
5. cache `Unavailable` when no suitable face exists.

Fallback success and failure must both be cached. The resolver should eventually support a chain rather than a single CJK face, because emoji, symbols, CJK, and user-selected fonts need not come from one file.

`FontBook::rasterize` will need mutable access so that it can resolve and retain a fallback on demand. Propagate that mutability only through glyph discovery/rasterization; do not make unrelated render-state APIs mutable.

### Initial Scheduling Policy

The first implementation should resolve synchronously on the first missing glyph. This is the smallest correct behavior and completely removes the Latin startup cost.

Do not add an asynchronous fallback parser before measuring this version. An asynchronous version needs a placeholder-glyph policy, a completion notification, redraw coordination, and cancellation/ownership rules for Windows font resources. If it becomes necessary later, it should run only after the first successful present and never as an unconditional startup task.

### Current Concurrency Defect

Dropping a `JoinHandle` does not cancel its thread. The current early return when a primary face contains the CJK probe, or when primary loading fails, can leave the background CJK parser running without a result consumer. Removing the eager thread fixes this behavior as well as the allocation cost.

### Tests

Use an injectable resolver or test font source rather than relying on host system fonts.

- Latin startup does not invoke the CJK resolver.
- First missing CJK glyph invokes resolution exactly once.
- A failed resolution is cached.
- A successful fallback is reused by later glyphs.
- Fallback glyphs preserve primary cell metrics.
- A configured primary font with CJK coverage never opens a fallback font.

### Acceptance

For the existing Latin-only DHAT scenario:

- zero calls to `load_first_cjk_font_file`;
- at least 90% fewer allocations than the 1,513,683-allocation baseline;
- a target global heap peak below 40 MiB on the profiled machine.

For CJK output:

- the first CJK glyph renders correctly;
- subsequent CJK glyphs do not reparse the fallback file;
- the deferred first-use cost is separately recorded.

## Phase 2: Introduce Stable Glyph Identity

The current atlas can key cached glyphs by `char` while only one primary and one manually selected fallback are involved. That does not generalize to system fallback, emoji variation, or multiple fallback faces.

Introduce a glyph key concept before adding a native backend:

```rust
struct GlyphKey {
    face_id: FaceId,
    glyph_id: u32,
    size: FontSizeKey,
    style: FontStyleKey,
}
```

Maintain a code-point-to-resolution cache separately from the atlas. The atlas then stores rasterized glyphs by `GlyphKey`, which prevents collisions when the same Unicode scalar resolves to different faces or glyph variants.

### Acceptance

- Multiple fallback faces can coexist without atlas corruption.
- Emoji presentation and fallback face selection can be tested independently of atlas placement.
- Existing primary-only glyph-cache behavior remains unchanged in observable rendering results.

## Phase 3: Reduce Atlas and Per-Frame Allocation Costs

The supplied profile does not identify the atlas or vertex work as the primary issue, so this phase follows lazy fallback work.

### Dynamic Atlas Allocation

`Text::new` currently allocates a fixed 2048 x 2048 R8 pixel buffer, which is roughly 4 MiB of CPU memory and a corresponding GPU texture even for an empty terminal.

Change the atlas policy to:

1. allocate a small initial texture, such as 512 x 512;
2. grow 512 -> 1024 -> 2048 only when required;
3. on growth, repack glyphs, recreate the texture and bind group, and force a full UV/vertex upload;
4. preserve incremental tile upload when the texture does not grow;
5. retain eviction only at the configured maximum size.

Tests must cover placement, UV correctness after repacking, texture/bind-group replacement, CJK glyph rendering after growth, and the maximum-size eviction path.

### Reuse Vertex Scratch Buffers

`Text::build_all_vertices`, `Text::build_range_vertices`, and decoration builders create short-lived vectors. They account for a small fraction of cumulative allocations compared with font parsing, but are worth addressing after the dominant path is fixed.

Store reusable scratch vectors in the relevant render component. Clear and reuse capacity per update; grow only when a larger terminal or dirty range requires it. Preserve the current dirty-range upload behavior.

### Acceptance

- Latin-idle CPU/GPU atlas residency is measured before and after the change.
- Atlas growth produces no glyph corruption after resize, DPI change, dialog rendering, or eviction.
- Render-frame allocation count declines without regressing dirty-range upload behavior.

## Phase 4: Optional Windows DirectWrite Backend

This is a deliberate architecture project, not a quick optimization patch.

### Preconditions

Proceed only if Phase 1 leaves unacceptable first-CJK latency or memory peak, or if Harbor needs robust Windows fallback for scripts and emoji beyond a curated candidate list.

### Design

Define a backend boundary similar to:

```rust
trait FontBackend {
    fn resolve_glyph(&mut self, ch: char, style: FontStyle) -> GlyphKey;
    fn rasterize(&mut self, key: GlyphKey, px: f32) -> RasterizedGlyph;
    fn metrics(&self, px: f32) -> FontMetrics;
}
```

The Windows backend should:

1. create a DirectWrite factory and system collection;
2. use `GetSystemFontFallback` and `MapCharacters` to resolve missing text;
3. retain DirectWrite font-face identities and glyph IDs;
4. rasterize into the existing WGPU atlas rather than replacing Harbor's GPU pipeline;
5. maintain terminal-cell semantics explicitly, with ligatures disabled initially;
6. define COM apartment ownership and thread boundaries before any background resolution.

### Non-goals for the First DirectWrite Version

- Do not use DirectWrite only to discover a path and then parse that file with `fontdue`; this preserves the costly eager parse.
- Do not combine complex-script shaping with this memory project. Shaping, ligatures, grapheme clusters, and terminal-cell width policy require separate compatibility design.
- Do not scan all system fonts at startup.

### Acceptance

Compare the same Phase 0 scenarios against the portable backend. The DirectWrite backend must preserve primary metrics, select fallback glyphs correctly, avoid repeated font resolution, and improve the measured first-CJK memory/latency target enough to justify its platform-specific maintenance cost.

## Delivery Order

Implement and verify each independently reviewable unit:

1. `test(font): add deterministic fallback-resolution fixtures and baseline metrics`
2. `perf(font): defer fallback loading until a glyph requires it`
3. `perf(render): grow the glyph atlas on demand and reuse vertex scratch buffers`
4. `feat(font): add an experimental DirectWrite backend` only if Phase 1 evidence warrants it

The implementation must run focused tests, `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and the defined profiling scenarios before changing roadmap or checklist status.

## Recommendation

Implement Phase 1 first. It removes the work responsible for almost all allocations in the supplied profile without sacrificing current rendering architecture or portability. Re-profile immediately afterward. Treat DirectWrite as the Windows-native end state if the deferred CJK cost remains unacceptable, not as a prerequisite for solving the current startup regression.
