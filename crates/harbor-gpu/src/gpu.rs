use std::sync::Arc;

use anyhow::{Context as _, Result};
use wgpu::util::DeviceExt;
use winit::window::Window;

// ── GpuContext ────────────────────────────────────────────────────────────

/// Shared GPU handles for layers to create and upload resources.
///
/// Fields are private — layers access device/queue/surface through methods only.
pub struct GpuContext {
    /// wgpu instance, kept alive for secondary surface creation (e.g. dialog windows).
    instance: Arc<wgpu::Instance>,
    /// Adapter, kept alive for secondary surface capability queries.
    adapter: wgpu::Adapter,
    /// wgpu surface bound to the main window, provides frame buffers.
    surface: wgpu::Surface<'static>,
    /// Logical GPU device for creating pipelines / textures / buffers.
    device: wgpu::Device,
    /// Command submission queue.
    queue: wgpu::Queue,
    /// Surface configuration (format, size, present mode).
    config: wgpu::SurfaceConfiguration,
    /// Shared untextured colored-quad pipeline (background / decoration / selection).
    colored_quad_pipeline: Arc<wgpu::RenderPipeline>,
}

impl GpuContext {
    /// Creates the GPU surface, device, queue, surface configuration, and the
    /// shared colored-quad pipeline from the window.
    pub async fn new(window: Arc<Window>) -> Result<Self> {
        let size = window.inner_size();
        tracing::info!(
            width = size.width,
            height = size.height,
            "creating gpu context"
        );
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
                // MemoryUsage: pre-allocate 8 MB device blocks instead of 128 MB.
                // A terminal emitter never allocates large GPU buffers, so the
                // smaller block size is sufficient and avoids unnecessary VRAM
                // reservation at startup.
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                // memory_hints: wgpu::MemoryHints::Performance,
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

        let colored_quad_pipeline = Arc::new(create_colored_quad_pipeline(
            &device,
            config.format,
            "colored-quad pipeline",
        ));

        Ok(Self {
            instance,
            adapter,
            surface,
            device,
            queue,
            config,
            colored_quad_pipeline,
        })
    }

    /// Reconfigures the surface for a new window size.
    pub fn resize(&mut self, width: u32, height: u32) {
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
    pub fn format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    /// Current surface dimensions `(width, height)`.
    pub fn surface_size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    /// Logical GPU device reference.
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Command queue reference.
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Shared untextured colored-quad pipeline (background / decoration / selection).
    pub fn colored_quad_pipeline(&self) -> Arc<wgpu::RenderPipeline> {
        Arc::clone(&self.colored_quad_pipeline)
    }

    /// Creates a wgpu surface from an owned window handle, using the same Instance.
    /// The returned surface has a `'static` lifetime and the caller is responsible
    /// for configuring the surface.
    pub fn create_surface(&self, window: Arc<winit::window::Window>) -> wgpu::Surface<'static> {
        self.instance
            .create_surface(window)
            .expect("create dialog surface")
    }

    /// Queries surface capabilities for a new surface, using the stored adapter.
    pub fn surface_capabilities(&self, surface: &wgpu::Surface) -> wgpu::SurfaceCapabilities {
        surface.get_capabilities(&self.adapter)
    }

    // ── surface operations ──────────────────────────────────────────────

    /// Gets the current frame surface texture.  See `CurrentSurfaceTexture`
    /// variant docs for how to handle each status.
    pub fn get_current_texture(&self) -> wgpu::CurrentSurfaceTexture {
        self.surface.get_current_texture()
    }

    /// Presents the frame after command submission.
    pub fn present(&self, surface_texture: wgpu::SurfaceTexture) {
        self.queue.present(surface_texture);
    }

    /// Acquires the surface texture, submits a single clear-color render pass,
    /// and presents the frame. No-ops on non-`Success` variants to keep the
    /// startup fast path simple — `Suboptimal` surfaces are intentionally
    /// skipped rather than presented with a size mismatch.
    pub fn clear_surface(&self, color: wgpu::Color) {
        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(output) => output,
            _ => return,
        };
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(color),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        }));
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(output);
    }
}

// ── Shared vertex type ────────────────────────────────────────────────────

/// GPU vertex for textured quads. Replaces both `text::Vertex` and
/// `cursor::CursorVertex`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TexturedVertex {
    /// NDC position (x, y), range [-1, 1].
    pub position: [f32; 2],
    /// Texture coordinates (u, v), range [0, 1].
    pub tex_coords: [f32; 2],
    /// Per-vertex RGBA tint, normalized [0, 1]. Glyph shader multiplies
    /// `glyph_alpha * color.a`, using `color.rgb` as the literal color.
    pub color: [f32; 4],
}

impl Default for TexturedVertex {
    fn default() -> Self {
        Self {
            position: [0.0; 2],
            tex_coords: [0.0; 2],
            color: [1.0; 4],
        }
    }
}

