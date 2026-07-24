use harbor_types::RenderSnapshot;

use anyhow::Result;
use wgpu::util::DeviceExt;

use crate::{
    Component,
    gpu::{self, GpuContext, TexturedVertex},
};
use harbor_config::TEXT_PADDING;
use harbor_terminal::{CellAttrs, Color, DirtyRange, TerminalSize};
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
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("glyph atlas sampler"),
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
                    resource: wgpu::BindingResource::TextureView(&texture_view),
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

    /// Uploads new glyph tiles into the pre-allocated 2048×2048 texture.
    fn update_glyphs(&self, queue: &wgpu::Queue, atlas: &GlyphAtlas, new_chars: &[char]) {
        for ch in new_chars {
            let Some(glyph) = atlas.glyph(*ch) else {
                continue;
            };
            if glyph.width == 0 || glyph.height == 0 {
                continue;
            }
            // Extract glyph bitmap from the atlas pixels, padding each row
            // to COPY_BYTES_PER_ROW_ALIGNMENT (256).
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

// ── TextLayer ─────────────────────────────────────────────────────────────

/// Holds the text render pipeline, glyph atlas, bind group layout, and pre-allocated vertex buffer.
pub struct Text {
    /// Loaded font set (primary + fallback fonts).
    fonts: FontBook,
    /// Cell dimensions and baseline metrics.
    metrics: TextMetrics,
    /// wgpu render pipeline.
    pipeline: wgpu::RenderPipeline,
    /// Texture bind group layout (shared via create_texture_bind_group_layout).
    bind_group_layout: wgpu::BindGroupLayout,
    /// CPU-side greyscale atlas + glyph lookup.
    atlas: GlyphAtlas,
    /// GPU-side atlas texture + bind group.
    gpu_atlas: GpuGlyphAtlas,
    /// Pre-allocated vertex buffer with COPY_DST for incremental uploads.
    vertex_buffer: wgpu::Buffer,
    /// Whether vertices need full re-upload.
    dirty: bool,
    /// Cached grid dimensions.
    rows: usize,
    cols: usize,
}

impl Text {
    /// Creates the text render pipeline, bind group layout, initial glyph atlas,
    /// and pre-allocated vertex buffer from the given GPU context and snap.
    pub fn new(
        gpu: &GpuContext,
        fonts: FontBook,
        metrics: TextMetrics,
        snap: &RenderSnapshot,
    ) -> Result<Self> {
        let (surf_w, surf_h) = gpu.surface_size();
        tracing::info!(
            surf_w,
            surf_h,
            rows = snap.rows,
            cols = snap.cols,
            "creating text layer"
        );
        let bind_group_layout = gpu::create_texture_bind_group_layout(gpu.device());
        let pipeline = Self::create_pipeline(gpu.device(), gpu.format(), &bind_group_layout);

        let mut atlas = GlyphAtlas::new();
        let all_chars = Self::collect_all_chars(snap);
        atlas.rebuild(&fonts, &all_chars);
        tracing::info!(glyphs = atlas.len(), "glyph atlas initialized");
        let gpu_atlas = GpuGlyphAtlas::new(gpu.device(), gpu.queue(), &bind_group_layout, &atlas);

        // Pre-allocate vertex buffer for the full grid (rows * cols * 6 vertices).
        let rows = snap.rows;
        let cols = snap.cols;
        let max_vertices = rows * cols * 6;
        let vertex_buffer = gpu::create_vertex_buffer(
            gpu.device(),
            &vec![TexturedVertex::default(); max_vertices.max(1)],
        );

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
        // Build initial vertex data and upload via write_buffer.
        let verts = layer.build_all_vertices(snap, surf_w as f32, surf_h as f32);
        gpu.queue()
            .write_buffer(&layer.vertex_buffer, 0, bytemuck::cast_slice(&verts));
        layer.dirty = false;

        Ok(layer)
    }

    /// Returns the terminal grid dimensions that fit the current surface given font metrics.
    pub fn terminal_size(&self, gpu: &GpuContext) -> TerminalSize {
        let (w, h) = gpu.surface_size();
        self.metrics.terminal_size(w, h)
    }

    /// Font metrics (cell dimensions, ascent, etc.).
    pub fn metrics(&self) -> &TextMetrics {
        &self.metrics
    }

    /// Looks up a glyph in the CPU-side atlas.
    pub fn glyph(&self, ch: char) -> Option<&AtlasGlyph> {
        self.atlas.glyph(ch)
    }

    /// The text render pipeline (glyph atlas texture bind group layout).
    pub fn text_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.pipeline
    }

    /// The bind group holding the glyph atlas texture and sampler.
    pub fn text_bind_group(&self) -> &wgpu::BindGroup {
        &self.gpu_atlas.bind_group
    }

    /// Ensures all characters in `text` are rasterized and uploaded to the GPU atlas.
    /// Call before building text vertices for dialog labels that reference the atlas.
    pub fn ensure_glyphs(&mut self, text: &str, _device: &wgpu::Device, queue: &wgpu::Queue) {
        let mut chars: Vec<char> = text.chars().filter(|&c| c != ' ').collect();
        chars.sort_unstable();
        chars.dedup();
        let result = self.atlas.rasterize_new(&self.fonts, &chars);
        if !result.new_chars.is_empty() {
            self.gpu_atlas
                .update_glyphs(queue, &self.atlas, &result.new_chars);
        }
    }

    /// Collects unique non-space characters from dirty ranges.
    fn collect_unique_chars_from_dirty(
        snap: &RenderSnapshot,
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

    /// Collects unique non-space characters from the entire visible snapshot.
    fn collect_all_chars(snap: &RenderSnapshot) -> Vec<char> {
        let mut chars: Vec<char> = snap
            .cells
            .iter()
            .filter_map(|cell| if cell.ch != ' ' { Some(cell.ch) } else { None })
            .collect();
        chars.sort_unstable();
        chars.dedup();
        chars
    }

    /// Builds the 6 * cols vertices for one row at fixed offsets. Blank cells → degenerate quad.
    fn build_row_vertices(
        &self,
        row: usize,
        snap: &RenderSnapshot,
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
        snap: &RenderSnapshot,
        surf_w: f32,
        surf_h: f32,
    ) -> Vec<TexturedVertex> {
        let mut verts = Vec::with_capacity((range.end_col - range.start_col) * 6);
        for col in range.start_col..range.end_col {
            let cell = snap.cell(range.row, col);
            if cell.ch != ' '
                && let Some(glyph) = self.atlas.glyph(cell.ch)
                && glyph.width > 0
                && glyph.height > 0
            {
                let cell_x = TEXT_PADDING + col as f32 * self.metrics.cell_width;
                let baseline = TEXT_PADDING
                    + self.metrics.ascent.ceil()
                    + range.row as f32 * self.metrics.line_height;
                let mut glyph_left = cell_x + glyph.xmin as f32;
                let glyph_bottom = baseline - glyph.ymin as f32;
                let glyph_top = glyph_bottom - glyph.height as f32;
                let mut glyph_right = glyph_left + glyph.width as f32;

                // Italic: shift glyph right for a subtle lean.
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
            // Blank cell → 6 degenerate vertices (zero-area quad, rasterizer drops it).
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

    /// Builds vertices for every row in the full grid.
    fn build_all_vertices(
        &self,
        snap: &RenderSnapshot,
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
            label: Some("glyph atlas shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("glyph atlas pipeline layout"),
            bind_group_layouts: &[Some(bind_group_layout)],
            immediate_size: 0,
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("glyph atlas pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(TexturedVertex::layout())],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
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
            multiview_mask: None,
            cache: None,
        })
    }
}

impl Text {
    pub fn prepare_with_dirty(
        &mut self,
        gpu: &GpuContext,
        snap: &RenderSnapshot,
        dirty_ranges: &[DirtyRange],
    ) {
        let (surf_w, surf_h) = gpu.surface_size();

        // Detect resize: dimensions changed → full rebuild.
        if snap.rows != self.rows || snap.cols != self.cols {
            tracing::trace!(rows = snap.rows, cols = snap.cols, "text layer resize");
            let all_chars = Self::collect_all_chars(snap);
            self.atlas.rebuild(&self.fonts, &all_chars);
            self.gpu_atlas = GpuGlyphAtlas::new(
                gpu.device(),
                gpu.queue(),
                &self.bind_group_layout,
                &self.atlas,
            );
            let new_cap = snap.rows * snap.cols * 6;
            let old_cap = self.rows * self.cols * 6;
            if new_cap > old_cap {
                self.vertex_buffer = gpu::create_vertex_buffer(
                    gpu.device(),
                    &vec![TexturedVertex::default(); new_cap.max(1)],
                );
            }
            let verts = self.build_all_vertices(snap, surf_w as f32, surf_h as f32);
            gpu.queue()
                .write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&verts));
            self.rows = snap.rows;
            self.cols = snap.cols;
            self.dirty = false;
            return;
        }

        // Atlas update: incremental rasterization of new glyphs.
        let unique = Self::collect_unique_chars_from_dirty(snap, dirty_ranges);
        let result = self.atlas.rasterize_new(&self.fonts, &unique);
        if !result.new_chars.is_empty() {
            tracing::debug!(
                new_glyphs = result.new_chars.len(),
                total_glyphs = self.atlas.len(),
                "uploading new glyph tiles"
            );
            self.gpu_atlas
                .update_glyphs(gpu.queue(), &self.atlas, &result.new_chars);

            // Incremental addition: new UVs only affect dirty rows,
            // which already rebuild via build_range_vertices below.
            if result.evicted {
                self.dirty = true; // atlas cleared → all UVs invalidated
            }
        }

        // Dirty check: skip upload if nothing changed.
        if !self.dirty && dirty_ranges.is_empty() {
            return;
        }

        if self.dirty {
            tracing::trace!("rebuilding text draw batch (full)");
            let verts = self.build_all_vertices(snap, surf_w as f32, surf_h as f32);
            gpu.queue()
                .write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&verts));
        } else {
            tracing::trace!("rebuilding text draw batch (incremental)");
            for range in dirty_ranges {
                let range_verts =
                    self.build_range_vertices(range, snap, surf_w as f32, surf_h as f32);
                let offset = (range.row * snap.cols + range.start_col)
                    * 6
                    * std::mem::size_of::<TexturedVertex>();
                gpu.queue().write_buffer(
                    &self.vertex_buffer,
                    offset as u64,
                    bytemuck::cast_slice(&range_verts),
                );
            }
        }
        self.dirty = false;
    }
}

impl Component for Text {
    fn prepare(&mut self, gpu: &GpuContext, snap: Option<&RenderSnapshot>) {
        if let Some(snap) = snap {
            self.prepare_with_dirty(gpu, snap, &snap.dirty_ranges);
        }
    }

    fn draw(&self, pass: &mut wgpu::RenderPass) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.gpu_atlas.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        let vertex_count = (self.rows * self.cols * 6) as u32;
        if vertex_count > 0 {
            pass.draw(0..vertex_count, 0..1);
        }
    }
    fn resize(&mut self, _gpu: &GpuContext, _size: (u32, u32)) {
        self.dirty = true;
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::Text;
    use super::glyph_color;
    use harbor_terminal::{CellAttrs, Color};
    use harbor_types::{Cell, CursorShape, DirtyRange, RenderSnapshot};

    #[test]
    fn glyph_color_bold_uses_fg_color() {
        assert_eq!(
            glyph_color(Color::Named(1), Color::Default, CellAttrs::default()),
            Color::Named(1).to_rgba(),
            "normal cell uses fg color"
        );
        let mut bold = CellAttrs::default();
        bold.set(CellAttrs::BOLD);
        let fg = Color::Named(2);
        assert_eq!(
            glyph_color(fg, Color::Default, bold),
            fg.to_rgba(),
            "bold cell uses fg color, not white"
        );
    }

    #[test]
    fn glyph_color_inverse_swaps_to_bg() {
        let fg = Color::Named(2);
        let bg = Color::Named(4);
        let mut inv = CellAttrs::default();
        inv.set(CellAttrs::INVERSE);
        assert_eq!(
            glyph_color(fg, bg, inv),
            bg.to_rgba(),
            "inverse cell uses bg color"
        );
    }

    #[test]
    fn glyph_color_inverse_default_bg_returns_white() {
        let white = [1.0, 1.0, 1.0, 1.0];
        let mut inv = CellAttrs::default();
        inv.set(CellAttrs::INVERSE);
        assert_eq!(
            glyph_color(Color::Named(1), Color::Default, inv),
            white,
            "inverse with default bg returns white"
        );
    }

    #[test]
    fn glyph_color_bold_with_inverse() {
        let fg = Color::Named(2);
        let bg = Color::Named(4);
        let mut attrs = CellAttrs::default();
        attrs.set(CellAttrs::BOLD);
        attrs.set(CellAttrs::INVERSE);
        // Bold no longer overrides; inverse swaps fg↔bg.
        assert_eq!(
            glyph_color(fg, bg, attrs),
            bg.to_rgba(),
            "bold+inverse: inverse applies (bg wins)"
        );
    }

    // ── char collection helpers ────────────────────────────────────────────

    fn make_cell(ch: char) -> Cell {
        Cell {
            ch,
            wide_continuation: false,
            fg: Color::Default,
            bg: Color::Default,
            attrs: CellAttrs::default(),
            protected: false,
        }
    }

    fn make_snapshot(rows: usize, cols: usize, chars: &[char]) -> RenderSnapshot {
        let cells: Vec<Cell> = chars.iter().map(|&ch| make_cell(ch)).collect();
        assert_eq!(cells.len(), rows * cols);
        RenderSnapshot {
            rows,
            cols,
            cells,
            cursor_x: 0,
            cursor_y: 0,
            cursor_visible: false,
            cursor_blink: false,
            cursor_shape: CursorShape::default(),
            scroll_count: 0,
            view_offset: 0,
            history_start: 0,
            is_alt: false,
            dirty_ranges: Vec::new(),
        }
    }

    #[test]
    fn collect_all_chars_filters_spaces() {
        let snap = make_snapshot(1, 4, &['a', ' ', 'b', ' ']);
        let chars = Text::collect_all_chars(&snap);
        assert_eq!(chars, vec!['a', 'b']);
    }

    #[test]
    fn collect_all_chars_deduplicates() {
        let snap = make_snapshot(2, 2, &['a', 'b', 'a', 'c']);
        let chars = Text::collect_all_chars(&snap);
        assert_eq!(chars, vec!['a', 'b', 'c']);
    }

    #[test]
    fn collect_all_chars_all_spaces_returns_empty() {
        let snap = make_snapshot(1, 3, &[' ', ' ', ' ']);
        let chars = Text::collect_all_chars(&snap);
        assert!(chars.is_empty());
    }

    #[test]
    fn collect_unique_chars_from_dirty_filters_spaces() {
        let snap = make_snapshot(2, 4, &['a', ' ', 'b', ' ', ' ', 'c', ' ', 'd']);
        let dirty = vec![
            DirtyRange {
                row: 0,
                start_col: 0,
                end_col: 4,
            },
            DirtyRange {
                row: 1,
                start_col: 0,
                end_col: 4,
            },
        ];
        let chars = Text::collect_unique_chars_from_dirty(&snap, &dirty);
        assert_eq!(chars, vec!['a', 'b', 'c', 'd']);
    }

    #[test]
    fn collect_unique_chars_from_dirty_deduplicates_across_ranges() {
        let snap = make_snapshot(2, 3, &['a', 'b', 'c', 'a', 'b', 'd']);
        let dirty = vec![
            DirtyRange {
                row: 0,
                start_col: 0,
                end_col: 3,
            },
            DirtyRange {
                row: 1,
                start_col: 0,
                end_col: 3,
            },
        ];
        let chars = Text::collect_unique_chars_from_dirty(&snap, &dirty);
        assert_eq!(chars, vec!['a', 'b', 'c', 'd']);
    }

    #[test]
    fn collect_unique_chars_from_dirty_subset_range() {
        let snap = make_snapshot(1, 5, &['x', 'y', 'a', 'z', 'y']);
        let dirty = vec![DirtyRange {
            row: 0,
            start_col: 1,
            end_col: 4,
        }];
        // Only 'y', 'a', 'z' are in the dirty range.
        let chars = Text::collect_unique_chars_from_dirty(&snap, &dirty);
        assert_eq!(chars, vec!['a', 'y', 'z']);
    }

    #[test]
    fn collect_unique_chars_from_dirty_empty_ranges_returns_empty() {
        let snap = make_snapshot(1, 3, &['a', 'b', 'c']);
        let chars = Text::collect_unique_chars_from_dirty(&snap, &[]);
        assert!(chars.is_empty());
    }

    #[test]
    fn collect_unique_chars_from_dirty_all_spaces_returns_empty() {
        let snap = make_snapshot(1, 4, &[' ', ' ', ' ', ' ']);
        let dirty = vec![DirtyRange {
            row: 0,
            start_col: 0,
            end_col: 4,
        }];
        let chars = Text::collect_unique_chars_from_dirty(&snap, &dirty);
        assert!(chars.is_empty());
    }
}
