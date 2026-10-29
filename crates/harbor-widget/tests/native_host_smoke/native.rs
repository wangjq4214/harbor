use super::*;

/// Requires an interactive Windows session and a DX12-capable adapter.
#[test]
#[ignore = "requires an interactive Windows desktop and native GPU"]
fn hidden_hosts_present_and_reuse_gpu_across_repeated_secondary_lifecycles() {
    let mut builder = EventLoop::<GpuWakeEvent>::with_user_event();
    builder.with_any_thread(true);
    let event_loop = builder.build().expect("create native smoke event loop");
    let mut app = NativeHostSmoke {
        wake: Some(GpuWake::new(event_loop.create_proxy())),
        ..Default::default()
    };
    event_loop
        .run_app(&mut app)
        .expect("run native smoke event loop");
    app.result
        .expect("native smoke resumed")
        .expect("native host smoke succeeded");
}