impl TexturedVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];

    /// Returns the vertex buffer layout matching `TexturedVertex` memory layout.
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }

    /// Builds 6 vertices (two triangles) from a pixel-space rect, atlas UV
    /// rect, and tint color, transformed to clip space.
    ///
    /// # Parameters
    /// - `left/top/right/bottom`: pixel-space rectangle
    /// - `uv_l/uv_t/uv_r/uv_b`: atlas sub-region UV rectangle
    /// - `color`: RGBA tint to apply (shader multiplies alpha, uses rgb as literal)
    /// - `surf_w/surf_h`: surface dimensions (for pixel→NDC transform)
    #[allow(clippy::too_many_arguments)]
    pub fn from_pixel_rect(
        left: f32,
        top: f32,
        right: f32,
        bottom: f32,
        uv_l: f32,
        uv_t: f32,
        uv_r: f32,
        uv_b: f32,
        color: [f32; 4],
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
                color,
            },
            Self {
                position: [ndc_left, ndc_bottom],
                tex_coords: [uv_l, uv_b],
                color,
            },
            Self {
                position: [ndc_right, ndc_bottom],
                tex_coords: [uv_r, uv_b],
                color,
            },
            Self {
                position: [ndc_left, ndc_top],
                tex_coords: [uv_l, uv_t],
                color,
            },
            Self {
                position: [ndc_right, ndc_bottom],
                tex_coords: [uv_r, uv_b],
                color,
            },
            Self {
                position: [ndc_right, ndc_top],
                tex_coords: [uv_r, uv_t],
                color,
            },
        ]
    }
}

// ── ColoredVertex ──────────────────────────────────────────────────────────

/// GPU vertex for solid-color quads (background rects, decoration rects).
/// No texture coordinates — color is per-vertex.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ColoredVertex {
    /// NDC position (x, y), range [-1, 1].
    pub position: [f32; 2],
    /// Per-vertex RGBA color, normalized [0, 1].
    pub color: [f32; 4],
}

impl Default for ColoredVertex {
    fn default() -> Self {
        Self {
            position: [0.0; 2],
            color: [0.0; 4],
        }
    }
}

impl ColoredVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];

    /// Returns the vertex buffer layout matching `ColoredVertex` memory layout.
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }

    /// Builds 6 vertices (two triangles) from a pixel-space rect and a single
    /// color, transformed to clip space.
    #[allow(clippy::too_many_arguments)]
    pub fn from_pixel_rect(
        left: f32,
        top: f32,
        right: f32,
        bottom: f32,
        color: [f32; 4],
        surf_w: f32,
        surf_h: f32,
    ) -> [Self; 6] {
        let ndc_left = left / surf_w * 2.0 - 1.0;
        let ndc_right = right / surf_w * 2.0 - 1.0;
        let ndc_top = 1.0 - top / surf_h * 2.0;
        let ndc_bottom = 1.0 - bottom / surf_h * 2.0;

        [
            Self {
                position: [ndc_left, ndc_top],
                color,
            },
            Self {
                position: [ndc_left, ndc_bottom],
                color,
            },
            Self {
                position: [ndc_right, ndc_bottom],
                color,
            },
            Self {
                position: [ndc_left, ndc_top],
                color,
            },
            Self {
                position: [ndc_right, ndc_bottom],
                color,
            },
            Self {
                position: [ndc_right, ndc_top],
                color,
            },
        ]
    }
}

/// Creates a vertex buffer from a slice of `ColoredVertex`. Uploads one
/// zero vertex when the slice is empty (wgpu requires non-zero buffers);
/// the caller must set `vertex_count` to 0 to skip drawing.
pub fn create_colored_vertex_buffer(
    device: &wgpu::Device,
    vertices: &[ColoredVertex],
) -> wgpu::Buffer {
    let vertices = if vertices.is_empty() {
        &[ColoredVertex::default()]
    } else {
        vertices
    };
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("colored vertex buffer"),
        contents: bytemuck::cast_slice(vertices),
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
    })
}

/// WGSL for untextured per-vertex color quads (`ColoredVertex` layout).
const COLORED_QUAD_SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
}
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
}
@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(in.position, 0.0, 1.0);
    out.color = in.color;
    return out;
}
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
"#;

/// Builds the untextured colored-quad pipeline once at `GpuContext` construction.
/// Layers clone the `Arc` instead of creating their own GPU objects.
fn create_colored_quad_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    label: &str,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(COLORED_QUAD_SHADER.into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[],
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(ColoredVertex::layout())],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

// ── Shared GPU helpers ────────────────────────────────────────────────────

/// Bind group layout used by the text layer (texture + sampler).
pub fn create_texture_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
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
pub fn create_vertex_buffer(device: &wgpu::Device, vertices: &[TexturedVertex]) -> wgpu::Buffer {
    let vertices = if vertices.is_empty() {
        &[TexturedVertex {
            position: [0.0, 0.0],
            tex_coords: [0.0, 0.0],
            color: [1.0; 4],
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
