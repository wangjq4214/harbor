#![cfg(all(target_os = "windows", feature = "winit"))]

//! Real winit WaitUntil delivery with two real Hosts and injected zero-size
//! transitions. This proves native timer delivery, not OS minimize behavior.

use harbor_widget::winit::{
    GpuWake, GpuWakeEvent, HostFrameOutcome, HostInitContext, WinitWindowHost,
    WinitWindowHostBuilder,
};
use harbor_widget::{ControlFlowEffect, SizedBox, layout::Size};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::platform::windows::EventLoopBuilderExtWindows;
use winit::window::{Window, WindowId};

#[derive(Default)]
struct DeadlineSmoke {
    wake: Option<GpuWake>,
    hosts: Vec<WinitWindowHost>,
    deadlines: Vec<Instant>,
    wakes: usize,
    complete: bool,
}

impl ApplicationHandler<GpuWakeEvent> for DeadlineSmoke {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if !self.hosts.is_empty() {
            return;
        }
        for _ in 0..2 {
            let mut builder = WinitWindowHostBuilder::new(
                Window::default_attributes()
                    .with_visible(false)
                    .with_title("harbor-widget suspended deadline smoke")
                    .with_inner_size(winit::dpi::PhysicalSize::new(64, 64)),
                |_: HostInitContext<'_>, _: &()| {
                    Ok::<_, anyhow::Error>(SizedBox::new(Size::new(10.0, 10.0)).unwrap())
                },
            )
            .with_gpu_wake(self.wake.as_ref().unwrap().clone());
            if let Some(primary) = self.hosts.first() {
                builder = builder.reuse_gpu(Arc::clone(primary.shared_gpu()));
            }
            let mut host = pollster::block_on(builder.build(event_loop)).expect("build real Host");
            host.handle_window_event(&WindowEvent::Resized(winit::dpi::PhysicalSize::new(0, 0)));
            assert!(matches!(
                host.present_now(),
                HostFrameOutcome::Skipped { .. }
            ));
            self.hosts.push(host);
        }
    }

    fn new_events(&mut self, _: &ActiveEventLoop, cause: StartCause) {
        if let StartCause::ResumeTimeReached {
            requested_resume, ..
        } = cause
        {
            assert_eq!(requested_resume, self.deadlines[self.wakes]);
            assert!(Instant::now() >= requested_resume);
            self.wakes += 1; // The application consumes the business deadline.
        }
    }

    fn window_event(&mut self, _: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        // Native startup resize events must not replace this test's injected
        // suspended geometry. Any real redraw delivered still runs the Host path.
        if matches!(event, WindowEvent::RedrawRequested) {
            let host = self
                .hosts
                .iter_mut()
                .find(|host| host.window_id() == id)
                .unwrap();
            assert!(matches!(
                host.handle_window_event(&event).frame,
                Some(HostFrameOutcome::Skipped { .. })
            ));
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if self.deadlines.is_empty() {
            self.deadlines = vec![
                now + Duration::from_millis(100),
                now + Duration::from_millis(200),
            ];
        }
        let mut wait = ControlFlowEffect::Wait;
        for (index, host) in self.hosts.iter_mut().enumerate() {
            // Repeat expired input on the consumed Host while the other waits.
            let outcome = host.about_to_wait(now, Some(self.deadlines[index]));
            let expected = if index < self.wakes {
                ControlFlowEffect::Wait
            } else {
                ControlFlowEffect::WaitUntil(self.deadlines[index])
            };
            assert_eq!(outcome.wait, Some(expected));
            wait = wait.arbitrate(expected);
        }
        if self.wakes == 2 {
            assert_eq!(wait, ControlFlowEffect::Wait);
            for host in &mut self.hosts {
                host.handle_window_event(&WindowEvent::Resized(winit::dpi::PhysicalSize::new(
                    64, 64,
                )));
                assert!(matches!(
                    host.present_now(),
                    HostFrameOutcome::Presented { .. }
                        | HostFrameOutcome::PresentedSuboptimal { .. }
                ));
            }
            self.complete = true;
            event_loop.exit();
            return;
        }
        assert_eq!(
            wait,
            ControlFlowEffect::WaitUntil(self.deadlines[self.wakes])
        );
        if let ControlFlowEffect::WaitUntil(deadline) = wait {
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, _: GpuWakeEvent) {
        // Failure watchdog only; it never supplies a successful business wake.
        event_loop.exit();
    }
}

/// Runs separately from other native test executables (one EventLoop/process).
#[test]
#[ignore = "requires an interactive Windows desktop and native GPU"]
fn suspended_hosts_receive_native_deadline_wakes_then_restore() {
    let mut builder = EventLoop::<GpuWakeEvent>::with_user_event();
    builder.with_any_thread(true);
    let event_loop = builder.build().expect("create native deadline event loop");
    let proxy = event_loop.create_proxy();
    let wake = GpuWake::new(proxy.clone());
    let (done, receiver) = mpsc::channel();
    let watchdog = std::thread::spawn(move || {
        if receiver.recv_timeout(Duration::from_secs(30)).is_err() {
            let _ = proxy.send_event(GpuWakeEvent);
        }
    });
    let mut app = DeadlineSmoke {
        wake: Some(wake),
        ..Default::default()
    };
    let result = event_loop.run_app(&mut app);
    let _ = done.send(());
    watchdog.join().unwrap();
    result.expect("run native deadline event loop");
    assert!(app.complete, "native deadline wake timed out");
    assert_eq!(app.wakes, 2);
}
