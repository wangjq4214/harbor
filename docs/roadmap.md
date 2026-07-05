# WGPU Terminal Emulator Roadmap

## Overall Goal

Build a high-performance terminal emulator with Rust + winit + wgpu + a custom text renderer.

The project follows an Alacritty-like product boundary:

- fast terminal core
- GPU-accelerated rendering
- strong terminal correctness
- minimal built-in UI features
- extensible architecture for future dev-tool or agent integration

Core milestones:

- v0.1: Minimal end-to-end terminal loop ✅
- v0.2: Terminal core (parser + state + SGR)
- v0.3: Cell-based wgpu renderer (color + decorations)
- v0.4: Interactive features
- v0.5: Performance and stability
- v0.6: Daily usable release

---

## v0.1: Minimal End-to-End Terminal Loop

> **Status: ✅ Windows complete. Unix PTY is a `bail!()` stub — macOS/Linux cannot launch.**

### Done

**Window & Rendering.** winit window, wgpu surface/device/queue, resize + surface reconfigure, custom text renderer with fixed font/size/colors, full-screen redraw.

**Terminal Grid.** `Terminal` + `Cell`, rows/cols tracking, cursor position, character write with automatic wrap, `\n`/`\r`/`\x08` handling, `scroll_up` on overflow, `clear`, `resize`, renderer reads grid row-by-row.

**PTY (Windows).** Custom ConPTY wrapper, default shell (`cmd`/`powershell`/`pwsh`), background reader thread, MPSC channel to main thread, keyboard input forwarded to PTY.

**Input.** Normal text, Enter→`\r`, Backspace→`0x7f`, Tab→`\t`, Escape→`0x1b`, Ctrl+letter→control char, arrow keys→CSI sequences.

**Minimal Parser.** Custom parser (not vte), printable chars, `\n`/`\r`/`\b`, basic clear screen, unknown escapes silently ignored.

**Acceptance.** `cargo run` opens a window, commands + output work, resize does not crash — all verified on Windows.

### To Do

- [ ] Unix PTY implementation (macOS/Linux support)
- [ ] macOS/Linux runtime acceptance verification

---

## v0.2: Terminal Core (Parser + State + SGR)

> **Status: 🟡 Cell model, SGR, grid editing, alt screen, DECSTBM, and cursor state all done (133 tests pass). Parser hardening and runtime verification pending (needs Unix PTY).**

### Done

**Cell Model.** `Cell` extended with `fg: Color`, `bg: Color`, `attrs: CellAttrs`. `Color` enum: Named(8), Bright, Indexed(256), Rgb. `CellAttrs` bitset: bold, dim, italic, underline, blink, inverse, strikethrough. Default fg/bg support, empty cell (space + defaults).

**SGR.** Full `CSI m` dispatcher: reset (0), bold (1), dim (2), italic (3), underline (4), blink (5), inverse (7), strikethrough (9), 8-color fg (30–37) / bg (40–47), bright fg (90–97) / bg (100–107), 256-color fg (`38;5;N`) / bg (`48;5;N`), truecolor fg (`38;2;R;G;B`) / bg (`48;2;R;G;B`). All tested.

**Grid Editing.** ECH (CSI X), ICH (CSI @), DCH (CSI P), IL (CSI L), DL (CSI M), SU (CSI S), SD (CSI T). All respect scrolling region.

**Scrolling Region.** DECSTBM (`CSI r`), scroll/insert/delete operations respect top/bottom margins. Unit-tested with vim-like scenarios.

**Cursor.** Move up/down/left/right (CSI A/B/C/D), set position (CSI H/f), clamp to bounds, save/restore (ESC 7/8 / DECSC/DECRC). Cursor show/hide (`?25h/l`) intentionally no-op (verified by test).

**Alternate Screen.** Enter/exit (`CSI ?1049h/l`), main screen state preserved on enter and restored on exit. Isolation unit-tested, idempotent re-entry handled.

