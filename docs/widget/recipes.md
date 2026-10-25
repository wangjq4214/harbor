# Composition recipes

[WIDGET home](README.md) · [Catalog](catalog.md) · [Testing](testing.md)

Each Rust block is independent. The native example opens a window; the other two run headlessly.

## 1. Mount a field in an ordinary native host

Use a path dependency with `harbor-widget`'s `winit` feature, plus `winit = "0.30"`, `pollster = "1"` and `anyhow = "1"` (as in the [crate README](../../crates/harbor-widget/README.md)). Run with a compatible GPU and interactive desktop.

This complete minimal executable owns the event loop while `WinitWindowHost` owns one window, runtime, adapter, surface and presentation lifecycle. It exits on close or a fatal frame error. Startup failure is deliberately fatal in this small example; a product should choose its own error UI/recovery policy.

```rust,no_run
use harbor_widget::{ControlFlowEffect, TextEditingController, TextField};
use harbor_widget::winit::{HostInitContext, WinitWindowHost, WinitWindowHostBuilder};
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

#[derive(Default)]
struct App { host: Option<WinitWindowHost> }

fn apply_wait(event_loop: &ActiveEventLoop, wait: Option<ControlFlowEffect>) {
    if let Some(wait) = wait {
        event_loop.set_control_flow(match wait {
            ControlFlowEffect::Wait => ControlFlow::Wait,
            ControlFlowEffect::WaitUntil(at) => ControlFlow::WaitUntil(at),
            ControlFlowEffect::Poll => ControlFlow::Poll,
        });
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.host.is_some() { return; }
        let builder = WinitWindowHostBuilder::new(
            Window::default_attributes().with_title("Harbor field"),
            |_context: HostInitContext<'_>, _setup: &()| {
                // Root construction, not repeated Component::build work.
                Ok::<_, anyhow::Error>(TextField::new(TextEditingController::new(""))
                    .placeholder("Type here"))
            },
        );
        self.host = Some(pollster::block_on(builder.build(event_loop))
            .expect("create native widget host"));
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop,
                    window_id: WindowId, event: WindowEvent) {
        let Some(host) = self.host.as_mut() else { return; };
        if host.window_id() != window_id { return; }
        if matches!(event, WindowEvent::CloseRequested) {
            self.host.take();
            event_loop.exit();
            return;
        }
        let outcome = host.handle_window_event(&event);
        apply_wait(event_loop, outcome.wait);
        if outcome.frame.as_ref().is_some_and(|frame| frame.is_fatal()) {
            self.host.take();
            event_loop.exit();
        }
        // This root has no CustomPaint and therefore no external input to drain.
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(host) = self.host.as_mut() {
            let outcome = host.about_to_wait(Instant::now(), None);
            apply_wait(event_loop, outcome.wait);
        }
    }
}

fn main() -> Result<(), winit::error::EventLoopError> {
    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut App::default())
}
```

For multiple windows, route by `WindowId`, arbitrate their wait requests (using `ControlFlowEffect::arbitrate`) and reuse `SharedGpu` where appropriate. Do not share focus/surface state between windows. The small example above is not a multi-window or mobile suspend/resume lifecycle implementation.

### Add an application controller

With the [Learn counter](learn.md), keep a Store clone outside the root, route a native event, drain/reduce pending actions, and publish new state. After publication, `host.invalidate_external(ExternalInvalidation::new())` processes build invalidation and returns a wait outcome to apply. Background dispatchers also need a host/event-loop wake (for example a winit user event); invalidation code cannot run while its thread is asleep. `host.request_frame()` alone is not a reducer.

If you embed `CustomPaint`, consume `HostEventOutcome::external_input` for that bridge unless an in-tree `.on_input(...)` callback already handled it. Keep the renderer with an application-owned lifetime; register draw/schedule/IME/input/unmount adapters rather than forking the widget event loop.

## 2. Connect an editing controller to typed intent

Keep the controller and focus handle outside repeated builds. Queue committed edits/submission/cancellation as application intent; do not execute a search while painting or during a field's build. The application decides whether intent means search, rename, filtering or something else.

```rust
use harbor_widget::{FocusHandle, Store, TextEditingController, TextField};
use harbor_widget::runtime::Runtime;
use std::time::Instant;

#[derive(Clone)]
enum Intent { Changed(String), Submit, Cancel }

fn main() {
    let inbox = Store::<(), Intent>::new(());
    let controller = TextEditingController::new("");
    let focus = FocusHandle::new();
    let changed = inbox.dispatcher();
    let submitted = inbox.dispatcher();
    let cancelled = inbox.dispatcher();
    let field = TextField::new(controller.clone())
        .focus_handle(focus.clone())
        .placeholder("Name")
        .on_change(move |text| changed.dispatch(Intent::Changed(text.to_owned())))
        .on_submit(move |_modifiers| submitted.dispatch(Intent::Submit))
        .on_escape(move || cancelled.dispatch(Intent::Cancel));

    let mut runtime = Runtime::new();
    runtime.set_root(field);
    let _ = runtime.request_focus(&focus);
    let _ = runtime.update(Instant::now());
    assert!(controller.set_text("New name"));
    assert!(inbox.drain_actions().is_empty()); // programmatic writes do not on_change
    let _ = runtime.update(Instant::now());
}
```

In a native application, call `host.request_focus(&focus)` and apply its wait result instead of discarding the runtime effects. Draining the inbox is application-owned; this snippet only verifies the programmatic-write notification boundary, not user typing or submit routing.

