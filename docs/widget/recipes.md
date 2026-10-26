# Composition recipes

[WIDGET home](README.md) · [Catalog](catalog.md) · [Testing](testing.md)

Each Rust block is independent. The native example opens a window; the other two run headlessly.

## 1. Mount a field in an ordinary native host

Use a path dependency with `harbor-widget`'s `winit` feature, plus `winit = "0.30"`, `pollster = "1"` and `anyhow = "1"` (as in the [crate README](../../crates/harbor-widget/README.md)). Run with a compatible GPU and interactive desktop.

This complete minimal executable owns the event loop while `WinitWindowHost` owns one window, runtime, adapter, surface and presentation lifecycle. It exits on close or a fatal frame error. Startup failure is deliberately fatal in this small example; a product should choose its own error UI/recovery policy.

```rust,no_run
use harbor_widget::{ControlFlowEffect, TextEditingController, TextField, view};
use harbor_widget::winit::{GpuWake, GpuWakeEvent, HostInitContext, WinitWindowHost, WinitWindowHostBuilder};
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

struct App { host: Option<WinitWindowHost>, wake: GpuWake }

fn apply_wait(event_loop: &ActiveEventLoop, wait: Option<ControlFlowEffect>) {
    if let Some(wait) = wait {
        event_loop.set_control_flow(match wait {
            ControlFlowEffect::Wait => ControlFlow::Wait,
            ControlFlowEffect::WaitUntil(at) => ControlFlow::WaitUntil(at),
            ControlFlowEffect::Poll => ControlFlow::Poll,
        });
    }
}

impl ApplicationHandler<GpuWakeEvent> for App {
    fn user_event(&mut self, event_loop: &ActiveEventLoop, _: GpuWakeEvent) {
        if let Some(fault) = self.host.as_ref().and_then(|host| host.gpu_fault()) {
            eprintln!("fatal GPU error: {fault}");
            event_loop.exit();
        }
    }
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.host.is_some() { return; }
        let builder = WinitWindowHostBuilder::new(
            Window::default_attributes().with_title("Harbor field"),
            |_context: HostInitContext<'_>, _setup: &()| {
                // A macro View needs no wrapper Component. Mount/build/hooks stay Runtime-owned.
                Ok::<_, anyhow::Error>(view! {
                    TextField::new(TextEditingController::new("")).placeholder("Type here");
                })
            },
        ).with_gpu_wake(self.wake.clone());
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
        if outcome.fatal.is_some() || outcome.frame.as_ref().is_some_and(|frame| frame.is_fatal()) {
            self.host.take();
            event_loop.exit();
        }
        // This root has no ExternalSurface and therefore no external input to drain.
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(host) = self.host.as_mut() {
            let outcome = host.about_to_wait(Instant::now(), None);
            if let Some(fault) = outcome.fatal {
                eprintln!("fatal GPU error: {fault}");
                event_loop.exit();
                return;
            }
            apply_wait(event_loop, outcome.wait);
        }
    }
}

fn main() -> Result<(), winit::error::EventLoopError> {
    let event_loop = EventLoop::<GpuWakeEvent>::with_user_event().build()?;
    let wake = GpuWake::new(event_loop.create_proxy());
    event_loop.run_app(&mut App { host: None, wake })
}
```

Both `build` and `build_with_output` accept `IntoChildView + 'static` roots: ordinary Components, macro Views, or existing Views moved into the one-shot factory. Do not call `Component::build` in the factory to satisfy a root bound. `build_with_output` returns application bootstrap output only alongside a successfully constructed Host.

Pass an application wake deadline as the second argument to `host.about_to_wait(now, Some(deadline))`. Future Host deadlines remain `WaitUntil` demands even while the window is zero-sized/non-drawable, without acquiring/submitting a frame or polling render animation. The application performs due business work and reschedules/removes the deadline; expired Host input is ignored and does not request redraw. Runtime deadlines remain separate and normal coalesced rendering resumes on restoration. Keep independently owned business/notification timers in the application's final wait arbitration.

For multiple windows, route by `WindowId`, arbitrate their wait requests (using `ControlFlowEffect::arbitrate`) and reuse `SharedGpu` where appropriate. Do not share focus/surface state between windows. The small example above is not a multi-window or mobile suspend/resume lifecycle implementation.

### GPU notifications and lifetimes

Bootstrap requires `.with_gpu_wake(GpuWake::new(proxy))`; for a custom event type implement `From<GpuWakeEvent>` in the stable executable. Reused Hosts inherit the bootstrap Device's transport. The wake is device-level: inspect all relevant Hosts, not only a visible window. Check event/idle `fatal`, frame `FrameError::SharedGpu`, or the explicit non-frame `host.gpu_fault()` before approving business actions. Harbor cancels confirmations and exits; the widget crate never owns exit policy.