**Resize.** Content preserved, rows/cols updated, PTY synced, cell metrics recalculated, redraw requested, cursor clamped. Tested.

**Tests.** 133 tests pass: SGR (all color modes + attribute combinations), ICH/DCH, IL/DL, alt screen, cursor save/restore, cursor show/hide (no-op verified), SU/SD, scroll region, resize, CJK wide chars, dirty-row tracking.

### To Do

**Parser Hardening**
- [x] Log unsupported sequences via `tracing::warn!` instead of silent ignore
- [x] Validate CSI parameter bounds (reject malformed sequences gracefully)
- [ ] Parse private mode sequences (`CSI ? ...`) beyond `?1049h/l` for cursor and DEC modes

**Unit Test Coverage**
- [ ] CSI cursor-movement sequences (A/B/C/D) tested at parser level

**Acceptance (runtime — needs Unix PTY for full verification)**
- [ ] `vim` / `less` scroll correctly at runtime
- [ ] `clear` works at runtime
- [ ] `cargo build` output does not corrupt the screen
- [ ] Shell adapts to resized rows/cols at runtime
- [ ] Alternate screen enter/exit works with `less` / `vim` at runtime

---

## v0.3: Cell-Based WGPU Renderer (Color + Decorations)

> **Status: 🟡 Background rects, glyph tint, atlas eviction, decorations (underline/strikethrough/bold/italic/inverse) done. Cursor styles (beam/underline), combining marks, and viewport offset pending.**

### Done

**Rendering Architecture.** Glyph atlas pipeline, CursorLayer, background clear + background rect pipeline + glyph pass + cursor pass drawn in order within single render pass.

**Background Pipeline.** Solid-color quad shader, pre-allocated vertex buffer (one rect per cell), single draw call, default-bg cells produce degenerate quads (skipped by rasterizer).

**Glyph Color.** Fragment shader multiplies white glyph alpha by vertex color, colored vertices generated per cell (position + UV + fg RGBA), default-fg cells use white.

**Glyph Atlas.** `HashMap<char, AtlasGlyph>` cache, atlas texture built once, lazy rasterization on first appearance, reuse across frames. Atlas-full handled via eviction + `full_update` (warn if still too large). Incremental tile uploads.

**Cell-Based Rendering.** Cell-by-cell read, glyph instance per visible cell, dynamic buffer upload via `write_buffer`, batched single draw call, pixel-coordinate conversion, `TEXT_PADDING = 16.0`.

**Font Metrics.** Cell width/height/baseline/ascent calculated via fontdue, characters and cursor aligned to cell grid.

**Cursor.** Block cursor (rasterized glyph), 530ms blink cycle with `CursorBlink` state machine, hidden state driven by blink phase.

**Unicode.** `unicode-width` integrated, wide chars occupy 2 cells, second cell marked `wide_continuation`, spacer cleaned on delete, overwrite handled, unsupported chars → U+FFFD replacement glyph, CJK glyphs rasterized from fallback font.

**Rendering Correctness.** SGR fg rendered via `cell.fg.to_rgba()` in glyph shader, SGR bg rendered via background rect pipeline with `cell.bg.to_rgba()`.

**Decorations.** `DecorationLayer` GPU pipeline (follows BackgroundLayer pattern), separate underline/strikethrough vertex buffers, degenerate-quad skipping for undecorated cells. `TextMetrics` extended with underline/strikethrough position/thickness from font descent. Bold→white glyph color, italic→rightward shift (15% cell width), inverse→fg↔bg swap (glyph and background). Full render order: clear → bg → glyphs → cursor → decorations.

### To Do

