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

- v0.1: Minimal end-to-end terminal loop
- v0.2: Terminal core (parser + state + SGR) 🔜
- v0.3: Cell-based wgpu renderer (color + decorations)
- v0.4: Interactive features
- v0.5: Performance and stability
- v0.6: Daily usable release

---

## v0.1: Minimal End-to-End Terminal Loop

> **Status: Windows path mostly complete; Unix PTY not implemented, runtime acceptance unverified.**
> The Windows shell loop (ConPTY → parser → screen → wgpu renderer) is wired, but cannot launch
> on macOS/Linux until the Unix PTY is implemented.

### Window and Rendering Basics

- [x] Create a window with `winit`
- [x] Initialize `wgpu` instance / adapter / device / queue / surface
- [x] Support window resize
- [x] Reconfigure surface after resize
- [x] Custom text renderer can draw a fixed string
- [x] Custom text renderer can draw multiple lines
- [x] Use a fixed font
- [x] Use a fixed font size
- [x] Use fixed foreground and background colors
- [x] Use full-screen redraw for now

### Terminal Grid

- [x] Add `Terminal`
- [x] Add `Cell`
- [x] Support `rows` / `cols`
- [x] Support `cursor_x` / `cursor_y`
- [x] Write normal characters into the current cell
- [x] Handle `\n` as line feed
- [x] Handle `\r` as carriage return
- [x] Handle `\x08` as backspace
- [x] Wrap automatically at the end of a line
- [x] Trigger `scroll_up` when writing past the last row
- [x] Support `clear`
- [x] Support `resize`
- [x] Renderer can draw text from `Terminal Grid` row by row

### PTY

- [ ] Integrate a custom ConPTY wrapper (Windows only; Unix is a `bail!()` stub)
- [ ] Start the default shell (Windows ConPTY works; Unix PTY not implemented)
- [x] Use `cmd` / `powershell` / `pwsh` on Windows
- [ ] Use `$SHELL` on macOS/Linux, fallback to `/bin/sh` (Unix PTY is a stub)
- [x] Read PTY output on a separate thread
- [x] Send PTY output to the main thread through a channel
- [x] Receive PTY bytes on the main thread
- [x] Update `Terminal` on the main thread
- [x] Write keyboard input to PTY

### Input

- [x] Send normal text input to PTY
- [x] Map Enter to `\r`
- [x] Map Backspace to `0x7f`
- [x] Map Tab to `\t`
- [x] Map Escape to `0x1b`
- [ ] Map Ctrl+C to `0x03` (no explicit handling; depends on winit `text` field)
- [ ] Map Ctrl+D to `0x04` (no explicit handling; depends on winit `text` field)
- [x] Map Up to `ESC [ A`
- [x] Map Down to `ESC [ B`
- [x] Map Right to `ESC [ C`
- [x] Map Left to `ESC [ D`

### Minimal Parser

- [x] Directly handle basic control characters (custom parser, not vte)
- [x] Support normal printable characters
- [x] Support `\n`
- [x] Support `\r`
- [x] Support `\b`
- [x] Support basic clear screen
- [x] Ignore unknown escape sequences without panicking

### Acceptance Criteria

- [x] `cargo run` opens a window on Windows (verified)
- [x] Can type commands and see output on Windows (verified)
- [x] Resize does not crash (verified)
- [ ] Unix PTY implemented so macOS/Linux can also launch

---

## v0.2: Terminal Core (Parser + State + SGR)

### Goal

Move from "can display shell output" to "parses ANSI correctly and stores all cell state" — **pixel-perfect color rendering is not required** in this milestone; colors are stored in cells and verified via unit tests.

### Terminal Grid: Color and Attributes

- [x] Extend `Cell` to include `fg: Color`, `bg: Color`, `attrs: CellAttrs`
- [x] Add `Color` enum: Named(8), Bright, Indexed(256), Rgb
- [x] Add `CellAttrs` bitset: bold, dim, italic, underline, blink, inverse, strikethrough
- [x] Support default foreground / default background
- [x] Support empty cell (space + default colors)

