# Terminal core

Harbor's terminal core turns a byte stream into retained terminal state and turns structured user input into terminal protocol bytes. It can be used without a window, PTY process or GPU. A separately owned, optional wgpu renderer projects that state into a host's render pass.

This documentation adopts the useful separation of **core, VT behavior and embedding contract** associated with Ghostty-quality terminal engineering. It does not claim Ghostty feature parity, API compatibility, performance equivalence or shared implementation. Harbor is a pre-1.0 Rust workspace project, not a stable embedding ABI or a promised crates.io release.

## Start here

| Need | Read |
| --- | --- |
| Understand text units, cells, physical/logical rows, buffers, positions and damage | [Concepts](concepts.md) |
| Build a headless consumer or supply your own native host/renderer | [Embedding](embedding.md) |
| Find common sequences, replies, examples and deliberate exclusions | [VT reference](vt.md) |
| Diagnose TERM, ConPTY, WSL, SSH or tmux differences | [Compatibility](compatibility.md) |
| Decide what terminal output may do to the host; understand actual limits | [Security](security.md) |

Related boundaries: [application](../app/README.md), [widget](../widget/README.md), [system architecture](../architecture/system.md), [target support](../targets.md). These describe the surrounding product, not extra promises made by the terminal crate.

## Minimal headless use

For an external Cargo project, use a path to your checkout (adjust the example path):

```toml
[dependencies]
harbor-terminal = { path = "../harbor/crates/harbor-terminal", default-features = false }
```

Save this complete program as that project's `src/main.rs` and run `cargo run`. It parses controlled bytes only; it starts no shell, GPU, window or clipboard effect.

```rust
use harbor_terminal::{ClipboardDelivery, Terminal, TerminalOutputEvent};
use std::time::Instant;

fn main() {
    let mut terminal = Terminal::new_headless(3, 20);
    // Pending delivery is not authorization; this consumer needs no clipboard requests.
    terminal.set_clipboard_delivery(ClipboardDelivery::Discard);
    terminal.put_bytes(b"hello\r\n\x1b[31mred\x1b[0m");

    let snapshot = terminal.snapshot();
    assert_eq!((snapshot.rows, snapshot.cols), (3, 20));
    assert_eq!(snapshot.cell_char(1, 0), 'r');
    assert_eq!(terminal.screen().cell(1, 2).raw_text(), "d");

    terminal.put_bytes(b"\x1b]0;controlled log\x07");
    assert_eq!(
        terminal.drain_output_events(),
        vec![TerminalOutputEvent::TitleChanged("controlled log".into())]
    );

    // Reading an update does not acknowledge it. No renderer is attached here.
    let update = terminal.read_update(Instant::now());
    assert_eq!(update.snapshot.cell_char(0, 0), 'h');
    println!("{} rows, {} columns", snapshot.rows, snapshot.cols);
}
```

`new_headless` alone does not remove renderer dependencies from a default-feature build. Cargo features are additive: another workspace dependency enabling `harbor-terminal/renderer` enables it for the unified build too. See [dependency choices](embedding.md#dependency-choices) before interpreting a build as core-only proof.

The program uses the current public API and participates in the repeatable [public-guide Rustdoc checks](../README.md#rust-examples-and-api-reference). Those checks use a renderer-enabled dependency graph; use the separate core-only verification recipe in [embedding](embedding.md#dependency-choices) when that distinction matters. Example compilation/execution is not native terminal, GPU, clipboard or transport acceptance.

## What the core owns

```text
PTY/output bytes -> streaming parser -> screen + retained buffers
                                         |             |
                                  renderer update   bounded content reads
                                         |             |
                                  one projection    search / tools

host input -> TerminalEvent -> active-mode encoding -> PTY input
parser metadata / interaction outcomes -> host policy -> optional host effects
```

- `harbor-parser`: incremental UTF-8/VT state machine and bounded sequence accumulation.
- `harbor-terminal`: parser integration, cells/history, modes, selection/pointer semantics, read contracts, input encoding, timing and session adapter. Prefer `Terminal` over manually coordinating parser buffer-switch requests.
- `harbor-pty`: session endpoints and native lifecycle. The implemented live product path is Windows ConPTY; the Unix PTY implementation is a stub, not native Unix support.
- `harbor-terminal/renderer`: concrete wgpu projection, fonts/raster resources and draw encoding. The host owns the renderer, device/surface and final presentation.
- The application owns routing, tab lifetime, clipboard permission, hyperlink activation and other OS effects. The core has no `harbor-widget` or `winit` dependency.

## How to read support claims

Use four different labels rather than one "supported" badge:

1. **Implemented semantics**: a recognized sequence changes the intended state or produces a defined reply; cite current source and focused tests.
2. **Safe discard**: a family is framed/bounded and consumed without implementing its feature. Discarding graphics is not graphics support.
3. **Unsupported**: no supported contract for that operation; do not advertise it through identity, terminfo or UI. This may coexist with safe framing/discard.
4. **Runtime evidence**: an exact build, OS, application and transport exercised an observable path. Unit tests or checklist marks do not prove ConPTY, WSL, SSH, tmux or native rendering acceptance.

The [detailed VT checklist](../protocol/checklist.md) owns sequence-level status and gaps. This line is an entry point, not a duplicate checklist or a new compatibility percentage. [Validation policy](../validation.md) governs evidence; [target support](../targets.md) owns the target matrix.

## Source and existing evidence

- [Crate guide](../../crates/harbor-terminal/README.md), [manifest](../../crates/harbor-terminal/Cargo.toml), [engine facade](../../crates/harbor-terminal/src/lib.rs).
- [Core replay tests](../../crates/harbor-terminal/src/core_tests.rs), [scheduling/input tests](../../crates/harbor-terminal/src/engine_tests.rs), [public rendered boundary tests](../../crates/harbor-terminal/tests/boundary_contract.rs).
- [Session ownership](../architecture/terminal-session-ownership.md), [reference bridge](../../crates/harbor-app/src/terminal_view.rs), [product factory](../../src/tab_coordinator.rs).
- [Bounded read-model contract and historical evidence](../terminal-read-model.md), [literal-search contract and historical evidence](../terminal-literal-search.md), [current-session search integration](../current-session-search.md).
- Existing scoped runtime records: [modern SGR](../modern-sgr-acceptance.md), [OSC palette](../osc-palette-acceptance.md), [mouse](../multi-encoding-mouse-acceptance.md), [procedural box drawing](../procedural-box-drawing-evidence.md).

These are references to their original evidence, including exclusions and dates. None is rerun or broadened by adding these pages.