**Decorations**
- [x] Underline / strikethrough rendering rect pipeline
- [x] Full render flow: clear → cell backgrounds → glyphs → cursor → decorations
- [x] Bold: render via white glyph color (attrs stored, now rendered)
- [x] Italic: render via rightward shift (attrs stored, now rendered)
- [x] Underline: render underline decoration (attrs stored, now rendered)
- [x] Inverse: swap fg/bg of rendered cell (attrs stored, now rendered)

**Cursor Styles**
- [ ] Beam cursor (thin vertical bar)
- [ ] Underline cursor
- [ ] Focused/unfocused cursor distinction
- [ ] Inverse cursor rendering (swap fg/bg of cells under cursor)

**Glyph Atlas**
- [ ] Clear atlas when font size or DPI scale changes

**Cell-Based Rendering**
- [ ] Viewport offset (for scrollback rendering)

**Font Metrics**
- [ ] Box drawing characters (`│` `─` `┌` …) aligned for seamless joins
- [ ] Stable underline position

**Unicode**
- [ ] Zero-width combining marks (render base char + combine, or preserve grid integrity)

**Acceptance**
- [ ] `ls --color` renders with correct colors
- [ ] `vim` syntax highlighting colors are correct
- [ ] `htop` / `top` displays colored output correctly
- [ ] Cursor position is accurate
- [ ] Cell alignment is accurate
- [ ] Text layout remains correct after resize
- [ ] Chinese characters do not severely corrupt the grid
- [ ] Large output does not noticeably freeze

---

## v0.4: Interactive Features

> **Status: 🔴 Not started.**

### Scrollback
- [ ] Add scrollback buffer (ring buffer)
- [ ] Normal screen output enters scrollback
- [ ] Alternate screen does not enter scrollback by default
- [ ] Mouse wheel enters history view
- [ ] PageUp / PageDown for scrollback navigation
- [ ] Home / End to top/bottom of scrollback
- [ ] Limit maximum scrollback lines
- [ ] Configurable scrollback line count
- [ ] Copy text from scrollback

### Selection
- [ ] Mouse drag selection
- [ ] Click to set selection start, drag to update range
- [ ] Keep selection after mouse release
- [ ] Select text across multiple lines
- [ ] Double-click to select word
- [ ] Triple-click to select line
- [ ] Clear selection
- [ ] Render selection highlight
- [ ] Auto-scroll while selecting

### Clipboard
- [ ] Integrate system clipboard
- [ ] Ctrl+Shift+C copies selected text
- [ ] Ctrl+Shift+V pastes
- [ ] Command+C / Command+V on macOS
- [ ] Bracketed paste mode
- [ ] Handle newlines during paste
- [ ] Filter dangerous control characters when necessary

### Mouse Protocol
- [ ] Mouse wheel sent to application in alternate screen
- [ ] Basic mouse reporting
- [ ] SGR mouse mode
- [ ] Mouse modifier encoding

### IME
- [ ] Integrate winit IME events
- [ ] Support preedit/composition
- [ ] Support committed text
- [ ] Keep IME candidate window near cursor
- [ ] Do not write composition text directly to PTY
- [ ] Write committed text to PTY

### Keyboard
- [ ] Add keybinding data structure
- [ ] Ctrl+C copies when selection exists, sends SIGINT otherwise
- [ ] Ctrl+Shift+C always copies
- [ ] Ctrl+Shift+V pastes
- [ ] Ctrl+Plus / Ctrl+Minus / Ctrl+0 for font zoom
- [ ] F11 toggles fullscreen
- [ ] Escape behaves correctly (mapped to `0x1b`)
- [ ] F1-F12, Home/End/PageUp/PageDown mappings correct

### Hyperlink
- [ ] Detect URLs
- [ ] Highlight URL on hover
- [ ] Ctrl+Click opens URL
- [ ] Support OSC 8 hyperlink, optional

### Acceptance Criteria
- [ ] Text selection works
- [ ] Copy/paste works
- [ ] Chinese IME works
- [ ] Mouse wheel scrolls scrollback
- [ ] Mouse works basically in vim/tmux
- [ ] Shortcuts do not conflict with shell control characters
- [ ] Multi-line paste works correctly with bracketed paste

