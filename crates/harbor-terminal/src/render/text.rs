use harbor_types::TerminalSnapshot;

use anyhow::Result;
use wgpu::util::DeviceExt;

use super::gpu::{self, GpuContext, TexturedVertex, UploadMode};
use crate::{CellAttrs, Color, DirtyRange, TerminalSize};
use harbor_config::TEXT_PADDING;
use harbor_text::atlas::MAX_ATLAS_SIZE;
use harbor_text::{AtlasGlyph, FontBook, GlyphAtlas, TextMetrics};

const SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) color: vec4<f32>,
}
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) color: vec4<f32>,
}
@group(0) @binding(0) var glyph_atlas: texture_2d<f32>;
@group(0) @binding(1) var glyph_sampler: sampler;
@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(in.position, 0.0, 1.0);
    out.tex_coords = in.tex_coords;
    out.color = in.color;
    return out;
}
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let alpha = textureSample(glyph_atlas, glyph_sampler, in.tex_coords).r;
    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
"#;

/// Computes the glyph color for a terminal cell based on its attributes.
/// Inverse swaps fg↔bg. Bold is rendered via the glyph's rasterised weight;
/// it does not change the foreground color.
pub fn glyph_color(fg: Color, bg: Color, attrs: CellAttrs) -> [f32; 4] {
    if attrs.contains(CellAttrs::INVERSE) {
        if bg == Color::Default {
            [1.0, 1.0, 1.0, 1.0]
        } else {
            bg.to_rgba()
        }
    } else {
        fg.to_rgba()
    }
}

// ── GPU glyph atlas ───────────────────────────────────────────────────────

/// GPU-side glyph atlas: texture, sampler, and bind group.
struct GpuGlyphAtlas {
    /// Atlas texture (held alive by this field).
    _texture: wgpu::Texture,
    /// Bind group consumed by the fragment shader (texture + sampler).
    bind_group: wgpu::BindGroup,
}

