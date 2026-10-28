use super::*;

#[cfg(target_os = "windows")]
#[test]
#[ignore = "native: real hidden Windows window"]
#[allow(deprecated)] // Hidden native fixture without entering a running event loop.
fn should_report_opaque_fallback_when_applied_to_a_real_hidden_window() {
    use winit::event_loop::EventLoop;
    use winit::platform::windows::EventLoopBuilderExtWindows;

    // Arrange — exactly one event loop; never manufacture a Window reference.
    let event_loop = EventLoop::builder()
        .with_any_thread(true)
        .build()
        .expect("Windows event loop for hidden-window regression");
    let window = event_loop
        .create_window(Window::default_attributes().with_visible(false))
        .expect("real hidden Windows window");
    let backend = OpaqueBackend;
    let style = WindowBackdropStyle::default();

    // Act
    let status = backend.apply(&window, &style);

    // Assert
    assert_eq!(
        status,
        BackdropStatus {
            tier: BackdropTier::OpaqueFallback,
            backdrop_available: false,
        }
    );
    let hooks = MainWindowPlatformHooks::new(Box::new(OpaqueBackend), style);
    for backdrop_available in [false, true] {
        let mut setup = MainWindowPlatformState {
            _backend: Box::new(OpaqueBackend),
            status: BackdropStatus {
                tier: if backdrop_available {
                    BackdropTier::WasdkAcrylic
                } else {
                    BackdropTier::OpaqueFallback
                },
                backdrop_available,
            },
        };
        // Repeated calls must use the supplied surface, not retained alpha state.
        for alpha_mode in [
            wgpu::CompositeAlphaMode::Opaque,
            wgpu::CompositeAlphaMode::PreMultiplied,
            wgpu::CompositeAlphaMode::PostMultiplied,
            wgpu::CompositeAlphaMode::Auto,
            wgpu::CompositeAlphaMode::Inherit,
        ] {
            let surface = WindowSurfaceInfo {
                format: wgpu::TextureFormat::Bgra8Unorm,
                alpha_mode,
                physical_size: window.inner_size().into(),
            };
            assert_eq!(
                hooks.surface_ready(&window, &mut setup, surface).unwrap(),
                backdrop_available && alpha_mode == wgpu::CompositeAlphaMode::PreMultiplied,
            );
        }
    }
    assert_eq!(window.is_visible(), Some(false));
}