### SGR (Select Graphic Rendition)

- [x] Implement `CSI m` dispatcher
- [x] Reset: `0` — clears all attributes, resets to default colors
- [x] Bold: `1`
- [x] Dim: `2`
- [x] Italic: `3`
- [x] Underline: `4`
- [x] Blink: `5`
- [x] Inverse: `7`
- [x] Strikethrough: `9`
- [x] 8-color foreground (`30`–`37`)
- [x] 8-color background (`40`–`47`)
- [x] Bright foreground (`90`–`97`)
- [x] Bright background (`100`–`107`)
- [x] 256-color foreground (`38;5;N`)
- [x] 256-color background (`48;5;N`)
- [x] Truecolor foreground (`38;2;R;G;B`)
- [x] Truecolor background (`48;2;R;G;B`)
### Grid Editing

- [ ] ICH: Insert characters (CSI `@`)
- [ ] DCH: Delete characters (CSI `P`)
- [ ] IL: Insert lines (CSI `L`)
- [ ] DL: Delete lines (CSI `M`)
- [ ] SU: Scroll up (CSI `S`)
- [ ] SD: Scroll down (CSI `T`)

### Scrolling Region (DECSTBM)

- [ ] Support `CSI r` to set top/bottom margins
- [ ] Scroll/insert/delete operations respect the region
- [ ] Full-screen applications like `vim` / `less` scroll correctly

### Cursor

- [x] Move cursor up / down / left / right
- [x] Move cursor to specific row and column
- [x] Clamp cursor within terminal bounds
- [ ] Save cursor position (ESC 7 / DECSC)
- [ ] Restore cursor position (ESC 8 / DECRC)
- [ ] Cursor show/hide via `CSI ?25h` / `CSI ?25l`

### Alternate Screen

- [ ] Support normal screen and alternate screen
- [ ] Enter alternate screen (`CSI ?1049h`)
- [ ] Exit alternate screen (`CSI ?1049l`)
- [ ] Save normal screen state on enter, restore on exit
- [ ] `vim` / `less` / `top` do not pollute the main screen

### Parser Hardening

- [ ] Log unsupported sequences via `tracing::warn!` instead of silent ignore
- [ ] Validate CSI parameter bounds (reject malformed sequences gracefully)
- [ ] Parse private mode sequences (`CSI ? ...`) for cursor/dec modes

### Resize

- [x] Preserve existing content during resize
- [x] Update `Terminal rows/cols`
- [x] Sync PTY size
- [x] Recalculate cell metrics
- [x] Request redraw after resize
- [x] Clamp cursor position after resize

### Unit Test Coverage

- [ ] Every CSI sequence has a unit test
- [x] SGR test: all color modes, all attribute combinations
- [ ] ICH/DCH test: insertion/deletion shifts cells correctly
- [ ] IL/DL test: lines inserted/deleted, scroll region respected
- [ ] Alternate screen test: enter/exit preserves main screen
- [ ] Save/restore cursor test
- [ ] Cursor show/hide test
- [ ] SU/SD test
- [ ] Resize test: content preserved, cursor clamped

### Acceptance Criteria

- [ ] `cargo test` passes — all CSI sequence tests green
- [ ] `vim` can open, move cursor, and exit (colors not required)
- [ ] `clear` works correctly
- [ ] `cargo build` output does not corrupt the screen
- [ ] Shell adapts to resized rows/cols
- [ ] Alternate screen enter/exit works correctly (`less` / `vim`)


---

## v0.3: Cell-Based WGPU Renderer (Color + Decorations)

### Goal

Upgrade the existing glyph+cursor renderer to display full-color cells: background rectangles, glyph tinting, underlines, cursor styles, and viewport offset for scrollback.

### Rendering Architecture

- [x] Add glyph pipeline (single-texture glyph atlas pipeline)
- [x] Add cursor pipeline or cursor rectangle rendering (CursorLayer)
- [x] Render flow includes clear background (solid color per frame)
- [ ] **Add background rectangle pipeline** — one rect per non-default-bg cell
- [ ] **Pass cell color to glyph shader** — tint glyph white by Cell fg color
- [ ] Add underline / strikethrough rendering rect pipeline
- [ ] Render flow: clear → cell backgrounds → glyphs → cursor → decorations
- [ ] Separate background rendering from glyph rendering into distinct passes