- `with_acceptance(initial, predicate)` can enforce an application text policy on the resulting committed string; rejected replacements leave state unchanged. Do not copy Harbor search's 128-byte budget into unrelated fields by default.
- Programmatic text/value replacement clears undo history; selection and IME preedit are not committed-change notifications. If an application-originated edit must also issue a business command, issue that intent explicitly—do not expect `on_change` to echo it.
- Public positions use UTF-8 bytes, normalized to grapheme boundaries. Use controller operations rather than byte slicing to implement user editing.
- Tagged `ClipboardEffect::Editor` results go back to `Runtime::complete_clipboard`. `WinitWindowHost` implements that path; a custom host must return text on successful reads, an empty string on successful writes, and content-free errors. Do not feed completion through generic `dispatch`, log payloads, or reuse terminal paste authorization.

See the full [input contract](../widget-input.md).

## 3. Compose a non-modal Portal with local glass

Portal children use root bounds and paint above ordinary content, escaping the declaring parent's ordinary layout slot and clips. Logical ancestry and lifetime stay with the declaring subtree. Use ordinary layout inside the Portal to position a panel; Portal itself is not hit-testable and does not request focus, dismiss on outside clicks or establish modality.

This headless composition constructs a top-right panel with real local blur, a translucent gradient fill and optional edge refraction. The values are illustrative consumer choices, **not Harbor's application search material preset**.

```rust
use harbor_widget::{Alignment, BackdropFilter, BorderRadius, BoxDecoration,
    Column, DecoratedBox, EdgeRefraction, GradientStop, LinearGradient, Paint, Portal};
use harbor_widget::layout::Point;
use harbor_widget::scene::primitive::Color;
use harbor_widget::runtime::Runtime;
use harbor_widget::{BuildCx, Component};
use harbor_widget::{Align, Padding, TextLabel};
use std::time::Instant;

fn main() {
    let highlight = Color { r: 1.0, g: 1.0, b: 1.0, a: 0.12 };
    let veil = Color { r: 0.0, g: 0.0, b: 0.0, a: 0.04 };
    let gradient = LinearGradient::new(Point::new(0.0, 0.0), Point::new(1.0, 1.0),
        &[GradientStop::new(0.0, highlight), GradientStop::new(1.0, veil)])
        .expect("valid gradient");
    let radius = BorderRadius::all(12.0).expect("valid radius");
    let decoration = BoxDecoration::new().try_paint(Paint::from(gradient))
        .expect("valid paint").border_radius(radius);
    let panel = BackdropFilter::new(8.0).expect("valid blur")
        .border_radius(radius)
        .edge_refraction(EdgeRefraction::new(10.0, 2.0).expect("valid lens"))
        .child(DecoratedBox::new(decoration)
            .child(Padding::all(12.0).unwrap().child(TextLabel::new("Saved locally"))));
    let overlay = Portal::new().child(Align::new(harbor_widget::Alignment2D::TOP_RIGHT).insets(harbor_widget::Insets { top: 12.0, right: 12.0, bottom: 12.0, left: 12.0 }).unwrap()
        .child(panel));
    let mut runtime = Runtime::new();
    runtime.set_root(move |cx: &mut BuildCx| Column::new()
        .push_child(TextLabel::new("Ordinary background content"))
        .push_child(overlay.clone()).build(cx));
    let _ = runtime.update(Instant::now());
}
```

The sample constructs descriptions only; it does not render a screenshot. A real host must encode through `Runtime::encode_frame` (the winit host already does) to sample the actual previously painted background, including embedded terminal draws. Uniform backgrounds naturally show less blur/refraction detail. Foreground text stays sharp; the filter is not a snapshot texture and does not move hit-test geometry.

### Optional named material inputs

To opt into frost/tone and a geometric rim, attach a material to the filter:

```rust
use harbor_widget::{BackdropMaterial, BackdropRim, BackdropTone};

let material = BackdropMaterial::new(
    4.0, -0.1,
    BackdropTone { offset: 0.0, slope: 1.0, quadratic: 0.0, saturation: 1.02 },
    BackdropRim { highlight: 0.30, shade: 0.22 },
).expect("finite material inputs");
// filter.material(Some(material)); use None for the original blur/refraction path.
```

These are illustrative values, not a library product preset. Tone and rim take named
structs, not positional arrays. Every field is checked by `BackdropMaterial::new`,
returning a recoverable `Result` with the actual invalid field and rule.
All inputs must be finite; sigma, saturation and rim must also be nonnegative.
Negative mix and tone coefficients remain legal. Valid inputs keep their caps:
fill/blur sigma 64dp, mix [-1,1], coefficients [-2,2], saturation 2, rim 1;
refraction width/displacement retain 64dp/32dp caps. Above-cap finite values
are clamped rather than rejected. No default material is implicitly enabled.
The read-only `frost()/tone()/rim()` arrays report effective renderer packing;
they are not positional configuration entry points. Application-owned recipes,
background sampling, foreground, layout and hit bounds do not change.

Keep filter shape and child decoration/clipping choices explicit: filter radius does not automatically clip all child content. For a different composition, `Border::with_paint` can use the same `Paint` abstraction for a gradient stroke. No new glass-specific component or whole-window Acrylic setting is needed.

For an interactive overlay, retain focus handles, request initial focus deliberately, and decide dismissal/restoration policy. `FocusScope::modal(true)` supplies an in-runtime routing mechanism, not a general dialog product or a cross-window input gate. Harbor's native paste confirmation remains a separate application-owned window decision.
For an interactive overlay, retain focus handles, request initial focus deliberately, and decide dismissal/restoration policy. `FocusScope::modal(true)` supplies in-runtime routing, not a general dialog service or a cross-window input gate.
