#![cfg(all(target_os = "windows", feature = "winit"))]
//! Non-destructive actual wgpu validation, not direct FrameError injection.
#[path = "native_gpu_faults/native.rs"]
mod native;

use harbor_widget::winit::{
    FrameError, GpuFaultKind, GpuWake, GpuWakeEvent, HostFrameOutcome, HostInitContext,
    HostStartupStage, WinitWindowHost, WinitWindowHostBuilder,
};
use harbor_widget::{ExternalSurface, SizedBox, layout::Size};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    platform::windows::EventLoopBuilderExtWindows,
    window::{Window, WindowId},
};

#[derive(Debug)]
enum Event {
    Gpu,
    Watchdog,
}
impl From<GpuWakeEvent> for Event {
    fn from(_: GpuWakeEvent) -> Self {
        Self::Gpu
    }
}
fn attributes() -> winit::window::WindowAttributes {
    Window::default_attributes()
        .with_visible(false)
        .with_title("Harbor GPU fault contract")
        .with_inner_size(winit::dpi::PhysicalSize::new(64, 64))
}
fn invalid_buffer(device: &wgpu::Device) {
    let _ = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("native fault sentinel"),
        size: 4,
        usage: wgpu::BufferUsages::empty(),
        mapped_at_creation: false,
    });
}
fn root(_: HostInitContext<'_>, _: &()) -> anyhow::Result<SizedBox> {
    Ok(SizedBox::new(Size::new(16.0, 16.0))?)
}
struct DropOutput(Arc<std::sync::atomic::AtomicBool>);
impl Drop for DropOutput {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}
struct App {
    wake: GpuWake,
    hosts: Vec<WinitWindowHost>,
    trigger: Option<std::thread::JoinHandle<()>>,
    complete: bool,
    closed_transport: Option<WinitWindowHost>,
}

