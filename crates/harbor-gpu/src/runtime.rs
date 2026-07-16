use anyhow::{Context as _, Result};
use std::sync::Arc;
use winit::window::Window;

/// Domain-neutral GPU device runtime shared by renderer targets.
pub struct GpuRuntime {
    instance: Arc<wgpu::Instance>,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

/// One configured surface owned by a renderer target.
pub struct GpuSurface {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
}

impl GpuRuntime {
    /// Creates a device compatible with `window` and returns its first surface.
    pub async fn new(window: Arc<Window>) -> Result<(Self, GpuSurface)> {
        let size = window.inner_size();
        let instance = Arc::new(wgpu::Instance::new(wgpu::InstanceDescriptor {
            #[cfg(target_os = "windows")]
            backends: wgpu::Backends::DX12,
            #[cfg(not(target_os = "windows"))]
            backends: wgpu::Backends::all(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        }));
        let surface = instance.create_surface(window).context("create surface")?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .context("request adapter")?;
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
        let runtime = Self {
            instance,
            adapter,
            device,
            queue,
        };
        let target = runtime.configure_surface(surface, (size.width, size.height));
        Ok((runtime, target))
    }

    /// Creates and configures an additional surface for a host-owned window.
    pub fn create_surface(&self, window: Arc<Window>) -> Result<GpuSurface> {
        let size = window.inner_size();
        let surface = self
            .instance
            .create_surface(window)
            .context("create surface")?;
        Ok(self.configure_surface(surface, (size.width, size.height)))
    }

    pub(crate) fn create_unconfigured_surface(
        &self,
        window: Arc<Window>,
    ) -> Result<wgpu::Surface<'static>> {
        self.instance
            .create_surface(window)
            .context("create surface")
    }

    pub(crate) fn surface_capabilities(
        &self,
        surface: &wgpu::Surface,
    ) -> wgpu::SurfaceCapabilities {
        surface.get_capabilities(&self.adapter)
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    fn configure_surface(&self, surface: wgpu::Surface<'static>, size: (u32, u32)) -> GpuSurface {
        let capabilities = surface.get_capabilities(&self.adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .unwrap_or(capabilities.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.0.max(1),
            height: size.1.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: capabilities.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&self.device, &config);
        GpuSurface { surface, config }
    }
}

impl GpuSurface {
    pub fn resize(&mut self, runtime: &GpuRuntime, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(runtime.device(), &self.config);
    }

    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    pub fn acquire(&self) -> wgpu::CurrentSurfaceTexture {
        self.surface.get_current_texture()
    }
}
