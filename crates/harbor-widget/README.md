# harbor-widget

`harbor-widget` is Harbor's retained, declarative desktop UI runtime. It provides component construction, keyed reconciliation, hooks and signals, flex-based layout, input routing, theming, GPU-backed painting, and an optional `winit` native-window host.

> **Project status:** this is a pre-1.0 workspace crate developed with Harbor. The API may change. This guide assumes a workspace, path, or revision-pinned Git dependency rather than a crates.io release.

For the progressive tutorial, mental models, current control catalog, composition recipes and limitations, start at [WIDGET documentation](../../docs/widget/README.md).

## Installation

Use a workspace or path dependency while developing inside this repository:

```toml
[dependencies]
harbor-widget = { path = "../harbor/crates/harbor-widget" }
```

To use the native `winit` host, enable its feature:

```toml
[dependencies]
harbor-widget = { path = "../harbor/crates/harbor-widget", features = ["winit"] }
```

A standalone native-host application also imports the host-facing crates used by its event loop:

```toml
anyhow = "1"
pollster = "1"
winit = "0.30"
```

A Git dependency can be used by replacing `path` with this repository's Git URL and pinning a revision. Pinning is recommended while the public API is evolving.

## Features

| Feature          | Default | Purpose                                                                      |
| ---------------- | ------- | ---------------------------------------------------------------------------- |
| `winit`          | No      | Native window, input, clipboard, surface, GPU, and presentation integration. |
| `hmr`            | No      | Debug widget hot reload; implies `winit`.                                    |
| `backend-dx12`   | No      | Selects the DX12 backend policy used by Harbor's host integration.           |
| `backend-vulkan` | No      | Selects the Vulkan backend policy used by Harbor's host integration.         |

The core runtime has no default features. Enable only the native integration needed by the application.

## Quick start: build a component tree

Components implement [`Component`](src/view.rs) and return a [`View`](src/view.rs). The `view!` macro constructs child trees while preserving the same component model used by the builder APIs.

```rust
use harbor_widget::view::{BuildCx, Component, View};
use harbor_widget::widgets::text_label::TextLabel;
use harbor_widget::{Button, Column, Store, view};

#[derive(Clone, Copy)]
enum CounterAction {
    Increment,
}

#[derive(Clone)]
struct Counter {
    store: Store<u32, CounterAction>,
}

impl Component for Counter {
    fn build(&self, cx: &mut BuildCx) -> View {
        let value = *self.store.watch(cx);
        let actions = self.store.dispatcher();

        view! { Column::new().gap(8.0) => {
            TextLabel::new(format!("Count: {value}"));
            Button::text("Increment")
                .on_click(move |_| actions.dispatch(CounterAction::Increment));
        } }
    }
}
```

`Store::watch` subscribes the component to Host-published state. Button callbacks use the thread-safe `Dispatcher` to enqueue actions; after event dispatch, the application Host drains those actions, applies its reducer, and calls `store.set_state(next)` to rebuild subscribers. `BuildCx::use_state` remains available for component-local hook state.

For dynamic lists, assign stable sibling keys:

```rust
use harbor_widget::ComponentExt;

// Inside view! { ... }
// Button::text(item.label.clone()).keyed(item.id.to_string());
```

Keys are parent-local. Unkeyed siblings reconcile by position.

## Mount in a native window

The `winit` feature exposes [`WinitWindowHostBuilder`](src/winit/host.rs). A host owns one window's widget runtime, surface, input adapter, scheduler, and presentation state. Your application still owns `EventLoop`, `ApplicationHandler`, multi-window routing, and exit/fatal-error policy.

The host must be created from `ApplicationHandler::resumed`, where an `ActiveEventLoop` is available:

