//! Required offscreen GPU fixture, shared by test targets only.
//! Resource suites are ignored by default; explicitly selected GPU tests fail closed.
pub fn device() -> (wgpu::Device, wgpu::Queue) {
    let selection = std::env::var("HARBOR_TEST_GPU")
        .or_else(|_| std::env::var("HARBOR_TEST_BACKEND"))
        .unwrap_or_else(|_| "auto".into());
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = match selection.as_str() {
        "auto" => wgpu::Backends::all(),
        "dx12" | "dx12-warp" => wgpu::Backends::DX12,
        "vulkan" => wgpu::Backends::VULKAN,
        _ => panic!("HARBOR_TEST_GPU must be auto, dx12, dx12-warp or vulkan"),
    };
    let warp = selection == "dx12-warp";
    let instance = wgpu::Instance::new(descriptor);
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        force_fallback_adapter: warp,
        ..Default::default()
    }))
    .expect("explicit GPU test requires an available adapter");
    let info = adapter.get_info();
    eprintln!("GPU test adapter: {info:?}");
    if warp {
        assert_eq!(info.backend, wgpu::Backend::Dx12);
        assert_eq!(info.device_type, wgpu::DeviceType::Cpu);
    }
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("test-device"),
        ..Default::default()
    }))
    .expect("explicit GPU test requires a usable device")
}
