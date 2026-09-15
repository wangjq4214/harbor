//! Shared native GPU resources and independent per-window surfaces.

use std::sync::Arc;

use anyhow::{Context as _, Result};
use winit::window::Window;

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HWND;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice, IDCompositionDevice, IDCompositionTarget, IDCompositionVisual,
};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
#[cfg(target_os = "windows")]
use windows::core::Interface;
#[cfg(target_os = "windows")]
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// The desktop composition target layer for the renderer surface is always the upper slot
/// (ADR 0028). The application-owned backdrop occupies the lower slot.
#[cfg(target_os = "windows")]
pub const RENDER_TARGET_IS_TOPMOST: bool = true;

fn selected_backends() -> wgpu::Backends {
    #[cfg(all(feature = "backend-dx12", feature = "backend-vulkan"))]
    {
        wgpu::Backends::DX12 | wgpu::Backends::VULKAN
    }
    #[cfg(all(feature = "backend-dx12", not(feature = "backend-vulkan")))]
    {
        wgpu::Backends::DX12
    }
    #[cfg(all(not(feature = "backend-dx12"), feature = "backend-vulkan"))]
    {
        wgpu::Backends::VULKAN
    }
    #[cfg(all(
        not(feature = "backend-dx12"),
        not(feature = "backend-vulkan"),
        target_os = "windows"
    ))]
    {
        // Desktop Acrylic requires a DirectComposition-capable swap chain.
        wgpu::Backends::DX12
    }
    #[cfg(all(
        not(feature = "backend-dx12"),
        not(feature = "backend-vulkan"),
        not(target_os = "windows")
    ))]
    {
        wgpu::Backends::all()
    }
}

/// Picks a compositing-capable alpha mode for the main window surface.
pub fn select_compositing_alpha_mode(
    modes: &[wgpu::CompositeAlphaMode],
) -> wgpu::CompositeAlphaMode {
    use wgpu::CompositeAlphaMode::{Auto, PostMultiplied, PreMultiplied};

    if modes.contains(&PreMultiplied) {
        return PreMultiplied;
    }
    if modes.contains(&PostMultiplied) {
        return PostMultiplied;
    }
    if modes.contains(&Auto) {
        return Auto;
    }
    modes.first().copied().unwrap_or(Auto)
}

#[cfg(target_os = "windows")]
struct CompositionHost {
    device: IDCompositionDevice,
    _target: IDCompositionTarget,
    _visual: IDCompositionVisual,
}

#[cfg(target_os = "windows")]
impl CompositionHost {
    fn commit(&self) {
        if let Err(error) = unsafe { self.device.Commit() } {
            tracing::warn!(?error, "failed to commit DirectComposition surface");
        }
    }
}

#[cfg(target_os = "windows")]
fn create_main_surface(
    instance: &wgpu::Instance,
    window: &Arc<Window>,
    backends: wgpu::Backends,
) -> Result<(wgpu::Surface<'static>, Option<CompositionHost>)> {
    if !backends.contains(wgpu::Backends::DX12) {
        return Ok((
            instance
                .create_surface(Arc::clone(window))
                .context("create main surface")?,
            None,
        ));
    }

    let handle = window.window_handle().context("get Win32 window handle")?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        anyhow::bail!("main window is not a Win32 window");
    };

    let device: IDCompositionDevice = unsafe {
        DCompositionCreateDevice(None::<&IDXGIDevice>).context("create DirectComposition device")?
    };
    let target = unsafe {
        device
            .CreateTargetForHwnd(HWND(handle.hwnd.get() as *mut _), RENDER_TARGET_IS_TOPMOST)
            .context("create DirectComposition window target")?
    };
    let visual = unsafe {
        device
            .CreateVisual()
            .context("create DirectComposition visual")?
    };
    unsafe {
        target
            .SetRoot(&visual)
            .context("set DirectComposition root visual")?;
        device.Commit().context("commit DirectComposition tree")?;
    }
    let surface = unsafe {
        instance
            .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::CompositionVisual(
                visual.as_raw(),
            ))
            .context("create DirectComposition surface")?
    };

    Ok((
        surface,
        Some(CompositionHost {
            device,
            _target: target,
            _visual: visual,
        }),
    ))
}

/// Reusable native GPU resources shared by all window surfaces.
pub struct SharedGpu {
    instance: Arc<wgpu::Instance>,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl SharedGpu {
    /// Bootstraps the shared adapter from the main window's compatible surface.
    pub(crate) async fn new(main_window: Arc<Window>) -> Result<(Self, WindowSurface)> {
        let size = main_window.inner_size();
        let backends = selected_backends();
        let instance = Arc::new(wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        }));

