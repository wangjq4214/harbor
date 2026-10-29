use super::*;

#[test]
#[ignore = "requires an interactive Windows desktop and native GPU"]
fn actual_gpu_callbacks_wake_suspended_shared_hosts_and_report_fatal() {
    let mut builder = EventLoop::<Event>::with_user_event();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let proxy = event_loop.create_proxy();
    let wake = GpuWake::new(proxy.clone());
    let (done, receiver) = mpsc::channel();
    let watchdog = std::thread::spawn(move || {
        if receiver.recv_timeout(Duration::from_secs(30)).is_err() {
            let _ = proxy.send_event(Event::Watchdog);
        }
    });
    let mut app = App {
        wake,
        hosts: Vec::new(),
        trigger: None,
        complete: false,
        closed_transport: None,
    };
    event_loop.run_app(&mut app).unwrap();
    let _ = done.send(());
    watchdog.join().unwrap();
    if let Some(trigger) = app.trigger.take() {
        trigger.join().unwrap();
    }
    assert!(
        app.complete,
        "GPU callback never woke the suspended event loop"
    );
    // EventLoop is closed; late production callbacks must tolerate the closed proxy.
    let host = app.closed_transport.take().unwrap();
    assert!(host.gpu_fault().is_none());
    let device = host.gpu().device().clone();
    invalid_buffer(&device); // First fault attempts a send on the closed proxy.
    assert_eq!(host.gpu_fault().unwrap().kind, GpuFaultKind::Validation);
    drop(host);
    app.hosts.clear();
    device.destroy();
}
