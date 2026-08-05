# ⚓ Harbor

🚀 Harbor is a Windows-first, GPU-accelerated terminal emulator written in 🦀 Rust with winit, wgpu, a custom VT parser, and a declarative GPU widget runtime.

🎯 The current development priority is terminal correctness and Windows stability. Unix PTY support is intentionally deferred until the Windows feature set, performance, and daily-use workflows are stable.

## ✨ Current Capabilities

- 🧠 Incremental ECMA-48/DEC parser with bounded CSI and string states
- 🖥️ Cell grid with SGR, scroll regions, margins, alternate screen, protected cells, and scrollback
- 🎨 Custom wgpu renderer with glyph atlas, incremental damage uploads, decorations, cursor styles, selection, and scrollbar
- 🪟 Windows ConPTY integration
- ⌨️ Application cursor/keypad modes, function and editing keys, bracketed paste, clipboard, and paste confirmation
- 🧩 Declarative `harbor-widget` runtime with retained scenes, event routing, focus, `CustomPaint`, and winit frame integration
- 🔤 DirectWrite primary-font selection and system fallback

🚧 Known gaps include terminal replies, several OSC families, mouse and focus protocols, IME preedit display, wide-cell normalization in some editing operations, combining marks, box-drawing alignment, configuration, search, packaging, and runtime performance evidence.

## 🛠️ Build and Run

```bash
cargo run
```

🪟 Harbor currently requires Windows for an operational PTY session. `HARBOR_FONT` may point to a font file used as the primary DirectWrite face.

## ✅ Quality Gates

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
```

📊 Heap profiling:

```bash
cargo run --profile dhat --features dhat-heap
```

## 🏗️ Architecture

```text
winit events
    -> harbor-widget WinitAdapter
    -> Runtime event routing and frame scheduling
    -> Terminal CustomPaint
       -> harbor-parser
       -> terminal screen and input model
       -> Windows ConPTY
       -> wgpu terminal renderer
```

The application owns windows and long-lived GPU resources. The feature-gated widget winit integration borrows those resources per frame to acquire, encode, submit, and present. Each OS window has an independent widget runtime.

## 📚 Documentation

Start with [`docs/README.md`](docs/README.md).

- [`docs/roadmap.md`](docs/roadmap.md) — priorities, phases, and release gates
- [`docs/protocol/checklist.md`](docs/protocol/checklist.md) — protocol coverage source of truth
- [`docs/architecture/widget-runtime.md`](docs/architecture/widget-runtime.md) — current widget runtime architecture
- [`docs/performance/`](docs/performance/) — memory evidence, profiling procedure, and remaining optimization work

🧭 Detailed architectural decisions and completed implementation records live under [`.grimoire/`](.grimoire/).

> ⚓ Build a reliable terminal first. Turn it into a development environment later.