Immediately after device creation, SharedGpu installs actual lost/uncaptured-error callbacks. They retain only weak shared state, record the first categorized diagnostic, and signal the proxy once. Observations never consume/reset it. OOM, unexpected device loss, validation and internal errors are fatal; normal Destroyed notifications are ignored. Surface lost/outdated/suboptimal remain local. No Device/Queue recreation is attempted.

One joined worker per Device calls nonblocking `Device::poll(PollType::Poll)` every 50 ms, independently of UI drawability. In wgpu-core 30 this maintains native fence progress and fires deferred callbacks, even without redraw/animation/business deadlines. DX12 additionally uses a read-only `GetDeviceRemovedReason` query: wgpu-hal 30's nonblocking fence query alone does not classify the removal sentinel. Vulkan uses wgpu's fence/timeline polling and error callbacks; physical-failure detection still depends on backend/driver reporting. It does not acquire, encode, submit or present; no render-animation Poll is introduced. Backend/driver detection latency is not a completion guarantee. A requested/presented frame does **not** prove GPU execution completed; later faults arrive on a subsequent wake/Host boundary.

The last SharedGpu owner stops/unparks/joins its worker before dropping Device/Queue. No callback captures a Host, Runtime, Window or root factory; a closed proxy is harmless. Keep the proxy event conversion in the stable executable, never a reloadable generation. Borrowed external GPU users must not overwrite error/lost callbacks. Existing Runtime/root destruction before native resources and HMR release barriers are unchanged. Real driver-loss/OOM and DLL-unload evidence remain separate from harmless validation/callback tests.

### Add an application controller

With the [Learn counter](learn.md), keep a Store clone outside the root, route a native event, drain/reduce pending actions, and publish new state. After publication, `host.invalidate_external(ExternalInvalidation::new())` processes build invalidation and returns a wait outcome to apply. Background dispatchers also need a host/event-loop wake (for example a winit user event); invalidation code cannot run while its thread is asleep. `host.request_frame()` alone is not a reducer.

If you embed `ExternalSurface`, consume `HostEventOutcome::external_input` for that bridge unless an in-tree `.on_input(...)` callback already handled it. Keep the renderer with an application-owned lifetime; attach `on_draw`, `schedule_with`, `ime_with`, `on_input` and `on_unmount` adapters rather than forking the widget event loop. The schedule/IME builders configure providers, not events: scheduling is queried before idle wait selection, IME while focused, and drawing only during scene encoding with borrowed frame-scoped GPU access. Provider `Arc`/lifetime/thread bounds are unchanged; the type name does not guarantee identity across DLL generations.

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
use harbor_widget::{BackdropFilter, BorderRadius, BoxDecoration,
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

## 4. Position a dismissible notification card

The component is `NotificationCard`; detailed inputs live in
`harbor_widget::widgets::notification_card`. It displays content, but does not
decide eligibility, message meaning, deduplication, timeouts, dismissal history,
or focus policy. Those remain application responsibilities.

```rust
use harbor_widget::{NotificationCard, Portal, PositionAlignment, view};
use harbor_widget::widgets::notification_card::{
    NotificationCardPlacement, NotificationCardStyle,
};
use harbor_widget::scene::primitive::Color;

let card = NotificationCard::new("Saved locally", || { /* caller dismisses */ })
    .title("Settings")
    .close_alignment(PositionAlignment::Start)
    .detail_foreground(Color::WHITE).expect("finite detail color")
    .placement(NotificationCardPlacement {
        horizontal: PositionAlignment::End,
        vertical: PositionAlignment::Start,
        inset: 12.0,
    }).expect("finite nonnegative inset");
let card = view! { card.style(NotificationCardStyle::default()); };
let overlay = Portal::new().child(card);
```

Horizontal/vertical placement and close alignment accept only
`PositionAlignment::{Start, Center, End}`. The default close alignment is Center.
`Alignment2D` uses the same positioning-only axes; `CrossAxisAlignment::Stretch`
is exclusively for real Flex/Row/Column cross-axis stretching. There is no
conversion from the flex type to a position. Migrate any old positioning-only
Stretch call to Start explicitly.

Style, placement, detail foreground and explicit close-button style consumption
return `Result`. A style or placement with public fields is validated again at
consumption, even if its `validate()` previously succeeded. All style dimensions
and inset must be finite and nonnegative; `max_width` must be finite and strictly
positive. Optional foreground/background, detail foreground and every close
style state color must have finite RGBA components, without a [0,1] restriction.
Legal large values can still cause a layout error when combined; parent clamps
do not hide derived overflow. The default dimensions remain 520dp maximum width,
6dp radius, 12dp horizontal/bottom padding, 32dp header and 36dp close width.

Only the final `view!` node expression handles a `Result` automatically, panicking
with the field and source location on error. Intermediate fallible setters in
the example are handled explicitly; handwritten callers may recover instead.
Long details wrap and scroll unless `max_lines` selects a capped/ellipsized
presentation. Placement does not create a Portal. These headless constructions
are not native focus or GPU appearance acceptance.
