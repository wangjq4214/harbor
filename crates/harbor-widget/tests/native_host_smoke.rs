#![cfg(all(target_os = "windows", feature = "winit"))]

use harbor_widget::layout::Size;
use harbor_widget::signal::Signal;
use harbor_widget::winit::{
    HostFrameOutcome, HostInitContext, WinitWindowHost, WinitWindowHostBuilder,
};
use harbor_widget::winit::{HostStartupStage, SharedGpu, WindowPlatformHooks, WindowSurfaceInfo};
use harbor_widget::{
    BuildCx, Component, IntoChildView, TextEditingController, TextField, View, view,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::platform::windows::EventLoopBuilderExtWindows;
use winit::window::Window;

const SECONDARY_LIFECYCLES: usize = 2;

type Trace = Rc<RefCell<Vec<&'static str>>>;

struct SetupHooks(Trace);

impl WindowPlatformHooks for SetupHooks {
    type Setup = u32;

    fn window_created(&self, _: &Window) -> anyhow::Result<u32> {
        self.0.borrow_mut().push("window");
        Ok(7)
    }

    fn surface_ready(
        &self,
        _: &Window,
        setup: &mut u32,
        _: WindowSurfaceInfo,
    ) -> anyhow::Result<bool> {
        self.0.borrow_mut().push("surface");
        *setup += 1;
        Ok(false)
    }
}

struct StatefulRoot {
    trace: Trace,
    signal: Rc<RefCell<Option<Signal<u32>>>>,
    observed: Rc<RefCell<Vec<u32>>>,
    editing: TextEditingController,
}

impl Component for StatefulRoot {
    fn build(&self, cx: &mut BuildCx) -> View {
        self.trace.borrow_mut().push("build");
        let state = cx.use_state(|| {
            self.trace.borrow_mut().push("hook");
            0_u32
        });
        self.observed.borrow_mut().push(*state.read());
        *self.signal.borrow_mut() = Some(state);
        TextField::new(self.editing.clone()).build(cx)
    }
}

fn attributes() -> winit::window::WindowAttributes {
    Window::default_attributes()
        .with_title("harbor-widget root factory contract")
        .with_visible(false)
        .with_inner_size(winit::dpi::LogicalSize::new(64.0, 64.0))
}

fn root_case<R: IntoChildView + 'static>(
    event_loop: &ActiveEventLoop,
    gpu: Option<Arc<SharedGpu>>,
    with_output: bool,
    describe: impl FnOnce(StatefulRoot) -> R,
) -> Result<WinitWindowHost, String> {
    let trace: Trace = Rc::default();
    let signal = Rc::default();
    let observed = Rc::default();
    let editing = TextEditingController::new("");
    let root = describe(StatefulRoot {
        trace: Rc::clone(&trace),
        signal: Rc::clone(&signal),
        observed: Rc::clone(&observed),
        editing: editing.clone(),
    });
    assert!(
        trace.borrow().is_empty(),
        "description must not build eagerly"
    );
    let factory_trace = Rc::clone(&trace);
    let factory = move |context: HostInitContext<'_>, setup: &u32| {
        assert_eq!(*setup, 8);
        assert_eq!(&*factory_trace.borrow(), &["window", "surface"]);
        assert!(context.text_metrics().cell_width > 0.0);
        assert_eq!(
            context.surface().physical_size,
            context.window().inner_size().into()
        );
        factory_trace.borrow_mut().push("factory");
        // Moving this root makes the factory genuinely FnOnce.
        Ok::<_, anyhow::Error>(root)
    };
    let mut host = if with_output {
        let mut builder = WinitWindowHostBuilder::new(
            attributes(),
            move |context: HostInitContext<'_>, setup: &u32| {
                factory(context, setup).map(|root| (root, String::from("bootstrap output")))
            },
        )
        .with_platform_hooks(SetupHooks(Rc::clone(&trace)))
        .focus_first(true);
        if let Some(gpu) = gpu {
            builder = builder.reuse_gpu(gpu);
        }
        let (host, output) = pollster::block_on(builder.build_with_output(event_loop))
            .map_err(|error| error.to_string())?;
        assert_eq!(output, "bootstrap output");
        host
    } else {
        let mut builder = WinitWindowHostBuilder::new(attributes(), factory)
            .with_platform_hooks(SetupHooks(Rc::clone(&trace)))
            .focus_first(true);
        if let Some(gpu) = gpu {
            builder = builder.reuse_gpu(gpu);
        }
        pollster::block_on(builder.build(event_loop)).map_err(|error| error.to_string())?
    };
    assert_eq!(
        &*trace.borrow(),
        &["window", "surface", "factory", "build", "hook"]
    );
    assert_eq!(&*observed.borrow(), &[0]);
    // A hook write must dirty the mounted Fiber, not a stub BuildCx.
    signal.borrow().as_ref().unwrap().set(9);
    host.about_to_wait(Instant::now(), None);
    assert_eq!(&*observed.borrow(), &[0, 9]);
    assert_eq!(trace.borrow().iter().filter(|&&s| s == "hook").count(), 1);
    // Initial focus/layout must route IME input without an explicit focus request.
    host.handle_window_event(&WindowEvent::Ime(winit::event::Ime::Commit(
        "mounted".into(),
    )));
    assert_eq!(editing.text(), "mounted");
    NativeHostSmoke::present(&mut host)?;
    Ok(host)
}

