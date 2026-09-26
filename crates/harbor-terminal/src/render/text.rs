use crate::model::{Cell, TerminalSnapshot};
use harbor_config::Palette;

use anyhow::Result;
use wgpu::util::DeviceExt;

use super::gpu::{self, TerminalGpuAccess, TexturedVertex, UploadMode};
use crate::render::{RenderViewport, layout_preedit};
use crate::{CellAttrs, Color, DirtyRange, Preedit};
#[cfg(test)]
use harbor_config::BACKGROUND;
use harbor_text::atlas::MAX_ATLAS_SIZE;
use harbor_text::{FontBook, GlyphAtlas, TextMetrics};

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

/// Computes glyph color using the built-in palette.
pub fn glyph_color(fg: Color, bg: Color, attrs: CellAttrs) -> [f32; 4] {
    glyph_color_with_palette(&Palette::default(), fg, bg, attrs)
}

/// Computes glyph color using an injected startup palette.
pub fn glyph_color_with_palette(
    palette: &Palette,
    fg: Color,
    bg: Color,
    attrs: CellAttrs,
) -> [f32; 4] {
    if attrs.contains(CellAttrs::INVERSE) {
        if bg == Color::Default {
            let [red, green, blue, _] = palette.background.components();
            [red, green, blue, 1.0]
        } else {
            palette.resolve(bg)
        }
    } else {
        palette.resolve(fg)
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
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
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

// The scalar atlas cannot shape a whole emoji sequence. Draw the base glyph
// within its assigned cells, plus combining marks; selectors and ZWJ are
// retained for copy but have no independent visual glyph. A joined emoji may
// therefore appear as its first pictograph, not a color/ligature emoji.
fn is_selector(ch: char) -> bool {
    matches!(ch, '\u{fe00}'..='\u{fe0f}' | '\u{e0100}'..='\u{e01ef}')
}

/// Scalars needed by both the initial atlas and incremental dirty uploads.
fn paint_chars(cell: &Cell) -> Vec<char> {
    if cell.wide_continuation {
        return Vec::new();
    }
    let mut chars = Vec::new();
    if cell.isolated_mark {
        chars.push('\u{25cc}');
    }
    if cell.ch != ' ' {
        chars.push(cell.ch);
    }
    chars.extend(cell.suffix.chars().filter(|&ch| {
        ch != '\u{200d}'
            && !is_selector(ch)
            && unicode_width::UnicodeWidthChar::width(ch) == Some(0)
    }));
    chars
}

/// Text rendering: glyph atlas + vertex buffer for every grid cell.
pub struct Text {
    fonts: FontBook,
    metrics: TextMetrics,
    pipeline: wgpu::RenderPipeline,
    atlas: GlyphAtlas,
    gpu_atlas: GpuGlyphAtlas,
    vertex_buffer: wgpu::Buffer,
    overlay_vertex_buffer: wgpu::Buffer,
    overlay_vertex_capacity: usize,
    overlay_vertex_count: u32,
    dirty: bool,
    rows: usize,
    cols: usize,
    palette: Palette,
}

impl Text {
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Apply CPU atlas changes to the GPU atlas / vertex dirty state.
    fn apply_rasterize_result(
        &mut self,
        gpu: TerminalGpuAccess<'_>,
        result: harbor_text::RasterizeResult,
    ) {
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
        gpu: TerminalGpuAccess<'_>,
        fonts: FontBook,
        metrics: TextMetrics,
        snap: &TerminalSnapshot,
        viewport: &RenderViewport,
        palette: Palette,
    ) -> Result<Self> {
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
        let overlay_vertex_buffer = gpu::create_vertex_buffer_sized(gpu.device(), 0);

        let mut layer = Self {
            fonts,
            metrics,
            pipeline,
            atlas,
            gpu_atlas,
            overlay_vertex_buffer,
            overlay_vertex_capacity: 0,
            overlay_vertex_count: 0,
            vertex_buffer,
            dirty: true,
            rows,
            cols,
            palette,
        };

        let verts = layer.build_all_vertices(snap, viewport);
        gpu.write_buffer(&layer.vertex_buffer, 0, bytemuck::cast_slice(&verts));
        layer.dirty = false;

        Ok(layer)
    }

    pub(crate) fn set_palette(&mut self, palette: Palette) {
        self.palette = palette;
        self.dirty = true;
    }

    pub fn invalidate_projection(&mut self) {
        self.dirty = true;
    }

    fn collect_all_chars(snap: &TerminalSnapshot) -> Vec<char> {
        let mut chars: Vec<char> = snap.cells.iter().flat_map(paint_chars).collect();
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
                (range.start_col..range.end_col)
                    .flat_map(move |col| paint_chars(snap.cell(range.row, col)))
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
        viewport: &RenderViewport,
    ) -> Vec<TexturedVertex> {
        self.build_range_vertices(
            &DirtyRange {
                row,
                start_col: 0,
                end_col: snap.cols,
            },
            snap,
            viewport,
        )
    }

    fn build_range_vertices(
        &self,
        range: &DirtyRange,
        snap: &TerminalSnapshot,
        viewport: &RenderViewport,
    ) -> Vec<TexturedVertex> {
        let (surf_w, surf_h) = viewport.surface_dimensions();
        let mut verts = Vec::with_capacity((range.end_col - range.start_col) * 6);
        for col in range.start_col..range.end_col {
            let cell = snap.cell(range.row, col);
            if cell.ch != ' '
                && !cell.isolated_mark
                && !cell.wide_continuation
                && let Some(glyph) = self.atlas.glyph_by_char(cell.ch)
                && glyph.width > 0
                && glyph.height > 0
            {
                let (cell_x, cell_y) = viewport.cell_pos(range.row, col);
                let baseline = cell_y + self.metrics.ascent.ceil();
                let mut glyph_left = cell_x + glyph.bearing_x as f32;
                let glyph_bottom = baseline - glyph.bearing_y as f32;
                let glyph_top = glyph_bottom - glyph.height as f32;
                let mut glyph_right = glyph_left + glyph.width as f32;

                if cell.attrs.contains(CellAttrs::ITALIC) {
                    let offset = self.metrics.cell_width * 0.15;
                    glyph_left += offset;
                    glyph_right += offset;
                }

                let color = glyph_color_with_palette(&self.palette, cell.fg, cell.bg, cell.attrs);

                // Scalar fallback never paints outside the unit's assigned cells.
                let clip_left = glyph_left.max(cell_x);
                let clip_right = glyph_right
                    .min(cell_x + self.metrics.cell_width * f32::from(cell.grid_width()));
                let clip_top = glyph_top.max(cell_y);
                let clip_bottom = glyph_bottom.min(cell_y + self.metrics.line_height);
                if clip_left < clip_right && clip_top < clip_bottom {
                    let u = |x: f32| {
                        glyph.uv.left
                            + (glyph.uv.right - glyph.uv.left) * (x - glyph_left)
                                / glyph.width as f32
                    };
                    let v = |y: f32| {
                        glyph.uv.top
                            + (glyph.uv.bottom - glyph.uv.top) * (y - glyph_top)
                                / glyph.height as f32
                    };
                    verts.extend_from_slice(&TexturedVertex::from_pixel_rect(
                        clip_left,
                        clip_top,
                        clip_right,
                        clip_bottom,
                        u(clip_left),
                        v(clip_top),
                        u(clip_right),
                        v(clip_bottom),
                        color,
                        surf_w,
                        surf_h,
                    ));
                } else {
                    verts.extend(std::iter::repeat_n(TexturedVertex::default(), 6));
                }
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
        viewport: &RenderViewport,
    ) -> Vec<TexturedVertex> {
        let mut verts = Vec::with_capacity(snap.rows * snap.cols * 6);
        for row in 0..snap.rows {
            verts.extend(self.build_row_vertices(row, snap, viewport));
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

    /// Rebuilds only the transient IME overlay while sharing the base glyph atlas.
    pub fn prepare_preedit(
        &mut self,
        gpu: TerminalGpuAccess<'_>,
        preedit: Option<&Preedit>,
        snap: &TerminalSnapshot,
        viewport: &RenderViewport,
    ) {
        let layout = preedit
            .filter(|preedit| !preedit.is_empty())
            .map(|preedit| {
                layout_preedit(
                    preedit,
                    (snap.cursor_x, snap.cursor_y),
                    snap.rows,
                    snap.cols,
                )
            });
        let (surf_w, surf_h) = viewport.surface_dimensions();
        let color = glyph_color_with_palette(
            &self.palette,
            Color::Default,
            Color::Default,
            CellAttrs::default(),
        );
        let mut vertices = Vec::new();
        for row in 0..snap.rows {
            for col in 0..snap.cols {
                let cell = snap.cell(row, col);
                if cell.wide_continuation {
                    continue;
                }
                let (cell_x, cell_y) = viewport.cell_pos(row, col);
                let baseline = cell_y + self.metrics.ascent.ceil();
                let cell_view = OverlayCell {
                    x: cell_x,
                    baseline,
                    color: glyph_color_with_palette(&self.palette, cell.fg, cell.bg, cell.attrs),
                    surface: (surf_w, surf_h),
                    width: cell.grid_width(),
                };
                if cell.isolated_mark {
                    append_overlay_glyph(
                        &self.atlas,
                        &self.metrics,
                        &mut vertices,
                        '\u{25cc}',
                        &cell_view,
                        false,
                    );
                }
                for mark in cell
                    .suffix
                    .chars()
                    .filter(|&ch| {
                        ch != '\u{200d}'
                            && !is_selector(ch)
                            && unicode_width::UnicodeWidthChar::width(ch) == Some(0)
                    })
                    .chain(cell.isolated_mark.then_some(cell.ch))
                {
                    append_overlay_glyph(
                        &self.atlas,
                        &self.metrics,
                        &mut vertices,
                        mark,
                        &cell_view,
                        true,
                    );
                }
            }
        }
        for positioned in layout.into_iter().flat_map(|layout| layout.glyphs) {
            let Some(glyph) = self.atlas.glyph_by_char(positioned.ch) else {
                continue;
            };
            if glyph.width == 0 || glyph.height == 0 {
                continue;
            }
            let (cell_x, cell_y) = viewport.cell_pos(positioned.row, positioned.col);
            let baseline = cell_y + self.metrics.ascent.ceil();
            let glyph_left = cell_x + glyph.bearing_x as f32;
            let glyph_bottom = baseline - glyph.bearing_y as f32;
            let glyph_top = glyph_bottom - glyph.height as f32;
            let glyph_right = glyph_left + glyph.width as f32;
            vertices.extend_from_slice(&TexturedVertex::from_pixel_rect(
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
        }

        if vertices.len() > self.overlay_vertex_capacity {
            let replacement = gpu::create_vertex_buffer_sized(gpu.device(), vertices.len());
            self.overlay_vertex_buffer = replacement;
            self.overlay_vertex_capacity = vertices.len();
        }
        if !vertices.is_empty() {
            gpu.write_buffer(
                &self.overlay_vertex_buffer,
                0,
                bytemuck::cast_slice(&vertices),
            );
        }
        self.overlay_vertex_count = vertices.len() as u32;
    }

    pub fn prepare_with_dirty(
        &mut self,
        gpu: TerminalGpuAccess<'_>,
        snap: &TerminalSnapshot,
        dirty_ranges: &[DirtyRange],
        viewport: &RenderViewport,
        preedit: Option<&Preedit>,
    ) {
        let resized = snap.rows != self.rows || snap.cols != self.cols;
        let bytes_per_cell = 6 * std::mem::size_of::<TexturedVertex>();

        if resized {
            tracing::trace!(rows = snap.rows, cols = snap.cols, "text layer resize");
            let mut all_chars = Self::collect_all_chars(snap);
            if let Some(preedit) = preedit {
                all_chars.extend(preedit.text.chars());
                all_chars.sort_unstable();
                all_chars.dedup();
            }
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
            let verts = self.build_all_vertices(snap, viewport);
            debug_assert_eq!(plan.mode, UploadMode::Full);
            gpu.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&verts));
            self.rows = snap.rows;
            self.cols = snap.cols;
            self.dirty = false;
            return;
        }

        let mut unique = Self::collect_unique_chars_from_dirty(snap, dirty_ranges);
        if let Some(preedit) = preedit {
            unique.extend(preedit.text.chars());
            unique.sort_unstable();
            unique.dedup();
        }
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
            let verts = self.build_all_vertices(snap, viewport);
            gpu.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&verts));
        } else {
            tracing::trace!("rebuilding text draw batch (incremental)");
            for range in dirty_ranges {
                let range_verts = self.build_range_vertices(range, snap, viewport);
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

    pub fn prepare(
        &mut self,
        gpu: TerminalGpuAccess<'_>,
        snap: Option<&TerminalSnapshot>,
        viewport: &RenderViewport,
        preedit: Option<&Preedit>,
    ) {
        if let Some(snap) = snap {
            self.prepare_with_dirty(gpu, snap, &snap.dirty_ranges, viewport, preedit);
        }
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.gpu_atlas.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        let vertex_count = (self.rows * self.cols * 6) as u32;
        if vertex_count > 0 {
            pass.draw(0..vertex_count, 0..1);
            if self.overlay_vertex_count > 0 {
                pass.set_vertex_buffer(0, self.overlay_vertex_buffer.slice(..));
                pass.draw(0..self.overlay_vertex_count, 0..1);
            }
        }
    }
}

/// Position and bounds shared by all extra glyphs painted over one grid cell.
struct OverlayCell {
    x: f32,
    baseline: f32,
    color: [f32; 4],
    surface: (f32, f32),
    width: u8,
}

/// Appends one actual atlas glyph quad at the originating grid cell.
fn append_overlay_glyph(
    atlas: &GlyphAtlas,
    metrics: &TextMetrics,
    vertices: &mut Vec<TexturedVertex>,
    ch: char,
    cell: &OverlayCell,
    center: bool,
) {
    let Some(glyph) = atlas.glyph_by_char(ch) else {
        return;
    };
    if glyph.width == 0 || glyph.height == 0 {
        return;
    }
    let left = if center {
        cell.x + (metrics.cell_width * cell.width as f32 - glyph.width as f32) * 0.5
    } else {
        cell.x + glyph.bearing_x as f32
    };
    let bottom = cell.baseline - glyph.bearing_y as f32;
    let top = bottom - glyph.height as f32;
    let right = left + glyph.width as f32;
    let clip_left = left.max(cell.x);
    let clip_right = right.min(cell.x + metrics.cell_width * cell.width as f32);
    let clip_top = top.max(cell.baseline - metrics.ascent.ceil());
    let clip_bottom = bottom.min(cell.baseline - metrics.ascent.ceil() + metrics.line_height);
    if clip_left >= clip_right || clip_top >= clip_bottom {
        return;
    }
    let u =
        |x: f32| glyph.uv.left + (glyph.uv.right - glyph.uv.left) * (x - left) / glyph.width as f32;
    let v =
        |y: f32| glyph.uv.top + (glyph.uv.bottom - glyph.uv.top) * (y - top) / glyph.height as f32;
    vertices.extend_from_slice(&TexturedVertex::from_pixel_rect(
        clip_left,
        clip_top,
        clip_right,
        clip_bottom,
        u(clip_left),
        v(clip_top),
        u(clip_right),
        v(clip_bottom),
        cell.color,
        cell.surface.0,
        cell.surface.1,
    ));
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
    fn sequence_fallback_rasterizes_base_not_format_or_joined_scalars() {
        let mut heart = Cell::default();
        heart.set(
            '♥',
            Color::Default,
            Color::Default,
            CellAttrs::default(),
            false,
        );
        heart.suffix.push('\u{fe0f}');
        heart.width = 2;
        assert_eq!(paint_chars(&heart), ['♥']);
        assert_eq!(heart.raw_text(), "♥️");
        let mut joined = Cell::default();
        joined.set(
            '👩',
            Color::Default,
            Color::Default,
            CellAttrs::default(),
            false,
        );
        joined.suffix.push_str("\u{200d}💻");
        assert_eq!(paint_chars(&joined), ['👩']);
        assert_eq!(joined.raw_text(), "👩‍💻");
    }

    #[test]
    fn combining_overlay_emits_mark_and_display_only_cue_quads() {
        let fonts = harbor_text::load_system_fonts(&harbor_config::FontSettings {
            family: None,
            size: 16.0,
        })
        .unwrap();
        let metrics = TextMetrics::from_font_metrics(fonts.font_metrics());
        let mut atlas = GlyphAtlas::new();
        atlas.rebuild(&fonts, &['e', '\u{0301}', '\u{25cc}']);
        let mut base = Cell::default();
        base.set(
            'e',
            Color::Default,
            Color::Default,
            CellAttrs::default(),
            false,
        );
        base.suffix.push('\u{0301}');
        let mut isolated = Cell::default();
        isolated.set(
            '\u{0301}',
            Color::Default,
            Color::Default,
            CellAttrs::default(),
            false,
        );
        isolated.isolated_mark = true;
        assert_eq!(paint_chars(&base), ['e', '\u{0301}']);
        assert_eq!(paint_chars(&isolated), ['\u{25cc}', '\u{0301}']);
        let mut verts = Vec::new();
        let cell_view = OverlayCell {
            x: 0.0,
            baseline: metrics.ascent,
            color: [1.0; 4],
            surface: (100.0, 100.0),
            width: 1,
        };
        for ch in paint_chars(&isolated) {
            append_overlay_glyph(
                &atlas,
                &metrics,
                &mut verts,
                ch,
                &cell_view,
                ch != '\u{25cc}',
            );
        }
        assert_eq!(
            verts.len(),
            12,
            "cue and mark must each contribute a real atlas quad"
        );
        verts.clear();
        append_overlay_glyph(&atlas, &metrics, &mut verts, '\u{0301}', &cell_view, true);
        assert_eq!(
            verts.len(),
            6,
            "combined mark must be painted over its base"
        );
    }

    #[test]
    fn inverse_default_glyph_uses_background_rgb() {
        let mut attrs = CellAttrs::default();
        attrs.set(CellAttrs::INVERSE);
        let color = glyph_color(Color::Default, Color::Default, attrs);
        assert_eq!(color, [BACKGROUND[0], BACKGROUND[1], BACKGROUND[2], 1.0]);
    }

    #[test]
    fn inverse_default_glyph_ignores_bold_for_color() {
        let mut attrs = CellAttrs::default();
        attrs.set(CellAttrs::BOLD);
        attrs.set(CellAttrs::INVERSE);
        let color = glyph_color(Color::Default, Color::Default, attrs);
        assert_eq!(color, [BACKGROUND[0], BACKGROUND[1], BACKGROUND[2], 1.0]);
    }

    #[test]
    fn inverse_named_fg_default_bg_glyph_uses_background_rgb() {
        let mut attrs = CellAttrs::default();
        attrs.set(CellAttrs::INVERSE);
        let color = glyph_color(Color::Named(1), Color::Default, attrs);
        assert_eq!(color, [BACKGROUND[0], BACKGROUND[1], BACKGROUND[2], 1.0]);
    }

    #[test]
    fn inverse_named_bg_default_fg_glyph_uses_bg_color() {
        let mut attrs = CellAttrs::default();
        attrs.set(CellAttrs::INVERSE);
        let color = glyph_color(Color::Default, Color::Named(1), attrs);
        assert_eq!(color, Color::Named(1).to_rgba());
    }

    #[test]
    fn injected_default_colors_control_normal_and_inverse_glyphs() {
        let palette = Palette {
            foreground: harbor_config::Rgba::from_rgba8(10, 20, 30, 128),
            background: harbor_config::Rgba::from_rgba8(40, 50, 60, 64),
            ..Palette::default()
        };
        assert_eq!(
            glyph_color_with_palette(
                &palette,
                Color::Default,
                Color::Default,
                CellAttrs::default(),
            ),
            palette.foreground.components()
        );

        let mut inverse = CellAttrs::default();
        inverse.set(CellAttrs::INVERSE);
        assert_eq!(
            glyph_color_with_palette(&palette, Color::Default, Color::Default, inverse,),
            [40.0 / 255.0, 50.0 / 255.0, 60.0 / 255.0, 1.0]
        );
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