---

## v0.5: Performance and Stability

> **Status: 🟡 Row-level damage tracking, incremental renderer updates, PTY batching, and basic surface/process error handling done. Latency measurement, benchmarking, memory optimization, and advanced stability pending.**

### Done

**Damage Tracking.** `Screen::dirty_rows` provides row-level dirty tracking — `mark_row_dirty()` called during write_char, erase, newline, scroll, insert/delete lines. Full damage on resize (`vec![true; rows]`). TextLayer and BackgroundLayer use dirty_rows for incremental row uploads instead of full rebuild.

**Rendering Optimization.** Render only visible area; pre-allocated instance buffers in `new()` updated via `write_buffer`; incremental atlas update (only rasterize new chars); batch atlas uploads in single `prepare` call; pipeline and bind groups created once (rebuilt only on resize).

**PTY / Parser Performance.** 4096-byte reader buffer, PTY bytes processed in chunks, one redraw per chunk (not per-byte), reader thread failure detected and logged.

**Thread Safety.** Terminal state mutation on UI thread only; renderer does not hold mutable reference to Terminal.

**Stability.** wgpu surface Lost/Outdated handled (logged + reconfigured); PTY child exit detected (reader exits on EOF); structured `tracing` logs in JSON format.

**Memory.** `Cell` representation compact (fixed-size fields, no heap indirection).

### To Do

**Damage Tracking**
- [ ] Formal `DamageTracker` struct (currently `dirty_rows: Vec<bool>`)
- [ ] Cell-level damage granularity

**Rendering Optimization**
- [ ] Reduce temporary `Vec` allocations per frame (build_row_vertices allocates per-row)
- [ ] Track atlas hit rate
- [ ] Frame coalescing during heavy PTY output

**PTY / Parser Performance**
- [ ] Limit redraw frequency during heavy output
- [ ] Backpressure strategy

**Latency**
- [ ] Record key input → PTY write → output receive → render timestamps
- [ ] Measure input-to-present latency
- [ ] Request redraw immediately after input
- [ ] Avoid lock contention

**Benchmark**
- [ ] `yes` throughput
- [ ] `cat large_file`
- [ ] `git log --graph --color=always`
- [ ] `vtebench`
- [ ] `vim` redraw test
- [ ] Record FPS, frame time, CPU, memory, atlas hit rate

**Memory Optimization**
- [ ] Ring buffer for scrollback
- [ ] Limit maximum scrollback lines
- [ ] Compact `Color` / `Attrs` representation
- [ ] Avoid frequent per-line `String` allocation
- [ ] Avoid cloning the whole grid
- [ ] Clear snapshot/diff mechanism

**Stability**
- [ ] Handle wgpu out-of-memory
- [ ] Handle device lost
- [ ] Handle shell crash (reader restart logic)
- [ ] Panic hook logging
- [ ] Optional debug overlay (FPS, frame time, glyph count, atlas usage, PTY bytes/sec)

**Acceptance**
- [ ] UI does not visibly freeze during heavy output
- [ ] Input latency is low
- [ ] CPU usage is acceptable
- [ ] Memory does not grow without bound
- [ ] Renderer no longer rebuilds all resources every frame
- [ ] Key benchmarks are recorded
- [ ] Crashes and errors produce useful logs

---

## v0.6: Daily Usable Release

> **Status: 🔴 Not started.** Target: dogfood-quality daily driver with config, themes, search, packaging.

### Config System
- [ ] Add config file (TOML)
- [ ] Support default config + user config path
- [ ] Report config parse errors clearly
- [ ] Support config hot reload
- [ ] Font family / size / line height config
- [ ] Color / theme / background opacity config
- [ ] Window padding config
- [ ] Shell / working directory config
- [ ] Scrollback line count config
- [ ] Cursor style config
- [ ] Keybindings config
- [ ] Window startup size / decorations config