impl ApplicationHandler<Event> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if !self.hosts.is_empty() {
            return;
        }
        // Bootstrap initialization already has callbacks; failed output is not published.
        let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let output = DropOutput(dropped.clone());
        let failed =
            WinitWindowHostBuilder::new(attributes(), move |cx: HostInitContext<'_>, _: &()| {
                invalid_buffer(cx.gpu().device());
                Ok::<_, anyhow::Error>((SizedBox::new(Size::new(8.0, 8.0))?, output))
            })
            .with_gpu_wake(self.wake.clone());
        let error = pollster::block_on(failed.build_with_output(event_loop))
            .err()
            .unwrap();
        assert_eq!(error.stage(), HostStartupStage::RuntimeInitialization);
        assert!(error.to_string().contains("native fault sentinel"));
        assert!(dropped.load(std::sync::atomic::Ordering::SeqCst));

        // Failed shared-device reuse cannot erase the live primary's notification.
        let mut primary = pollster::block_on(
            WinitWindowHostBuilder::new(attributes(), root)
                .with_gpu_wake(self.wake.clone())
                .build(event_loop),
        )
        .unwrap();
        let failed =
            WinitWindowHostBuilder::new(attributes(), |cx: HostInitContext<'_>, _: &()| {
                invalid_buffer(cx.gpu().device());
                root(cx, &())
            })
            .reuse_gpu(primary.shared_gpu().clone());
        assert!(pollster::block_on(failed.build(event_loop)).is_err());
        assert_eq!(
            primary
                .about_to_wait(Instant::now(), None)
                .fatal
                .unwrap()
                .kind,
            GpuFaultKind::Validation
        );
        drop(primary);

        // Actual encoding and Host queue submission failures, not injected FrameErrors.
        for submit_failure in [false, true] {
            let encoded = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let mark = encoded.clone();
            let mut host = pollster::block_on(
                WinitWindowHostBuilder::new(attributes(), move |_: HostInitContext<'_>, _: &()| {
                    Ok::<_, anyhow::Error>(ExternalSurface::new(1).on_draw(Arc::new(
                        move |_, _, gpu, pass, _| {
                            mark.store(true, std::sync::atomic::Ordering::SeqCst);
                            if submit_failure {
                                let buffer = gpu.device().create_buffer(&wgpu::BufferDescriptor {
                                    label: Some("submission fault sentinel"),
                                    size: 4,
                                    usage: wgpu::BufferUsages::VERTEX,
                                    mapped_at_creation: false,
                                });
                                pass.set_vertex_buffer(0, buffer.slice(..));
                                buffer.destroy(); // Harmless fixture resource; rejected by Host's submit.
                            } else {
                                invalid_buffer(gpu.device());
                            }
                        },
                    )))
                })
                .with_gpu_wake(self.wake.clone())
                .build(event_loop),
            )
            .unwrap();
            let frame = host.present_now();
            assert!(encoded.load(std::sync::atomic::Ordering::SeqCst));
            assert!(matches!(
                frame,
                HostFrameOutcome::Fatal {
                    error: FrameError::SharedGpu(_),
                    ..
                }
            ));
            if submit_failure {
                assert!(host.gpu_fault().unwrap().message.contains("Queue::submit"));
            }
            drop(host);
        }

        for _ in 0..2 {
            let mut builder =
                WinitWindowHostBuilder::new(attributes(), root).with_gpu_wake(self.wake.clone());
            if let Some(first) = self.hosts.first() {
                builder = builder.reuse_gpu(first.shared_gpu().clone());
            }
            let mut host = pollster::block_on(builder.build(event_loop)).unwrap();
            assert!(host.present_now().is_presented());
            // Native minimize plus explicit zero geometry exercises scheduler suppression
            // deterministically; do not count this as driver removal or visual smoke.
            host.window().set_minimized(true);
            host.handle_window_event(&WindowEvent::Resized(winit::dpi::PhysicalSize::new(0, 0)));
            assert!(matches!(
                host.present_now(),
                HostFrameOutcome::Skipped { .. }
            ));
            self.hosts.push(host);
        }
        assert!(Arc::ptr_eq(
            self.hosts[0].shared_gpu(),
            self.hosts[1].shared_gpu()
        ));
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.complete {
            return;
        }
        // Ignore native startup resize/redraws: no drawing after the explicit suspend.
        for host in &mut self.hosts {
            assert_eq!(host.viewport().physical_size, (0, 0));
            let idle = host.about_to_wait(Instant::now(), None);
            // Startup fault wakes from the earlier devices may be queued, but not this one.
            if idle.fatal.is_none() {
                assert_eq!(idle.wait, Some(harbor_widget::ControlFlowEffect::Wait));
            }
        }
        event_loop.set_control_flow(ControlFlow::Wait);
        if self.trigger.is_none() {
            let device = self.hosts[0].gpu().device().clone();
            self.trigger = Some(std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(200));
                invalid_buffer(&device); // Real callback records and wakes the sleeping UI.
            }));
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Event) {
        if matches!(event, Event::Watchdog) {
            event_loop.exit();
            return;
        }
        if self.hosts.is_empty() || self.hosts[0].gpu_fault().is_none() {
            return;
        }
        let mut secondary = self.hosts.pop().unwrap();
        let fault = secondary.about_to_wait(Instant::now(), None).fatal.unwrap();
        assert_eq!(fault.kind, GpuFaultKind::Validation);
        assert!(fault.message.contains("native fault sentinel"));
        assert!(secondary.present_now().is_fatal()); // Cannot be hidden by zero-size skip.
        drop(secondary);
        let primary = &mut self.hosts[0];
        assert_eq!(primary.gpu_fault(), Some(fault.clone()));
        assert_eq!(
            primary.about_to_wait(Instant::now(), None).fatal,
            Some(fault.clone())
        );
        assert_eq!(
            primary
                .handle_window_event(&WindowEvent::Focused(true))
                .fatal,
            Some(fault)
        );
        assert!(primary.present_now().is_fatal());
        self.closed_transport = Some(
            pollster::block_on(
                WinitWindowHostBuilder::new(attributes(), root)
                    .with_gpu_wake(self.wake.clone())
                    .build(event_loop),
            )
            .unwrap(),
        );
        self.complete = true;
        event_loop.exit(); // Application policy, never callback-thread exit.
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}