struct DropOutput(Rc<Cell<usize>>);
impl Drop for DropOutput {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[derive(Clone, Copy, Debug)]
enum StartupFailure {
    FactoryError,
    FactoryPanic,
    MountPanic,
}

fn startup_failures(event_loop: &ActiveEventLoop, gpu: &Arc<SharedGpu>) {
    for with_output in [false, true] {
        for failure in [
            StartupFailure::FactoryError,
            StartupFailure::FactoryPanic,
            StartupFailure::MountPanic,
        ] {
            let calls = Rc::new(Cell::new(0));
            let drops = Rc::new(Cell::new(0));
            let factory_calls = Rc::clone(&calls);
            let output_drops = Rc::clone(&drops);
            let factory =
                move |_: HostInitContext<'_>, _: &()| -> anyhow::Result<(View, DropOutput)> {
                    factory_calls.set(factory_calls.get() + 1);
                    match failure {
                        StartupFailure::FactoryError => anyhow::bail!("factory error sentinel"),
                        StartupFailure::FactoryPanic => panic!("factory panic sentinel"),
                        StartupFailure::MountPanic => Ok((
                            view! { |_: &mut BuildCx| -> View { panic!("mount panic sentinel") }; },
                            DropOutput(output_drops),
                        )),
                    }
                };
            let error = if with_output {
                pollster::block_on(
                    WinitWindowHostBuilder::new(attributes(), factory)
                        .reuse_gpu(Arc::clone(gpu))
                        .build_with_output(event_loop),
                )
                .err()
                .expect("must fail")
            } else {
                pollster::block_on(
                    WinitWindowHostBuilder::new(
                        attributes(),
                        move |cx: HostInitContext<'_>, setup: &()| {
                            factory(cx, setup).map(|(root, _)| root)
                        },
                    )
                    .reuse_gpu(Arc::clone(gpu))
                    .build(event_loop),
                )
                .err()
                .expect("must fail")
            };
            assert_eq!(calls.get(), 1);
            assert_eq!(error.stage(), HostStartupStage::RootConstruction);
            let message = match failure {
                StartupFailure::FactoryError => "factory error sentinel",
                StartupFailure::FactoryPanic => "root factory panicked: factory panic sentinel",
                StartupFailure::MountPanic => "root component panicked: mount panic sentinel",
            };
            assert!(error.to_string().contains(message), "{error}");
            assert_eq!(
                drops.get(),
                usize::from(matches!(failure, StartupFailure::MountPanic)),
                "failed mounting must drop bootstrap output, not publish it"
            );
        }
    }
}

#[derive(Default)]
struct NativeHostSmoke {
    result: Option<Result<(), String>>,
}

