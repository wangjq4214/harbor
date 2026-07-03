use std::sync::Arc;

use anyhow::{Context as _, Result};
use wgpu::util::DeviceExt;
use winit::window::Window;


// ── GpuContext ────────────────────────────────────────────────────────────

/// Shared GPU handles for layers to create and upload resources.
///
/// Fields are private — layers access device/queue/surface through methods only.
pub(crate) struct GpuContext {
    /// wgpu surface bound to the window, provides frame buffers.
    surface: wgpu::Surface<'static>,
    /// Logical GPU device for creating pipelines / textures / buffers.
    device: wgpu::Device,
    /// Command submission queue.
    queue: wgpu::Queue,
    /// Surface configuration (format, size, present mode).
    config: wgpu::SurfaceConfiguration,
}

impl GpuContext {
    /// Creates the GPU surface, device, queue, and surface configuration from
    /// the window. Does not create any render pipelines or layer resources.
    pub(crate) async fn new(window: Arc<Window>) -> Result<Self> {
        let size = window.inner_size();
        tracing::info!(
            width = size.width,
            height = size.height,
            "creating gpu context"
        );
        let instance = wgpu::Instance::default();
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
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            })
            .await
            .context("request device")?;

        // Prefer sRGB format so fragment shader colours display correctly.
        let capabilities = surface.get_capabilities(&adapter);
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
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: capabilities.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        tracing::info!(
            width = config.width,
            height = config.height,
            ?format,
            "gpu context configured"
        );

        Ok(Self {
            surface,
            device,
            queue,
            config,
        })
    }

    /// Reconfigures the surface for a new window size.
    pub(crate) fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            tracing::trace!("ignored zero-sized resize");
            return;
        }
        self.config.width = width;
        self.config.height = height;
        tracing::trace!(width, height, "gpu context resized");
        self.surface.configure(&self.device, &self.config);
    }

    /// Surface pixel format.
    pub(crate) fn format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    /// Current surface dimensions `(width, height)`.
    pub(crate) fn surface_size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    /// Logical GPU device reference.
    pub(crate) fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Command queue reference.
    pub(crate) fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    // ── surface-only operations (used exclusively by Renderer) ──────────

    /// Gets the current frame surface texture.  See `CurrentSurfaceTexture`
    /// variant docs for how to handle each status.
    pub(crate) fn get_current_texture(&self) -> wgpu::CurrentSurfaceTexture {
        self.surface.get_current_texture()
    }

    /// Presents the frame after command submission.
    pub(crate) fn present(&self, surface_texture: wgpu::SurfaceTexture) {
        self.queue.present(surface_texture);
    }
}

// ── Shared vertex type ────────────────────────────────────────────────────

/// GPU vertex for textured quads. Replaces both `text::Vertex` and
/// `cursor::CursorVertex`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct TexturedVertex {
    /// NDC position (x, y), range [-1, 1].
    pub(crate) position: [f32; 2],
    /// Texture coordinates (u, v), range [0, 1].
    pub(crate) tex_coords: [f32; 2],
}

impl Default for TexturedVertex {
    fn default() -> Self {
        Self {
            position: [0.0; 2],
            tex_coords: [0.0; 2],
        }
    }
}

impl TexturedVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2];

    /// Returns the vertex buffer layout matching `TexturedVertex` memory layout.
    pub(crate) fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }

    /// Builds 6 vertices (two triangles) from a pixel-space rect and atlas UV
    /// rect, transformed to clip space.
    ///
    /// # Parameters
    /// - `left/top/right/bottom`: pixel-space rectangle
    /// - `uv_l/uv_t/uv_r/uv_b`: atlas sub-region UV rectangle
    /// - `surf_w/surf_h`: surface dimensions (for pixel→NDC transform)
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_pixel_rect(
        left: f32,
        top: f32,
        right: f32,
        bottom: f32,
        uv_l: f32,
        uv_t: f32,
        uv_r: f32,
        uv_b: f32,
        surf_w: f32,
        surf_h: f32,
    ) -> [Self; 6] {
        // Pixel → NDC [-1, 1]: linear x mapping, y-flip (screen is y-down, NDC is y-up).
        let ndc_left = left / surf_w * 2.0 - 1.0;
        let ndc_right = right / surf_w * 2.0 - 1.0;
        let ndc_top = 1.0 - top / surf_h * 2.0;
        let ndc_bottom = 1.0 - bottom / surf_h * 2.0;

        // Two triangles forming a quad: TL → BL → BR, TL → BR → TR.
        [
            Self {
                position: [ndc_left, ndc_top],
                tex_coords: [uv_l, uv_t],
            },
            Self {
                position: [ndc_left, ndc_bottom],
                tex_coords: [uv_l, uv_b],
            },
            Self {
                position: [ndc_right, ndc_bottom],
                tex_coords: [uv_r, uv_b],
            },
            Self {
                position: [ndc_left, ndc_top],
                tex_coords: [uv_l, uv_t],
            },
            Self {
                position: [ndc_right, ndc_bottom],
                tex_coords: [uv_r, uv_b],
            },
            Self {
                position: [ndc_right, ndc_top],
                tex_coords: [uv_r, uv_t],
            },
        ]
    }
}

// ── Shared GPU helpers ────────────────────────────────────────────────────

/// Bind group layout shared by text and cursor layers (texture + sampler).
pub(crate) fn create_texture_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("texture bind group layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

/// Creates a vertex buffer from a slice of `TexturedVertex`. Uploads one
/// zero vertex when the slice is empty (wgpu requires non-zero buffers);
/// the caller must set `vertex_count` to 0 to skip drawing.
pub(crate) fn create_vertex_buffer(
    device: &wgpu::Device,
    vertices: &[TexturedVertex],
) -> wgpu::Buffer {
    let vertices = if vertices.is_empty() {
        &[TexturedVertex {
            position: [0.0, 0.0],
            tex_coords: [0.0, 0.0],
        }]
    } else {
        vertices
    };
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("vertex buffer"),
        contents: bytemuck::cast_slice(vertices),
        // COPY_DST lets CursorLayer use queue.write_buffer for partial updates.
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
    })
}