### Themes
- [ ] Built-in default theme
- [ ] 16-color palette + bright colors
- [ ] Foreground / background / cursor / selection / search colors
- [ ] Load external theme files
- [ ] Theme hot reload

### Search
- [ ] Search scrollback and current screen
- [ ] Highlight search matches
- [ ] Next / previous match
- [ ] Case-sensitive option

### Window / Platform
- [ ] Windows basic support (ConPTY + window + renderer)
- [ ] macOS basic support (font loading OK; PTY stub)
- [ ] Linux X11 basic support (font loading OK; PTY stub)
- [ ] Linux Wayland basic support
- [ ] Correct DPI scaling + multi-monitor DPI switching
- [ ] Window title updates (via OSC)
- [ ] Configurable shell working directory
- [ ] CLI argument to start a specific command
- [ ] Transparent background support
- [ ] Window icon
- [ ] Close confirmation, optional

### Shell Integration
- [ ] OSC title update
- [ ] OSC 8 hyperlink
- [ ] Working directory tracking, optional
- [ ] Shell prompt marker, optional

### Logging and Diagnostics
- [ ] `RUST_LOG` controls logging (EnvFilter, not fixed level)
- [ ] Log file path
- [ ] Print version, platform, wgpu backend, adapter on startup
- [ ] Print config load path and PTY shell
- [ ] Toggle performance stats overlay
- [ ] Debug overlay: FPS, frame time, glyph count, atlas usage, damage rows, PTY bytes/sec

### Packaging and Release
- [ ] Windows `.exe`
- [ ] macOS `.app`
- [ ] Linux tarball
- [ ] GitHub Release workflow
- [ ] CI: `cargo check`, `cargo test`, `clippy`, `fmt`
- [ ] Version management
- [ ] Changelog
- [ ] README
- [ ] Basic usage guide
- [ ] Example config

### Dogfood
- [ ] Use continuously for 1 day
- [ ] Use continuously for 1 week
- [ ] `nvim` / `tmux` / `git` / `cargo build` work
- [ ] Chinese IME works
- [ ] Copy/paste works
- [ ] Heavy output works
- [ ] Crash frequency is acceptable

### Acceptance Criteria
- [ ] Usable as personal daily terminal
- [ ] Config file works
- [ ] Themes work
- [ ] Search works
- [ ] At least one platform is stable (Windows)
- [ ] Other target platforms can build (macOS/Linux PTY)
- [ ] README exists
- [ ] Example config exists
- [ ] Basic release package exists

---

## Milestone Overview

| Version | Core Goal                      | Acceptance Standard                                      | Status                                                                                                                                   |
| ------- | ------------------------------ | -------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | End-to-end terminal loop       | Open shell, type commands, display output                | ✅ Windows path done; Unix PTY stub                                                                                                       |
| v0.2    | Terminal core (parser + state) | All CSI unit tests pass (133), model state complete      | 🟡 SGR + grid editing + alt screen + DECSTBM + cursor done; parser hardening + runtime verification pending                               |
| v0.3    | Color cell renderer            | `ls --color`/`vim` syntax colors correct; atlas + cursor | 🟡 Background + glyph tint + atlas eviction + decorations done; cursor styles, combining marks, viewport offset pending                   |
| v0.4    | Interaction                    | Selection, copy/paste, IME, mouse, scrollback            | 🔴 Not started                                                                                                                            |
| v0.5    | Performance                    | Heavy output smooth, low latency, damage tracking        | 🟡 Row-level damage + incremental updates + surface/process handling done; latency measurement, benchmarking, memory optimization pending |
| v0.6    | Daily use                      | Config, themes, search, packaging, dogfood               | 🔴 Not started                                                                                                                            |
---

> Build a reliable terminal first. Turn it into a development environment later.