impl NativeHostSmoke {
    fn build_host(
        event_loop: &ActiveEventLoop,
        shared_gpu: Option<Arc<harbor_widget::winit::SharedGpu>>,
    ) -> Result<WinitWindowHost, harbor_widget::winit::HostStartupError> {
        let builder = WinitWindowHostBuilder::new(
            Window::default_attributes()
                .with_title("harbor-widget native host smoke")
                .with_visible(false)
                .with_inner_size(winit::dpi::LogicalSize::new(64.0, 64.0)),
            |_context: HostInitContext<'_>, _setup: &()| {
                Ok::<_, anyhow::Error>((
                    harbor_widget::widgets::sized_box::SizedBox::new(Size::new(64.0, 64.0))
                        .unwrap(),
                    17_u32,
                ))
            },
        );
        let result = match shared_gpu {
            Some(gpu) => pollster::block_on(builder.reuse_gpu(gpu).build_with_output(event_loop)),
            None => pollster::block_on(builder.build_with_output(event_loop)),
        };
        result.map(|(host, output)| {
            assert_eq!(output, 17);
            host
        })
    }

    fn present(host: &mut WinitWindowHost) -> Result<(), String> {
        let outcome = host.handle_window_event(&WindowEvent::RedrawRequested);
        match outcome.frame {
            Some(HostFrameOutcome::Presented { .. })
            | Some(HostFrameOutcome::PresentedSuboptimal { .. }) => Ok(()),
            Some(HostFrameOutcome::Fatal { error, .. }) => {
                Err(format!("native host frame failed: {error:?}"))
            }
            Some(other) => Err(format!("native host did not present: {other:?}")),
            None => Err("redraw event returned no frame outcome".into()),
        }
    }

    fn run(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        let mut primary = Self::build_host(event_loop, None)
            .map_err(|error| format!("primary host startup failed: {error}"))?;
        Self::present(&mut primary)?;

        for lifecycle in 0..SECONDARY_LIFECYCLES {
            let gpu = Arc::clone(primary.shared_gpu());
            let mut secondary = Self::build_host(event_loop, Some(Arc::clone(&gpu)))
                .map_err(|error| format!("secondary host startup {lifecycle} failed: {error}"))?;
            assert!(
                Arc::ptr_eq(primary.shared_gpu(), secondary.shared_gpu()),
                "secondary must reuse the primary GPU owner"
            );
            assert_ne!(
                primary.window_id(),
                secondary.window_id(),
                "each Host must own a distinct native window"
            );

            let primary_viewport = primary.viewport().clone();
            let resized = winit::dpi::PhysicalSize::new(48, 40);
            let resize_outcome = secondary.handle_window_event(&WindowEvent::Resized(resized));
            assert!(resize_outcome.handled);
            assert_eq!(secondary.viewport().physical_size, resized.into());
            assert_eq!(
                primary.viewport(),
                &primary_viewport,
                "secondary resize must not mutate primary viewport state"
            );

            Self::present(&mut secondary)?;
            // Drop the complete secondary Host before constructing the next one.
            drop(secondary);
        }

        // Both public builders × Component/macro View/existing View, using the
        // production native mount path. Include a fresh GPU bootstrap as well as reuse.
        let fresh = root_case(event_loop, None, false, |root| root)?;
        for with_output in [false, true] {
            let gpu = || Some(Arc::clone(fresh.shared_gpu()));
            drop(root_case(event_loop, gpu(), with_output, |root| root)?);
            drop(root_case(
                event_loop,
                gpu(),
                with_output,
                |root| view! { root; },
            )?);
            let describe_existing = |root: StatefulRoot| {
                let existing: View = root.into_child_view();
                existing
            };
            drop(root_case(
                event_loop,
                gpu(),
                with_output,
                describe_existing,
            )?);
        }
        startup_failures(event_loop, fresh.shared_gpu());
        drop(fresh);

        // Exercise the declared Surface/platform/Window/GPU field drop sequence.
        drop(primary);
        Ok(())
    }
}

impl ApplicationHandler for NativeHostSmoke {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some(self.run(event_loop));
        }
        event_loop.exit();
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        _event: WindowEvent,
    ) {
    }
}

/// Real native/GPU smoke. Kept ignored in the default suite because it requires
/// an interactive Windows session and a DX12-capable adapter.
#[test]
#[ignore = "requires an interactive Windows desktop and native GPU"]
fn hidden_hosts_present_and_reuse_gpu_across_repeated_secondary_lifecycles() {
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().expect("create native smoke event loop");
    let mut app = NativeHostSmoke::default();
    event_loop
        .run_app(&mut app)
        .expect("run native smoke event loop");
    app.result
        .expect("native smoke resumed")
        .expect("native host smoke succeeded");
}