impl GpuGlyphAtlas {
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bind_group_layout: &wgpu::BindGroupLayout,
        atlas: &GlyphAtlas,
    ) -> Self {
        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("glyph atlas texture"),
                size: wgpu::Extent3d {
                    width: MAX_ATLAS_SIZE,
                    height: MAX_ATLAS_SIZE,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            atlas.pixels(),
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("glyph atlas sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("glyph atlas bind group"),
            layout: bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        Self {
            _texture: texture,
            bind_group,
        }
    }

    /// Re-uploads the complete CPU atlas after its glyph layout changes.
    fn update_full(&self, queue: &wgpu::Queue, atlas: &GlyphAtlas) {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self._texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            atlas.pixels(),
            wgpu::TexelCopyBufferLayout {
                bytes_per_row: Some(MAX_ATLAS_SIZE),
                rows_per_image: Some(MAX_ATLAS_SIZE),
                offset: 0,
            },
            wgpu::Extent3d {
                width: MAX_ATLAS_SIZE,
                height: MAX_ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Uploads new glyph tiles into the pre-allocated 2048×2048 texture.
    fn update_glyphs(
        &self,
        queue: &wgpu::Queue,
        atlas: &GlyphAtlas,
        new_keys: &[harbor_text::GlyphKey],
    ) {
        for key in new_keys {
            let Some(glyph) = atlas.glyph(*key) else {
                continue;
            };
            if glyph.width == 0 || glyph.height == 0 {
                continue;
            }
            let padded_bytes_per_row = glyph.width.div_ceil(256) * 256;
            let mut tile_data = vec![0u8; (padded_bytes_per_row * glyph.height) as usize];
            let pixels = atlas.pixels();
            for row in 0..glyph.height {
                let src_offset = ((glyph.atlas_y + row) * MAX_ATLAS_SIZE + glyph.atlas_x) as usize;
                let dst_offset = (row * padded_bytes_per_row) as usize;
                tile_data[dst_offset..dst_offset + glyph.width as usize]
                    .copy_from_slice(&pixels[src_offset..src_offset + glyph.width as usize]);
            }

            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self._texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: glyph.atlas_x,
                        y: glyph.atlas_y,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &tile_data,
                wgpu::TexelCopyBufferLayout {
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(glyph.height),
                    offset: 0,
                },
                wgpu::Extent3d {
                    width: glyph.width,
                    height: glyph.height,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
}

// ── TextLayer ────────────────────────────────────────────────────────────────

/// Text rendering: glyph atlas + vertex buffer for every grid cell.
pub struct Text {
    fonts: FontBook,
    metrics: TextMetrics,
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    atlas: GlyphAtlas,
    gpu_atlas: GpuGlyphAtlas,
    vertex_buffer: wgpu::Buffer,
    dirty: bool,
    rows: usize,
    cols: usize,
}

impl Text {
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn text_bind_group(&self) -> &wgpu::BindGroup {
        &self.gpu_atlas.bind_group
    }

    pub fn text_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    /// Looks up a glyph in the CPU-side atlas.
    pub fn glyph(&self, ch: char) -> Option<&AtlasGlyph> {
        self.atlas.glyph_by_char(ch)
    }

    /// Ensures dialog text characters are rasterized into the atlas.
    pub fn ensure_glyphs(&mut self, text: &str, gpu: &GpuContext) {
        let mut chars: Vec<char> = text.chars().filter(|&c| c != ' ').collect();
        chars.sort_unstable();
        chars.dedup();
        let result = self.atlas.rasterize_new(&self.fonts, &chars);
        self.apply_rasterize_result(gpu, result);
    }

    /// Apply CPU atlas changes to the GPU atlas / vertex dirty state.
    fn apply_rasterize_result(&mut self, gpu: &GpuContext, result: harbor_text::RasterizeResult) {
        match atlas_gpu_sync(&result) {
            AtlasGpuSync::None => {}
            AtlasGpuSync::Incremental => {
                self.gpu_atlas
                    .update_glyphs(gpu.queue(), &self.atlas, &result.new_keys);
            }
            AtlasGpuSync::FullWithVertexRebuild => {
                self.gpu_atlas.update_full(gpu.queue(), &self.atlas);
                self.dirty = true;
            }
        }
    }

    /// Font metrics (cell dimensions, ascent, etc.).
    pub fn metrics(&self) -> &TextMetrics {
        &self.metrics
    }

    /// Creates the text pipeline, rasterises all unique characters on the initial
    /// screen snapshot, and uploads vertex data for every cell.
    pub fn new(
        gpu: &GpuContext,
        fonts: FontBook,
        metrics: TextMetrics,
        snap: &TerminalSnapshot,
    ) -> Result<Self> {
        let (surf_w, surf_h) = gpu.surface_size();
        let bind_group_layout = gpu::create_texture_bind_group_layout(gpu.device());
        let pipeline = Self::create_pipeline(gpu.device(), gpu.format(), &bind_group_layout);

        let mut atlas = GlyphAtlas::new();
        let all_chars = Self::collect_all_chars(snap);
        atlas.rebuild(&fonts, &all_chars);
        let gpu_atlas = GpuGlyphAtlas::new(gpu.device(), gpu.queue(), &bind_group_layout, &atlas);

        let rows = snap.rows;
        let cols = snap.cols;
        let max_vertices = rows
            .checked_mul(cols)
            .and_then(|cells| cells.checked_mul(6))
            .expect("text vertex count overflow");
        let vertex_buffer = gpu::create_vertex_buffer_sized(gpu.device(), max_vertices);

        let mut layer = Self {
            fonts,
            metrics,
            pipeline,
            bind_group_layout,
            atlas,
            gpu_atlas,
            vertex_buffer,
            dirty: true,
            rows,
            cols,
        };

        let verts = layer.build_all_vertices(snap, surf_w as f32, surf_h as f32);
        gpu.write_buffer(&layer.vertex_buffer, 0, bytemuck::cast_slice(&verts));
        layer.dirty = false;

        Ok(layer)
    }

    pub fn terminal_size(&self, gpu: &GpuContext) -> TerminalSize {
        let (w, h) = gpu.surface_size();
        let avail_w = (w as f32 - 2.0 * TEXT_PADDING).max(0.0);
        let avail_h = (h as f32 - 2.0 * TEXT_PADDING).max(0.0);

        let cols = (avail_w / self.metrics.cell_width).floor() as usize;
        let rows = (avail_h / self.metrics.line_height).floor() as usize;

        TerminalSize {
            rows: rows.max(1),
            cols: cols.max(1),
        }
    }

    fn collect_all_chars(snap: &TerminalSnapshot) -> Vec<char> {
        let mut chars: Vec<char> = snap
            .cells
            .iter()
            .filter_map(|cell| if cell.ch != ' ' { Some(cell.ch) } else { None })
            .collect();
        chars.sort_unstable();
        chars.dedup();
        chars
    }

    fn collect_unique_chars_from_dirty(
        snap: &TerminalSnapshot,
        dirty_ranges: &[DirtyRange],
    ) -> Vec<char> {
        let mut chars: Vec<char> = dirty_ranges
            .iter()
            .flat_map(|range| {
                (range.start_col..range.end_col).filter_map(move |col| {
                    let ch = snap.cell_char(range.row, col);
                    if ch != ' ' { Some(ch) } else { None }
                })
            })
            .collect();
        chars.sort_unstable();
        chars.dedup();
        chars
    }

    fn build_row_vertices(
        &self,
        row: usize,
        snap: &TerminalSnapshot,
        surf_w: f32,
        surf_h: f32,
    ) -> Vec<TexturedVertex> {
        self.build_range_vertices(
            &DirtyRange {
                row,
                start_col: 0,
                end_col: snap.cols,
            },
            snap,
            surf_w,
            surf_h,
        )
    }

    fn build_range_vertices(
        &self,
        range: &DirtyRange,
        snap: &TerminalSnapshot,
        surf_w: f32,
        surf_h: f32,
    ) -> Vec<TexturedVertex> {
        let mut verts = Vec::with_capacity((range.end_col - range.start_col) * 6);
        for col in range.start_col..range.end_col {
            let cell = snap.cell(range.row, col);
            if cell.ch != ' '
                && let Some(glyph) = self.atlas.glyph_by_char(cell.ch)
                && glyph.width > 0
                && glyph.height > 0
            {
                let cell_x = TEXT_PADDING + col as f32 * self.metrics.cell_width;
                let baseline = TEXT_PADDING
                    + self.metrics.ascent.ceil()
                    + range.row as f32 * self.metrics.line_height;
                let mut glyph_left = cell_x + glyph.bearing_x as f32;
                let glyph_bottom = baseline - glyph.bearing_y as f32;
                let glyph_top = glyph_bottom - glyph.height as f32;
                let mut glyph_right = glyph_left + glyph.width as f32;

                if cell.attrs.contains(CellAttrs::ITALIC) {
                    let offset = self.metrics.cell_width * 0.15;
                    glyph_left += offset;
                    glyph_right += offset;
                }

                let color = glyph_color(cell.fg, cell.bg, cell.attrs);

                verts.extend_from_slice(&TexturedVertex::from_pixel_rect(
                    glyph_left,
                    glyph_top,
                    glyph_right,
                    glyph_bottom,
                    glyph.uv.left,
                    glyph.uv.top,
                    glyph.uv.right,
                    glyph.uv.bottom,
                    color,
                    surf_w,
                    surf_h,
                ));
                continue;
            }
            verts.extend(std::iter::repeat_n(
                TexturedVertex {
                    color: [0.0; 4],
                    ..Default::default()
                },
                6,
            ));
        }
        verts
    }

    fn build_all_vertices(
        &self,
        snap: &TerminalSnapshot,
        surf_w: f32,
        surf_h: f32,
    ) -> Vec<TexturedVertex> {
        let mut verts = Vec::with_capacity(snap.rows * snap.cols * 6);
        for row in 0..snap.rows {
            verts.extend(self.build_row_vertices(row, snap, surf_w, surf_h));
        }
        verts
    }

    fn create_pipeline(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        bind_group_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("text shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("text pipeline layout"),
            bind_group_layouts: &[Some(bind_group_layout)],
            immediate_size: 0,
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("text pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(TexturedVertex::layout())],
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

    pub fn prepare_with_dirty(
        &mut self,
        gpu: &GpuContext,
        snap: &TerminalSnapshot,
        dirty_ranges: &[DirtyRange],
    ) {
        let (surf_w, surf_h) = gpu.surface_size();
        let resized = snap.rows != self.rows || snap.cols != self.cols;
        let bytes_per_cell = 6 * std::mem::size_of::<TexturedVertex>();

        if resized {
            tracing::trace!(rows = snap.rows, cols = snap.cols, "text layer resize");
            let all_chars = Self::collect_all_chars(snap);
            self.atlas.rebuild(&self.fonts, &all_chars);
            self.gpu_atlas.update_full(gpu.queue(), &self.atlas);

            let new_cap = snap
                .rows
                .checked_mul(snap.cols)
                .and_then(|cells| cells.checked_mul(6))
                .expect("text vertex count overflow");
            let old_cap = self
                .rows
                .checked_mul(self.cols)
                .and_then(|cells| cells.checked_mul(6))
                .expect("text vertex count overflow");
            if new_cap > old_cap {
                let placeholder = gpu::create_vertex_buffer_sized(gpu.device(), 0);
                let old_buffer = std::mem::replace(&mut self.vertex_buffer, placeholder);
                drop(old_buffer);
                self.vertex_buffer = gpu::create_vertex_buffer_sized(gpu.device(), new_cap);
            }
            let plan = gpu.upload_plan(snap.rows, snap.cols, bytes_per_cell, dirty_ranges, true);
            let verts = self.build_all_vertices(snap, surf_w as f32, surf_h as f32);
            debug_assert_eq!(plan.mode, UploadMode::Full);
            gpu.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&verts));
            self.rows = snap.rows;
            self.cols = snap.cols;
            self.dirty = false;
            return;
        }

        let unique = Self::collect_unique_chars_from_dirty(snap, dirty_ranges);
        let result = self.atlas.rasterize_new(&self.fonts, &unique);
        self.apply_rasterize_result(gpu, result);

        let plan = gpu.upload_plan(
            snap.rows,
            snap.cols,
            bytes_per_cell,
            dirty_ranges,
            self.dirty,
        );
        if plan.mode == UploadMode::None {
            return;
        }

        if plan.mode == UploadMode::Full {
            tracing::trace!("rebuilding text draw batch (full)");
            let verts = self.build_all_vertices(snap, surf_w as f32, surf_h as f32);
            gpu.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&verts));
        } else {
            tracing::trace!("rebuilding text draw batch (incremental)");
            for range in dirty_ranges {
                let range_verts =
                    self.build_range_vertices(range, snap, surf_w as f32, surf_h as f32);
                let offset = (range.row * snap.cols + range.start_col)
                    * 6
                    * std::mem::size_of::<TexturedVertex>();
                gpu.write_buffer(
                    &self.vertex_buffer,
                    offset as u64,
                    bytemuck::cast_slice(&range_verts),
                );
            }
        }
        self.dirty = false;
    }

    pub fn prepare(&mut self, gpu: &GpuContext, snap: Option<&TerminalSnapshot>) {
        if let Some(snap) = snap {
            self.prepare_with_dirty(gpu, snap, &snap.dirty_ranges);
        }
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.gpu_atlas.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        let vertex_count = (self.rows * self.cols * 6) as u32;
        if vertex_count > 0 {
            pass.draw(0..vertex_count, 0..1);
        }
    }

    pub fn resize(&mut self, _gpu: &GpuContext, _size: (u32, u32)) {
        self.dirty = true;
    }
}

/// GPU atlas upload decision derived from a CPU [`harbor_text::RasterizeResult`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AtlasGpuSync {
    /// No new tiles — skip upload.
    None,
    /// Ordinary additions — upload only new glyph tiles.
    Incremental,
    /// Eviction repack — full texture upload and vertex rebuild.
    FullWithVertexRebuild,
}

fn atlas_gpu_sync(result: &harbor_text::RasterizeResult) -> AtlasGpuSync {
    if result.new_keys.is_empty() {
        AtlasGpuSync::None
    } else if result.evicted {
        AtlasGpuSync::FullWithVertexRebuild
    } else {
        AtlasGpuSync::Incremental
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harbor_text::{FaceId, FontSize, FontStyle, GlyphId, GlyphKey, RasterizeResult};

    #[test]
    fn default_colors_preserve_attributes() {
        let mut attrs = CellAttrs::default();
        attrs.set(CellAttrs::BOLD);
        attrs.set(CellAttrs::INVERSE);
        let color = glyph_color(Color::Default, Color::Default, attrs);
        assert_eq!(color, [1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn named_color_conversion() {
        let attrs = CellAttrs::default();
        let color = glyph_color(Color::Named(1), Color::Default, attrs);
        assert_eq!(color, Color::Named(1).to_rgba());
    }

    #[test]
    fn should_skip_upload_when_no_new_keys() {
        // Arrange
        let result = RasterizeResult {
            new_keys: Vec::new(),
            evicted: true,
        };

        // Act
        let action = atlas_gpu_sync(&result);

        // Assert
        assert_eq!(action, AtlasGpuSync::None);
    }

    #[test]
    fn should_choose_incremental_when_new_keys_without_eviction() {
        // Arrange
        let result = RasterizeResult {
            new_keys: vec![GlyphKey::new(
                FaceId::PRIMARY,
                GlyphId::new(1),
                FontSize::new(1.0).expect("valid test font size"),
                FontStyle::REGULAR,
            )],
            evicted: false,
        };

        // Act
        let action = atlas_gpu_sync(&result);

        // Assert
        assert_eq!(action, AtlasGpuSync::Incremental);
    }

    #[test]
    fn should_choose_full_rebuild_when_evicted() {
        // Arrange
        let result = RasterizeResult {
            new_keys: vec![GlyphKey::new(
                FaceId::new(1),
                GlyphId::new(2),
                FontSize::new(1.0).expect("valid test font size"),
                FontStyle::REGULAR,
            )],
            evicted: true,
        };

        // Act
        let action = atlas_gpu_sync(&result);

        // Assert
        assert_eq!(action, AtlasGpuSync::FullWithVertexRebuild);
    }
}
