#![cfg(target_os = "windows")]

use harbor_widget::layout::Size;
use harbor_widget::winit::{
    HostFrameOutcome, HostInitContext, WinitWindowHost, WinitWindowHostBuilder,
};
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::platform::windows::EventLoopBuilderExtWindows;
use winit::window::Window;

const SECONDARY_LIFECYCLES: usize = 2;

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
                Ok::<_, anyhow::Error>(harbor_widget::widgets::sized_box::SizedBox::new(Size::new(
                    64.0, 64.0,
                )))
            },
        );
        match shared_gpu {
            Some(gpu) => pollster::block_on(builder.reuse_gpu(gpu).build(event_loop)),
            None => pollster::block_on(builder.build(event_loop)),
        }
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
            let mut secondary = Self::build_host(event_loop, Some(gpu))
                .map_err(|error| format!("secondary host startup {lifecycle} failed: {error}"))?;
            Self::present(&mut secondary)?;
            // Drop the complete secondary Host before constructing the next one.
            drop(secondary);
        }

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
