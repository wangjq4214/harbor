# Terminal core

Harbor's terminal core turns a byte stream into retained terminal state and turns structured user input into terminal protocol bytes. It can be used without a window, PTY process or GPU. A separately owned, optional wgpu renderer projects that state into a host's render pass.

Harbor is a pre-1.0 Rust workspace library. Pin the repository revision when embedding it; there is no stable C ABI.

## Start here

| Need | Read |
| --- | --- |
| Understand text units, cells, physical/logical rows, buffers, positions and damage | [Concepts](concepts.md) |
| Build a headless consumer or supply your own native host/renderer | [Embedding](embedding.md) |
| Find common sequences, replies, examples and deliberate exclusions | [VT reference](vt.md) |
| Diagnose TERM, ConPTY, WSL, SSH or tmux differences | [Compatibility](compatibility.md) |
| Decide what terminal output may do to the host; understand actual limits | [Security](security.md) |

Related guides: [application](../app/README.md) and [widget toolkit](../widget/README.md).

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