### Background Pipeline

- [ ] Background shader (solid color quad)
- [ ] Background instance buffer (one instance per non-default-bg cell)
- [ ] Batch background drawing (single draw call)
- [ ] Support default background (skip when cell bg matches terminal bg)

### Glyph Color

- [x] Glyph atlas pipeline (existing)
- [ ] Modify fragment shader to multiply white glyph alpha by uniform/vertex color
- [ ] Generate colored glyph vertices per cell (position + UV + fg color)
- [ ] Default-foreground cells use white (current behavior preserved)

### Glyph Atlas

- [x] Build glyph cache (`HashMap<char, AtlasGlyph>`)
- [x] Build glyph atlas texture
- [x] Rasterize glyph when it first appears
- [x] Reuse glyph from atlas on later frames
- [x] Support atlas texture upload
- [ ] Handle atlas full condition (grow or evict)
- [x] Allow simple atlas rebuild
- [ ] Clear atlas when font size or DPI scale changes

### Cell-Based Rendering

- [x] Renderer reads data cell by cell
- [x] Generate glyph instance for each visible cell
- [x] Support dynamic buffer upload
- [x] Support batched drawing
- [x] Convert cell coordinates to pixel coordinates
- [ ] Support viewport offset (for scrollback rendering)
- [x] Support padding (`TEXT_PADDING = 16.0`)

### Font Metrics

- [x] Calculate cell width / cell height / baseline / ascent
- [x] Align characters to cell grid
- [ ] Keep box drawing characters (`│` `─` `┌` …) aligned for seamless joins
- [x] Align cursor rectangle to cell grid
- [ ] Keep underline position stable

### Cursor Rendering

- [x] Block cursor (rasterized `│` / `|`)
- [ ] Beam cursor (thin vertical bar)
- [ ] Underline cursor
- [x] Cursor blink state (530ms on/off cycle)
- [x] Cursor hidden state (blink-driven)
- [ ] Focused/unfocused cursor distinction
- [ ] Inverse cursor rendering (swap fg/bg of cells under cursor)

### Basic Unicode

- [x] Integrate `unicode-width`
- [x] Support wide characters occupying 2 cells
- [ ] Support zero-width combining marks (render base char + combine, or at minimum preserve grid integrity)
- [x] Mark the second cell of a wide character as spacer (`wide_continuation`)
- [x] Clean spacer when deleting a wide character
- [x] Handle overwriting spacer cells reasonably
- [x] Render unsupported characters as fallback box or replacement glyph (U+FFFD)

### Rendering Correctness

- [ ] SGR foreground color renders correctly
- [ ] SGR background color renders correctly
- [ ] Bold state is visible (brighter or thicker glyph)
- [ ] Italic state is visible (oblique or font select)
- [ ] Underline renders correctly
- [ ] Inverse renders correctly

### Acceptance Criteria

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

### Goal

Provide daily terminal interaction features: selection, copy/paste, mouse, IME, scrollback, and keyboard shortcuts.

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

### Goal

Move from usable to fast, low-latency, and stable.

### Damage Tracking

- [ ] Add `DamageTracker` (full / row / cell granularity)
- [ ] Record damage during parser updates
- [ ] Record scroll damage
- [ ] Record cursor damage on cursor movement
- [ ] Record selection damage
- [ ] Use full damage on resize
- [ ] Renderer updates instance buffer based on damage, not full rebuild
- [ ] At least row-level damage in v0.5

### Rendering Optimization

- [x] Do not render outside visible area
- [ ] Reuse glyph / background instance buffer (write_buffer instead of recreate)
- [ ] Reduce temporary `Vec` allocations per frame
- [ ] Reduce per-frame glyph lookup (incremental atlas update, not full rebuild)
- [ ] Track atlas hit rate
- [ ] Batch atlas uploads
- [x] Avoid rebuilding pipeline every frame
- [ ] Avoid rebuilding bind groups every frame
- [ ] Support frame coalescing during heavy PTY output

