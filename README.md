# ⚓ Harbor — GPU-Accelerated Terminal Console

🎯 **Goal:** build a GPU-driven GUI Agent console. For now, the focus is on the terminal emulator core — a custom terminal renderer built from scratch with 🦀 Rust + winit + wgpu.

## 🎨 Custom Renderer

A pure wgpu (Vulkan/Metal/DX12) terminal renderer pipeline:

- 📦 Custom glyph atlas with shelf packing, lazy per-glyph rasterization, incremental texture uploads
- ⚡ Pre-allocated vertex buffers + dirty-row incremental updates (no per-frame full rebuild)
- 🧩 Multi-layer compositing in a single render pass: background rects → glyphs → cursor → decorations (underline/strikethrough)
- 🌈 8/16/256/truecolor support, Bold/Italic/Underline/Strikethrough/Inverse
- 🖱️ DECSCUSR cursor shapes (block/underline/bar) with blink control

## 📊 Current Status

| Milestone                                           | Status                                        |
| --------------------------------------------------- | --------------------------------------------- |
| 🏁 v0.1 Minimal end-to-end terminal loop             | ✅ Windows complete                            |
| 🧠 v0.2 Terminal core (Parser + SGR + state machine) | 🟡 Core done, 157 tests passing                |
| 🖌️ v0.3 Cell-based wgpu renderer                     | 🟡 Color / decoration / cursor pipelines ready |
| 🚀 v0.4+ Interactive features                        | 🔴 Not started                                 |

We're at the **v0.2–v0.3 crossover** 🛤️: terminal state machine and SGR are fully implemented, GPU rendering pipelines for colors, decorations, and cursor are in place. Currently working through rendering correctness verification and polish.

```bash
cargo run    # 🪟 Windows only — Unix PTY is a stub for now
```
