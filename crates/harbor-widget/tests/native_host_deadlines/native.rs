use super::*;

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