```rust,no_run
use harbor_widget::winit::{HostInitContext, WinitWindowHostBuilder};
use harbor_widget::widgets::text_label::TextLabel;
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

fn create_host(
    event_loop: &ActiveEventLoop,
) -> Result<harbor_widget::winit::WinitWindowHost, harbor_widget::winit::HostStartupError> {
    let builder = WinitWindowHostBuilder::new(
        Window::default_attributes().with_title("Widget example"),
        |_context: HostInitContext<'_>, _setup: &()| {
            Ok::<_, anyhow::Error>(TextLabel::new("Hello from harbor-widget"))
        },
    );

    pollster::block_on(builder.build(event_loop))
}
```

The embedding `ApplicationHandler` should then:

1. route events for the matching `WindowId` to `host.handle_window_event(&event)`;
2. inspect `HostFrameOutcome` and treat fatal frame errors according to application policy;
3. call `host.about_to_wait(Instant::now(), application_deadline)` before sleeping;
4. apply the returned `ControlFlowEffect` to the event loop; and
5. keep close requests, multi-window routing, and process exit decisions outside the host.

See [`tests/native_host_smoke.rs`](tests/native_host_smoke.rs) for a compact lifecycle example and the Harbor application host in [`../../src/shell.rs`](../../src/shell.rs) for production integration.

## Common building blocks

- **Layout:** `Row`, `Column`, `Flex`, `Flexible`, `Expanded`, `Padding`, `SizedBox`, `ConstrainedBox`, `Align`, and `Stack`.
- **Controls and editing:** `Button`, `IconButton`, `InteractiveRegion`, `MouseRegion`, `Focus`, `FocusScope`, `Shortcuts`, `Actions`, `TextEditingController`, `EditableText`, and `TextField`.
- **Presentation and overlays:** `TextLabel`, `Icon`, `DecoratedBox`, `Separator`, `ThemeProvider`, `ScrollView`, `Notification`, and `Portal`. `Paint`/`LinearGradient` support fills and border strokes; `BackdropFilter` supports local blur with optional `EdgeRefraction`.
- **State:** `BuildCx::use_state`, `Signal`, `Store`, and `Dispatcher`.
- **External rendering:** `CustomPaint` registers draw, scheduling, IME, input, and unmount callbacks for a renderer owned outside the widget crate.

Some widgets are available from `harbor_widget::widgets::<module>` rather than the crate root. The crate root re-exports the most commonly used construction, layout, control, and theme types.

`TextLabel` measures its natural width and optional `ellipsis()` against the renderer's actual UI-font glyph advances at the current DPI. It adds no implicit padding; use `Padding` explicitly. Truncation preserves grapheme boundaries and the full source for later wider layouts. Without initialized text resources, headless layout uses the same per-Unicode-scalar fallback as text rendering, not UTF-8 byte counts.

## Runtime without `winit`

Use [`runtime::Runtime`](src/runtime/mod.rs) directly when another host already owns the window or event loop. The embedding host is responsible for:

- setting the root component and viewport;
- dispatching normalized `UiEvent` values;
- calling runtime update/layout work;
- rendering the produced scene with the widget renderer; and
- applying `RuntimeEffects` such as redraw, cursor, IME, clipboard, and control-flow requests.

Prefer `WinitWindowHost` unless you need a custom platform adapter.

## Relationship to Harbor Terminal

`harbor-widget` is terminal-agnostic and does not depend on `harbor-terminal`. Harbor embeds its terminal renderer through `CustomPaint`; the application-owned bridge translates widget allocation, input, IME, and scheduling contracts into terminal operations.

See the [widget concepts](../../docs/widget/concepts.md) and [composition recipes](../../docs/widget/recipes.md) for runtime ownership and integration guidance.


## License

Licensed under the [Apache License 2.0](../../LICENSE).

## Acknowledgements

Thanks to [Glass HQ's liquid-glass](https://github.com/Glass-HQ/liquid-glass) for the glass material work adapted by Harbor, including its edge-refraction, rim-light and frost/tone algorithms (revision `a4331da89ee47bcaaef3276de522257f37b0ee26`).

<details>
<summary>MIT license for the adapted material code</summary>

MIT License

Copyright (c) 2026 Glass HQ

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

</details>
