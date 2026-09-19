# harbor-terminal

`harbor-terminal` is Harbor's host-neutral terminal engine. It combines incremental ANSI/VT parsing, screen and scrollback state, terminal input encoding, PTY I/O ownership, selection and pointer behavior, frame scheduling, and an optional wgpu render pipeline.

The crate deliberately does **not** depend on `harbor-widget`, `winit`, or native window/composition APIs. An application host chooses the PTY implementation, maps platform input into terminal events, schedules frames, and embeds rendering in its UI.

> **Project status:** this is a pre-1.0 workspace crate developed with Harbor. The API may change. This guide assumes a workspace, path, or revision-pinned Git dependency rather than a crates.io release.

## Installation

Use a workspace or path dependency while developing inside this repository:

```toml
[dependencies]
harbor-terminal = { path = "../harbor/crates/harbor-terminal" }
```

A live rendered terminal also needs compatible PTY, font, and GPU setup. In this repository those capabilities are supplied by `harbor-pty`, `harbor-text`, and `wgpu`.

## Quick start: parse terminal output without a PTY or GPU

Headless mode is useful for parser integration, snapshots, tests, and tools that only need terminal state:

```rust
use harbor_terminal::{Terminal, TerminalOutputEvent};

let mut terminal = Terminal::new_headless(3, 20);
terminal.put_bytes(b"hello\r\n\x1b[31mred\x1b[0m");

assert_eq!(terminal.row_text(0), "hello               ");
assert_eq!(terminal.screen().cell(1, 0).ch, 'r');

terminal.put_bytes(b"\x1b]0;build log\x07");
for event in terminal.drain_output_events() {
    if let TerminalOutputEvent::TitleChanged(title) = event {
        println!("title: {title}");
    }
}
```

Use `put_bytes` when bytes should be parsed exactly as supplied. Use `process_output` for host-driven output that should first snap normal-buffer scrollback to the bottom unless that behavior has been suppressed.

## Screen state and snapshots

`Terminal::screen()` gives read-only access to the live `Screen`. `Terminal::snapshot()` returns a GPU-independent `TerminalSnapshot` containing the visible cells, cursor state, scroll position, input modes, and dirty ranges.

```rust
use harbor_terminal::Terminal;

let mut terminal = Terminal::new_headless(2, 8);
terminal.put_str("abc");

let snapshot = terminal.snapshot();
assert_eq!((snapshot.rows, snapshot.cols), (2, 8));
assert_eq!(snapshot.cell_char(0, 1), 'b');
```

If the terminal owns a live PTY reader, `drain_and_snapshot()` first consumes currently queued output. Parser side effects such as title changes, working-directory metadata, shell-integration markers, clipboard requests, and terminal replies are returned separately by `drain_output_events()`.

## Create a live rendered terminal

A live terminal is normally constructed on the UI/render thread with [`Terminal::try_new_with_appearance_from_endpoints`](src/lib.rs). The constructor keeps the PTY endpoint bundle intact until GPU renderer creation succeeds, preserving safe teardown on startup failure.

The required inputs are:

| Input                        | Purpose                                                                                                            |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `TerminalSize`               | Initial rows and columns. Use `Terminal::terminal_size_for` to derive it from surface dimensions and text metrics. |
| `PtyEndpoints`               | Reader, writer, and lifecycle/resize control from `harbor-pty`.                                                    |
| `TerminalGpuAccess`          | Borrowed wgpu device, queue, target format, and upload policy.                                                     |
| Surface size                 | Current physical target size in pixels.                                                                            |
| `FontBook` and `TextMetrics` | Glyph fallback resources and fixed-cell measurements from `harbor-text`.                                           |
| `TerminalAppearance`         | Palette and default-background tint policy.                                                                        |
| Wake callback                | Notifies the host when the PTY reader has queued output or disconnected.                                           |

If startup has already split PTY ownership into reader, writer, and `PtyControl`, `Terminal::new` and `Terminal::new_with_appearance` provide the lower-level constructors.

The integrated live PTY path is currently Windows-first and uses ConPTY through `harbor-pty`. The headless parser and screen model remain useful independently of native window hosting.

## Feed input

Map host keyboard, pointer, focus, and IME events into `TerminalEvent`. The terminal encodes protocol bytes using the active modes parsed from the child process:

```rust,no_run
use harbor_terminal::{
    Terminal, TerminalEvent, TerminalKey, TerminalKeyboardEvent, TerminalModifiers,
};

fn send_enter(terminal: &mut Terminal) -> anyhow::Result<()> {
    terminal.handle_event(TerminalEvent::Keyboard(
        TerminalKeyboardEvent::KeyDown {
            key: TerminalKey::Enter,
            modifiers: TerminalModifiers::default(),
        },
    ))
}
```

Use `handle_event_with_outcome` when the host also needs redraw, pointer-capture, or pointer-release effects. Raw input can be sent with `write_pty`, but structured events are preferred because they honor terminal modes such as application cursor keys, bracketed paste, mouse tracking, and focus reporting.

## Schedule and render frames

The host should query frame demand before sleeping:

```rust,no_run
use harbor_terminal::Terminal;
use std::time::Instant;

fn update_schedule(terminal: &mut Terminal) {
    let demand = terminal.frame_demand(Instant::now());
    if demand.redraw_now {
        // Ask the native host for a redraw.
    }
    if let Some(deadline) = demand.deadline {
        // Wake no later than `deadline` for cursor blink or auto-scroll.
    }
    // `ordinary_present_eligible` is false while synchronized output defers
    // an ordinary presentation.
}
```

During a wgpu render pass, call `Terminal::render` with the terminal's allocation inside the full surface:

```rust,ignore
let target = RenderTarget::new_with_scale(
    (allocation_x, allocation_y),
    (allocation_width, allocation_height),
    (surface_width, surface_height),
    scale_factor,
);

terminal.render(
    target,
    &mut render_pass,
    TerminalGpuAccess::new(device, queue, surface_format),
);
```

`render` drains queued PTY output, updates the grid when allocation geometry changes, prepares damaged GPU resources, and draws the terminal. `draw_retained` can replay the last committed buffers when the host intentionally skips live preparation; it automatically falls back to a live render when geometry changed.

Harbor's reference integration is in [`../harbor-app/src/terminal_view.rs`](../harbor-app/src/terminal_view.rs) and [`../../src/tab_coordinator.rs`](../../src/tab_coordinator.rs).

## PTY lifecycle and threading

- `Terminal` owns the PTY reader thread, input writer, and `PtyControl` for the lifetime of a live session.
- The reader callback should only wake the application; parsing and screen mutation occur when the owning thread drains output.
- Dropping the terminal tears down its PTY resources. Keep terminal ownership aligned with the tab/session lifecycle.
- `resize` and `resize_if_changed` update both the PTY and the in-memory grid. Failed PTY resizing leaves the screen geometry unchanged.

## API map

- **Parsing and state:** `Terminal`, `TerminalParser`, `Screen`, `TerminalSnapshot`, `Cell`, `CellAttrs`, and `InputModes`.
- **Input:** `TerminalEvent`, `TerminalKeyboardEvent`, `TerminalPointerEvent`, `TerminalFocusEvent`, `TerminalKey`, and `TerminalModifiers`.
- **Host effects:** `TerminalEventOutcome`, `TerminalOutputEvent`, `FrameDemand`, and working-directory/shell-integration metadata.
- **Rendering:** `RenderTarget`, `RenderViewport`, `TerminalGpuAccess`, `TerminalRenderPipeline`, and `UploadPolicy`.
- **Selection and scrolling:** `PointerInteraction`, `SelectionModel`, `SelectionRange`, and `AutoScroll`.
- **Appearance:** `TerminalAppearance`, `Color`, and the active screen palette.

## Relationship to `harbor-widget`

The crates meet at an application-owned bridge rather than depending on each other:

```text
native host / application policy
  -> harbor-widget CustomPaint allocation, input, IME, and scheduling
    -> application terminal bridge
      -> harbor-terminal PTY, screen, parser, and renderer
```

This boundary lets `harbor-terminal` be embedded in another UI toolkit or custom compositor. See [the widget runtime architecture](../../docs/architecture/widget-runtime.md) for Harbor's implementation.

## Development

From the repository root:

```bash
cargo test -p harbor-terminal
cargo clippy -p harbor-terminal --all-targets -- -D warnings
```

Protocol support and known gaps are tracked in the [terminal protocol checklist](../../docs/protocol/checklist.md).

## License

Licensed under the [Apache License 2.0](../../LICENSE).