### PTY / Parser Performance

- [x] Use suitable PTY reader buffer size (4096)
- [x] Process PTY bytes in batches
- [ ] Request redraw once after parser batch, not per-byte
- [ ] Limit redraw frequency during heavy output
- [ ] Add backpressure strategy
- [x] Detect reader thread failure

### Latency

- [ ] Record key input → PTY write → output receive → render timestamps
- [ ] Measure input-to-present latency
- [ ] Request redraw immediately after input
- [ ] Avoid lock contention
- [x] Keep Terminal state mutation on UI thread only

### Benchmark

- [ ] `yes` throughput
- [ ] `cat large_file`
- [ ] `git log --graph --color=always`
- [ ] `vtebench`
- [ ] `vim` redraw test
- [ ] Record FPS, frame time, CPU, memory, atlas hit rate

### Memory Optimization

- [ ] Use ring buffer for scrollback
- [ ] Limit maximum scrollback lines
- [x] Compact `Cell` representation
- [ ] Compact `Color` / `Attrs` representation
- [ ] Avoid frequent per-line `String` allocation
- [ ] Avoid cloning the whole grid
- [x] Renderer does not hold mutable reference to Terminal
- [ ] Snapshot/diff mechanism is clear

### Stability

- [x] Handle wgpu surface lost (Lost/Outdated logged + reconfigured)
- [ ] Handle wgpu out-of-memory
- [ ] Handle device lost
- [x] Handle PTY child process exit (reader exits on EOF)
- [ ] Handle shell crash (reader restart logic)
- [ ] Add panic hook logging
- [x] Use structured `tracing` logs
- [ ] Optional debug overlay (FPS, frame time, glyph count, atlas usage, PTY bytes/sec)

### Acceptance Criteria

- [ ] UI does not visibly freeze during heavy output
- [ ] Input latency is low
- [ ] CPU usage is acceptable
- [ ] Memory does not grow without bound
- [ ] Renderer no longer rebuilds all resources every frame
- [ ] Key benchmarks are recorded
- [ ] Crashes and errors produce useful logs

---

## v0.6: Daily Usable Release

### Goal

Reach a state suitable for long-term dogfooding. Config, themes, search, platform integration, packaging.

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

| Version | Core Goal                      | Acceptance Standard                                      | Status                                               |
| ------- | ------------------------------ | -------------------------------------------------------- | ---------------------------------------------------- |
| v0.1    | End-to-end terminal loop       | Open shell, type commands, display output                | 🟡 Windows path OK; Unix PTY stub                     |
| v0.2    | Terminal core (parser + state) | All CSI unit tests pass; vim/less exit cleanly           | 🟡 SGR done; ICH/DCH/IL/DL/alt-screen missing        |
| v0.3    | Color cell renderer            | `ls --color`/`vim` syntax colors correct; atlas + cursor | 🔜 No background pipeline; no glyph tint               |
| v0.4    | Interaction                    | Selection, copy/paste, IME, mouse, scrollback            | 🔴 Not started                                        |
| v0.5    | Performance                    | Heavy output smooth, low latency, damage tracking        | 🔴 Not started                                        |
| v0.6    | Daily use                      | Config, themes, search, packaging, dogfood               | 🔴 Not started                                        |

---

## Development Priority

Recommended priority order:

1. ✓ **SGR + Cell colors** (v0.2) — done; unlocks `ls --color`, `vim` highlights, `htop`
2. **Background rect + glyph tint pipelines** (v0.3) — makes colors visible
3. **Alternate screen** (v0.2) — `vim`/`less` usability
4. **Unicode: zero-width combining marks** (v0.3) — correct character display
5. **Scrollback** (v0.4) — daily usability
6. **Selection + clipboard** (v0.4) — daily usability
7. **IME** (v0.4) — CJK input
8. **Performance** (v0.5)
9. **Config / theme / packaging** (v0.6)

Core principle:

> Build a reliable terminal first. Turn it into a development environment later.
