use wgpu::util::DeviceExt;

use crate::model::DirtyRange;
/// Upload operation selected for a dirty grid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UploadMode {
    None,
    Incremental,
    Full,
}

/// Pure upload decision, separated from wgpu so it can be tested headlessly.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UploadPlan {
    pub mode: UploadMode,
    pub dirty_range_count: usize,
    pub dirty_cells: usize,
    pub dirty_bytes: usize,
    pub full_bytes: usize,
}

/// Chooses full writes when fragmented or broad damage makes them cheaper.
#[derive(Clone, Copy, Debug)]
pub struct UploadPolicy {
    full_upload_ratio: f64,
    max_incremental_ranges: usize,
}

impl Default for UploadPolicy {
    fn default() -> Self {
        Self {
            full_upload_ratio: 0.5,
            max_incremental_ranges: 64,
        }
    }
}

impl UploadPolicy {
    pub fn decide(
        self,
        rows: usize,
        cols: usize,
        bytes_per_cell: usize,
        dirty_ranges: &[DirtyRange],
        force_full: bool,
    ) -> UploadPlan {
        let dirty_cells = dirty_ranges.iter().fold(0usize, |total, range| {
            total.saturating_add(range.end_col.saturating_sub(range.start_col))
        });
        let dirty_bytes = dirty_cells.saturating_mul(bytes_per_cell);
        let full_bytes = rows.saturating_mul(cols).saturating_mul(bytes_per_cell);
        if force_full {
            return UploadPlan {
                mode: UploadMode::Full,
                dirty_range_count: dirty_ranges.len(),
                dirty_cells,
                dirty_bytes,
                full_bytes,
            };
        }
        if dirty_ranges.is_empty() {
            return UploadPlan {
                mode: UploadMode::None,
                dirty_range_count: 0,
                dirty_cells,
                dirty_bytes,
                full_bytes,
            };
        }
        let ratio = if full_bytes == 0 {
            1.0
        } else {
            dirty_bytes as f64 / full_bytes as f64
        };
        let mode = if ratio >= self.full_upload_ratio
            || dirty_ranges.len() > self.max_incremental_ranges
        {
            UploadMode::Full
        } else {
            UploadMode::Incremental
        };
        UploadPlan {
            mode,
            dirty_range_count: dirty_ranges.len(),
            dirty_cells,
            dirty_bytes,
            full_bytes,
        }
    }
}

/// Returns true only for the alpha mode supported by the terminal's current
/// straight-source blend pipelines. `PostMultiplied`, `Auto`, and `Inherit`
/// are intentionally excluded until a matching pipeline path is implemented.
pub const fn alpha_mode_supports_transparency(mode: wgpu::CompositeAlphaMode) -> bool {
    matches!(mode, wgpu::CompositeAlphaMode::PreMultiplied)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(row: usize, start_col: usize, end_col: usize) -> DirtyRange {
        DirtyRange {
            row,
            start_col,
            end_col,
        }
    }

    #[test]
    fn should_only_allow_supported_compositing_mode_for_transparency() {
        use wgpu::CompositeAlphaMode::{Auto, Opaque, PostMultiplied, PreMultiplied};
        assert!(alpha_mode_supports_transparency(PreMultiplied));
        assert!(!alpha_mode_supports_transparency(PostMultiplied));
        assert!(!alpha_mode_supports_transparency(Auto));
        assert!(!alpha_mode_supports_transparency(Opaque));
        assert!(!alpha_mode_supports_transparency(
            wgpu::CompositeAlphaMode::Inherit
        ));
    }

    #[test]
    fn upload_policy_selects_none_incremental_and_full_uploads() {
        let policy = UploadPolicy::default();
        assert_eq!(policy.decide(2, 2, 4, &[], false).mode, UploadMode::None);
        assert_eq!(
            policy.decide(10, 10, 4, &[range(2, 1, 2)], false).mode,
            UploadMode::Incremental
        );
        assert_eq!(
            policy
                .decide(
                    10,
                    10,
                    4,
                    &[
                        range(0, 0, 10),
                        range(1, 0, 10),
                        range(2, 0, 10),
                        range(3, 0, 10),
                        range(4, 0, 10)
                    ],
                    false
                )
                .mode,
            UploadMode::Full
        );
    }

    #[test]
    fn upload_policy_uses_full_upload_for_fragmented_or_forced_damage() {
        let policy = UploadPolicy::default();
        let fragmented = (0..65).map(|row| range(row, 0, 1)).collect::<Vec<_>>();
        assert_eq!(
            policy.decide(100, 100, 4, &fragmented, false).mode,
            UploadMode::Full
        );
        assert_eq!(
            policy.decide(2, 2, 8, &[range(1, 1, 2)], true).mode,
            UploadMode::Full
        );
    }
}
/// Borrowed terminal GPU capabilities valid only for resource creation or one draw callback.
#[derive(Clone, Copy)]
pub struct TerminalGpuAccess<'frame> {
    device: &'frame wgpu::Device,
    queue: &'frame wgpu::Queue,
    format: wgpu::TextureFormat,
    upload_policy: UploadPolicy,
}

impl<'frame> TerminalGpuAccess<'frame> {
    pub fn new(
        device: &'frame wgpu::Device,
        queue: &'frame wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Self {
        Self {
            device,
            queue,
            format,
            upload_policy: UploadPolicy::default(),
        }
    }

    pub fn device(self) -> &'frame wgpu::Device {
        self.device
    }

    pub fn queue(self) -> &'frame wgpu::Queue {
        self.queue
    }

    pub const fn format(self) -> wgpu::TextureFormat {
        self.format
    }

    pub fn upload_plan(
        self,
        rows: usize,
        cols: usize,
        bytes_per_cell: usize,
        dirty_ranges: &[DirtyRange],
        force_full: bool,
    ) -> UploadPlan {
        self.upload_policy
            .decide(rows, cols, bytes_per_cell, dirty_ranges, force_full)
    }

    pub fn write_buffer(self, buffer: &wgpu::Buffer, offset: wgpu::BufferAddress, data: &[u8]) {
        self.queue.write_buffer(buffer, offset, data);
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

/// Builds the terminal's shared untextured colored-quad pipeline.
pub(super) fn create_colored_quad_pipeline(
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
/// Creates an uninitialized vertex buffer of exactly `vertex_count` vertices.
///
/// This avoids allocating a temporary zero-filled CPU vertex array during resize.
pub fn create_vertex_buffer_sized(device: &wgpu::Device, vertex_count: usize) -> wgpu::Buffer {
    let byte_len = vertex_count
        .checked_mul(std::mem::size_of::<TexturedVertex>())
        .expect("vertex buffer size overflow");
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("vertex buffer"),
        size: byte_len.max(std::mem::size_of::<TexturedVertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
