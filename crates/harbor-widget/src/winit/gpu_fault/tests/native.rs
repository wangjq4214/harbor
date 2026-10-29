use super::*;

#[test]
#[ignore = "requires a native GPU; exercises real callbacks and independent poll progress"]
fn production_callbacks_and_idle_progress() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: crate::winit::gpu::selected_backends(),
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    eprintln!("notification test adapter: {:?}", adapter.get_info());
    let (normal_device, normal_queue) =
        pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let (normal_tx, normal_rx) = std::sync::mpsc::channel();
    let normal = GpuMonitor::start(
        &normal_device,
        GpuWake(Arc::new(move || {
            let _ = normal_tx.send(());
        })),
    )
    .unwrap();
    normal_device.destroy();
    normal_device.poll(wgpu::PollType::Poll).unwrap();
    assert!(
        normal.fault().is_none(),
        "normal destroy is not device loss"
    );
    assert!(matches!(
        normal_rx.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
    drop(normal);
    drop(normal_queue);
    drop(normal_device);

    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let monitor = GpuMonitor::start(
        &device,
        GpuWake(Arc::new(move || {
            let _ = tx.send(());
        })),
    )
    .unwrap();

    // A map callback needs native poll progress. No UI/render loop calls poll.
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("idle notification progress"),
        size: 4,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let (mapped, mapping) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = mapped.send(result);
        });
    mapping
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    buffer.unmap();
    assert_eq!(monitor.fault(), None);

    // Harmless real uncaptured validation, through the installed production handler.
    let _ = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("fatal validation sentinel"),
        size: 4,
        usage: wgpu::BufferUsages::empty(),
        mapped_at_creation: false,
    });
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let fault = monitor.fault().unwrap();
    assert_eq!(fault.kind, GpuFaultKind::Validation);
    assert!(fault.message.contains("fatal validation sentinel"));
    assert_eq!(monitor.fault(), Some(fault));
    let weak = Arc::downgrade(&monitor.state);
    drop(monitor);
    assert!(weak.upgrade().is_none());
    drop(rx); // closed transport/late device callbacks cannot panic
    for _ in 0..8 {
        let (tx, rx) = std::sync::mpsc::channel();
        let monitor = GpuMonitor::start(
            &device,
            GpuWake(Arc::new(move || {
                let _ = tx.send(());
            })),
        )
        .unwrap();
        let weak = Arc::downgrade(&monitor.state);
        drop(rx);
        let producer_device = device.clone();
        let producer = std::thread::spawn(move || {
            let _ = producer_device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("teardown race"),
                size: 4,
                usage: wgpu::BufferUsages::empty(),
                mapped_at_creation: false,
            });
        });
        drop(monitor);
        producer.join().unwrap();
        assert!(weak.upgrade().is_none());
    }
    drop(queue);
    device.destroy();
    let _ = device.poll(wgpu::PollType::Poll);
}
