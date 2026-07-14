# ⚓ Harbor — GPU-Accelerated Terminal Emulator

🎯 **Goal:** build a GPU-driven terminal emulator from scratch with 🦀 Rust + winit + wgpu.

A fast terminal core with a custom wgpu renderer (Vulkan/Metal/DX12), following an Alacritty-like product boundary: GPU-accelerated rendering, strong terminal correctness, minimal built-in UI.

## 🎨 Custom Renderer

- 📦 **Glyph atlas** with shelf packing, lazy per-glyph rasterization, incremental texture uploads
- ⚡ **Dirty-row incremental updates** — pre-allocated vertex buffers, no per-frame full rebuild
- 🧩 **Multi-layer compositing** in a single render pass: background rects → glyphs → decorations → cursor → scrollbar
- 🌈 **Color support:** 8/16/256/truecolor fg+bg, Bold / Italic / Underline / Strikethrough / Inverse
- 🖱️ **DECSCUSR cursor shapes** (block / underline / bar) with blink control
- 🔄 **Ring-buffer scrollback** (1000-line default) with GPU scrollbar + auto-hide

## 📊 Current Status

| Milestone                                           | Status                                                                                                           |
| --------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| 🏁 v0.1 End-to-end terminal loop                     | ✅ Windows complete                                                                                               |
| 🧠 v0.2 Terminal core (Parser + SGR + state machine) | 🟡 Core done, **317 tests** passing 🔬                                                                             |
| 🖌️ v0.3 Cell-based wgpu renderer                     | ✅ Color / decoration / cursor / viewport done                                                                    |
| 🚀 v0.4 Interactive features                         | 🟡 Selection (double/triple-click + auto-scroll), clipboard, keyboard mapping done. IME / mouse / paste pending 📜 |
| 🎯 v0.5 Performance & stability                      | 🟡 Damage tracking + ring buffer in place                                                                         |

We're well into **v0.4 interactive features**: multi-click selection (double/triple-click + auto-scroll), clipboard copy/paste, scrollback viewport, and full keyboard with application modes (DECCKM, DECKPAM), Home/End/Page/F1-F12 mappings are done. Next up: IME, mouse protocols, bracketed paste, focus reporting.

```bash
cargo run    # 🪟 Windows only — Unix PTY is a stub for now
```

## ✨ Features at a Glance

**Terminal Core** 🧠
- Custom ECMA-48/DEC streaming parser — full state machine covering CSI, OSC, DCS, SOS/PM/APC string families; SGR, DECSTBM, DECSCUSR, alt screen, grid editing (ICH/DCH/IL/DL/SU/SD)
- 317 unit tests covering color modes, scroll regions, cursor save/restore, CJK wide chars, parser error recovery
- CSI parameter hardening — malformed sequences rejected gracefully, unsupported sequences logged via `tracing::warn!`

**GPU Rendering** 🎨
- Full render pass: clear → cell backgrounds → glyphs → decorations → cursor → scrollbar
- Each layer owns its pipeline + vertex buffer; incremental uploads via `write_buffer`
- Font loading with fast-path candidates + fontdb fallback + automatic CJK probe

**Scrollback & Scrollbar** 📜
- Ring-buffer scrollback (O(1) on full-screen scroll, no cell copies)
- GPU-rendered scrollbar with SDF rounded-rect shader
- Auto-hide after 1500 ms inactivity, proportional thumb height
- Alt-screen isolation — `less`/`vim` scrollback stays clean

## 🏗️ Architecture

```
winit → App → UiRoot (Background + TextLayer + Decoration + Cursor + Scrollbar)
                ↕
         Terminal → Screen (NormalBuf ring buffer)
                ↕
         Parser (streaming byte-at-a-time)
                ↕
         PTY (Windows ConPTY, Unix stub)
```

## 🧪 Run Tests

```bash
cargo test   # 317 tests, all passing ✅
```

> ⚓ **Build a reliable terminal first. Turn it into a development environment later.**