        #[cfg(target_os = "windows")]
        let (surface, composition_host) = create_main_surface(&instance, &main_window, backends)?;
        #[cfg(not(target_os = "windows"))]
        let surface = instance
            .create_surface(Arc::clone(&main_window))
            .context("create main surface")?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .context("request adapter")?;
        let info = adapter.get_info();
        tracing::info!(
            name = %info.name,
            backend = ?info.backend,
            device_type = ?info.device_type,
            "selected gpu adapter"
        );
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
            })
            .await
            .context("request device")?;

        let gpu = Self {
            instance,
            adapter,
            device,
            queue,
        };
        let main_surface = WindowSurface::from_surface(
            surface,
            #[cfg(target_os = "windows")]
            composition_host,
            &gpu,
            (size.width, size.height),
            None,
            true,
        )?;
        Ok((gpu, main_surface))
    }

    /// Creates and configures an independent secondary surface against this adapter.
    pub(crate) fn create_window_surface(&self, window: Arc<Window>) -> Result<WindowSurface> {
        let size = window.inner_size();
        let surface = self
            .instance
            .create_surface(Arc::clone(&window))
            .with_context(|| format!("create surface for window {:?}", window.id()))?;
        WindowSurface::from_surface(
            surface,
            #[cfg(target_os = "windows")]
            None,
            self,
            (size.width, size.height),
            None,
            false,
        )
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }
}

/// One window's presentation surface and nonzero configuration.
///
/// `surface` is declared before the Windows composition keepalive so the surface is destroyed
/// first, while the visual tree it targets is still alive.
pub(crate) struct WindowSurface {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    #[cfg(target_os = "windows")]
    composition_host: Option<CompositionHost>,
}

impl WindowSurface {
    #[allow(clippy::too_many_arguments)]
    fn from_surface(
        surface: wgpu::Surface<'static>,
        #[cfg(target_os = "windows")] composition_host: Option<CompositionHost>,
        gpu: &SharedGpu,
        size: (u32, u32),
        preferred_format: Option<wgpu::TextureFormat>,
        compositing: bool,
    ) -> Result<Self> {
        let capabilities = surface.get_capabilities(&gpu.adapter);
        let format = preferred_format
            .filter(|format| capabilities.formats.contains(format))
            .or_else(|| {
                capabilities
                    .formats
                    .iter()
                    .copied()
                    .find(wgpu::TextureFormat::is_srgb)
            })
            .or_else(|| capabilities.formats.first().copied())
            .context("surface reports no supported texture formats")?;
        let alpha_mode = if compositing {
            select_compositing_alpha_mode(&capabilities.alpha_modes)
        } else {
            capabilities
                .alpha_modes
                .first()
                .copied()
                .context("surface reports no supported alpha modes")?
        };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.0.max(1),
            height: size.1.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        if size.0 > 0 && size.1 > 0 {
            surface.configure(gpu.device(), &config);
            #[cfg(target_os = "windows")]
            if let Some(host) = composition_host.as_ref() {
                host.commit();
            }
        }
        Ok(Self {
            surface,
            config,
            #[cfg(target_os = "windows")]
            composition_host,
        })
    }

    /// Reconfigures this surface only for drawable dimensions.
    pub fn configure_size(&mut self, gpu: &SharedGpu, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(gpu.device(), &self.config);
        #[cfg(target_os = "windows")]
        if let Some(host) = self.composition_host.as_ref() {
            host.commit();
        }
    }

    pub fn surface(&self) -> &wgpu::Surface<'static> {
        &self.surface
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    pub fn alpha_mode(&self) -> wgpu::CompositeAlphaMode {
        self.config.alpha_mode
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::CompositeAlphaMode::{Auto, Opaque, PostMultiplied, PreMultiplied};

    #[test]
    fn compositing_alpha_mode_prefers_transparency() {
        assert_eq!(
            select_compositing_alpha_mode(&[Opaque, Auto, PostMultiplied, PreMultiplied]),
            PreMultiplied
        );
        assert_eq!(
            select_compositing_alpha_mode(&[Opaque, Auto, PostMultiplied]),
            PostMultiplied
        );
        assert_eq!(select_compositing_alpha_mode(&[Opaque, Auto]), Auto);
        assert_eq!(select_compositing_alpha_mode(&[Opaque]), Opaque);
        assert_eq!(select_compositing_alpha_mode(&[]), Auto);
    }

    #[cfg(all(
        target_os = "windows",
        not(feature = "backend-dx12"),
        not(feature = "backend-vulkan")
    ))]
    #[test]
    fn windows_defaults_to_dx12() {
        assert_eq!(selected_backends(), wgpu::Backends::DX12);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn renderer_composition_target_is_topmost() {
        let expected = true;
        assert_eq!(RENDER_TARGET_IS_TOPMOST, expected);
    }
}
