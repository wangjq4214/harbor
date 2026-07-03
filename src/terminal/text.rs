use hashbrown::HashMap;

use anyhow::Result;
use fontdue::Metrics;
use wgpu::util::DeviceExt;

use crate::{
    font::FontBook,
    gpu::{self, GpuContext, TexturedVertex},
    metrics::{TEXT_PADDING, TextMetrics},
    render::Layer,
    terminal::{Screen, TerminalSize},
};

pub(crate) const FONT_SIZE: f32 = 24.0;
const ATLAS_PADDING: u32 = 1;
const SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) tex_coords: vec2<f32>,
}
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}
@group(0) @binding(0) var glyph_atlas: texture_2d<f32>;
@group(0) @binding(1) var glyph_sampler: sampler;
@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(in.position, 0.0, 1.0);
    out.tex_coords = in.tex_coords;
    return out;
}
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let alpha = textureSample(glyph_atlas, glyph_sampler, in.tex_coords).r;
    return vec4<f32>(1.0, 1.0, 1.0, alpha);
}
"#;

// ── CPU-side glyph atlas ──────────────────────────────────────────────────

/// Atlas placement and metrics for one rasterized glyph.
#[derive(Clone, Copy)]
struct AtlasGlyph {
    /// UV sub-region (unit texture coordinates).
    uv: AtlasUv,
    /// Glyph pixel width.
    width: u32,
    /// Glyph pixel height.
    height: u32,
    /// fontdue horizontal offset (sub-pixel).
    xmin: i32,
    /// fontdue vertical offset (sub-pixel).
    ymin: i32,
}

/// UV rectangle within the atlas texture.
#[derive(Clone, Copy)]
struct AtlasUv {
    /// Left UV boundary [0, 1].
    left: f32,
    /// Top UV boundary [0, 1].
    top: f32,
    /// Right UV boundary [0, 1].
    right: f32,
    /// Bottom UV boundary [0, 1].
    bottom: f32,
}

/// One rasterised glyph (used for repacking).
struct RasterizedGlyph {
    /// Source character.
    ch: char,
    /// fontdue metrics.
    metrics: Metrics,
    /// Greyscale bitmap (1 byte/pixel, 0 = transparent, 255 = opaque).
    bitmap: Vec<u8>,
}

/// CPU-side glyph atlas plus enough metrics to build cell quads.
struct GlyphAtlas {
    /// Atlas texture width (pixels).
    width: u32,
    /// Atlas texture height (pixels).
    height: u32,
    /// Flattened greyscale pixel data.
    pixels: Vec<u8>,
    /// Character → atlas placement / UV lookup.
    glyphs: HashMap<char, AtlasGlyph>,
    /// Currently rasterised glyph list (consumed by repack).
    rasterized: Vec<RasterizedGlyph>,
    /// Cell width derived from font metrics.
    cell_width: f32,
    /// Line height.
    line_height: f32,
    /// Baseline ascent.
    ascent: f32,
}

impl GlyphAtlas {
    /// Creates an empty 1×1 atlas and records cell metrics.
    fn new(text_metrics: TextMetrics) -> Self {
        Self {
            width: 1,
            height: 1,
            pixels: vec![0],
            glyphs: HashMap::new(),
            rasterized: Vec::new(),
            cell_width: text_metrics.cell_width,
            line_height: text_metrics.line_height,
            ascent: text_metrics.ascent,
        }
    }

    /// Rebuilds the atlas from all non-space characters currently on screen.
    ///
    /// Stale glyphs that are no longer visible are evicted so the atlas never grows past the
    /// current screen's glyph set.  This avoids unbounded single-row width growth at the cost of
    /// re-rasterising after scroll or content replacement.
    fn update(&mut self, fonts: &FontBook, screen: &Screen) -> bool {
        // Collect unique non-space characters from the screen.
        let mut chars: Vec<char> = screen
            .cells()
            .filter_map(|(_, _, ch)| if ch != ' ' { Some(ch) } else { None })
            .collect();
        chars.sort_unstable();
        chars.dedup();

        // Nothing to add and nothing to evict.
        if chars.iter().all(|ch| self.glyphs.contains_key(ch))
            && self.rasterized.len() == chars.len()
        {
            return false;
        }

        tracing::debug!(
            glyphs = chars.len(),
            "rebuilding glyph atlas from current screen"
        );
        self.rasterized.clear();
        for ch in chars {
            let (metrics, bitmap) = fonts.rasterize(ch, FONT_SIZE);
            self.rasterized.push(RasterizedGlyph {
                ch,
                metrics,
                bitmap,
            });
        }
        // `chars` was already sorted; repack consumes `rasterized` in its current order.
        self.repack();
        tracing::debug!(
            atlas_width = self.width,
            atlas_height = self.height,
            "glyph atlas repacked"
        );
        true
    }

    /// Repacks all rasterised glyphs into a single-row atlas and computes UVs.
    ///
    /// Layout is a single horizontal row with 1px padding after each glyph.
    /// Atlas height is the maximum glyph height.
    fn repack(&mut self) {
        if self.rasterized.is_empty() {
            self.width = 1;
            self.height = 1;
            self.pixels = vec![0];
            self.glyphs.clear();
            return;
        }

        // Width = sum of all glyph widths + padding; height = max glyph height.
        self.width = self
            .rasterized
            .iter()
            .map(|glyph| glyph.metrics.width as u32 + ATLAS_PADDING)
            .sum::<u32>()
            .max(1);
        self.height = self
            .rasterized
            .iter()
            .map(|glyph| glyph.metrics.height as u32)
            .max()
            .unwrap_or(1)
            .max(1);
        self.pixels = vec![0; (self.width * self.height) as usize];
        self.glyphs.clear();
        self.glyphs.reserve(self.rasterized.len());

        // Copy each glyph's bitmap and record UV coordinates.
        let mut atlas_x = 0;
        for glyph in &self.rasterized {
            let metrics = &glyph.metrics;
            for row in 0..metrics.height {
                let dst_start = row * self.width as usize + atlas_x as usize;
                let src_start = row * metrics.width;
                let src_end = src_start + metrics.width;
                self.pixels[dst_start..dst_start + metrics.width]
                    .copy_from_slice(&glyph.bitmap[src_start..src_end]);
            }

            let left = atlas_x as f32 / self.width as f32;
            let right = (atlas_x + metrics.width as u32) as f32 / self.width as f32;
            self.glyphs.insert(
                glyph.ch,
                AtlasGlyph {
                    uv: AtlasUv {
                        left,
                        top: 0.0,
                        right,
                        bottom: metrics.height as f32 / self.height as f32,
                    },
                    width: metrics.width as u32,
                    height: metrics.height as u32,
                    xmin: metrics.xmin,
                    ymin: metrics.ymin,
                },
            );
            atlas_x += metrics.width as u32 + ATLAS_PADDING;
        }
    }

    /// Converts non-empty terminal cells into one batched vertex list using atlas UVs.
    ///
    /// Returns 6 vertices per visible non-space cell (two triangles per quad).
    fn vertices(&self, screen: &Screen, surface_width: f32, surface_height: f32) -> Vec<TexturedVertex> {
        let mut vertices = Vec::new();

        for (row, col, ch) in screen.cells() {
            if ch == ' ' {
                continue;
            }
            let Some(glyph) = self.glyphs.get(&ch) else {
                continue;
            };
            if glyph.width == 0 || glyph.height == 0 {
                continue;
            }

            // Pixel coordinate: padding + column offset, adjusted by glyph xmin/ymin.
            let cell_x = TEXT_PADDING + col as f32 * self.cell_width;
            let baseline = TEXT_PADDING + self.ascent.ceil() + row as f32 * self.line_height;
            let glyph_left = cell_x + glyph.xmin as f32;
            let glyph_bottom = baseline - glyph.ymin as f32;
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
                surface_width,
                surface_height,
            ));
        }

        vertices
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
                    width: atlas.width,
                    height: atlas.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &atlas.pixels,
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
}

// ── TextLayer ─────────────────────────────────────────────────────────────

/// Holds the text render pipeline, glyph atlas, bind group layout, and per-frame vertex buffer.
pub(crate) struct TextLayer {
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
    /// Current frame vertex buffer (rebuilt each frame).
    vertex_buffer: wgpu::Buffer,
    /// Current frame vertex count (0 = nothing to draw).
    vertex_count: u32,
}

impl TextLayer {
    /// Creates the text render pipeline, bind group layout, initial glyph atlas,
    /// and first-frame vertex buffer from the given GPU context and screen.
    pub(crate) fn new(
        gpu: &GpuContext,
        fonts: FontBook,
        metrics: TextMetrics,
        screen: &Screen,
    ) -> Result<Self> {
        let (surf_w, surf_h) = gpu.surface_size();
        tracing::info!(
            surf_w,
            surf_h,
            rows = screen.rows(),
            cols = screen.cols(),
            "creating text layer"
        );
        let bind_group_layout = gpu::create_texture_bind_group_layout(gpu.device());
        let pipeline = Self::create_pipeline(gpu.device(), gpu.format(), &bind_group_layout);
        let mut atlas = GlyphAtlas::new(metrics);
        atlas.update(&fonts, screen);
        tracing::info!(
            glyphs = atlas.glyphs.len(),
            atlas_width = atlas.width,
            atlas_height = atlas.height,
            "glyph atlas initialized"
        );
        let gpu_atlas = GpuGlyphAtlas::new(gpu.device(), gpu.queue(), &bind_group_layout, &atlas);
        let vertices = atlas.vertices(screen, surf_w as f32, surf_h as f32);
        let vertex_count = vertices.len() as u32;
        let vertex_buffer = gpu::create_vertex_buffer(gpu.device(), &vertices);

        Ok(Self {
            fonts,
            metrics,
            pipeline,
            bind_group_layout,
            atlas,
            gpu_atlas,
            vertex_buffer,
            vertex_count,
        })
    }

    /// Returns the terminal grid dimensions that fit the current surface given font metrics.
    pub(crate) fn terminal_size(&self, gpu: &GpuContext) -> TerminalSize {
        let (w, h) = gpu.surface_size();
        self.metrics.terminal_size(w, h)
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

impl Layer for TextLayer {
    fn prepare(&mut self, gpu: &GpuContext, screen: Option<&Screen>) {
        let screen = screen.expect("text layer requires screen");
        let (surf_w, surf_h) = gpu.surface_size();

        let atlas_changed = self.atlas.update(&self.fonts, screen);
        if atlas_changed {
            tracing::debug!(
                glyphs = self.atlas.glyphs.len(),
                atlas_width = self.atlas.width,
                atlas_height = self.atlas.height,
                "glyph atlas changed; uploading texture"
            );
            self.gpu_atlas = GpuGlyphAtlas::new(
                gpu.device(),
                gpu.queue(),
                &self.bind_group_layout,
                &self.atlas,
            );
        }
        tracing::trace!(
            surf_w,
            surf_h,
            rows = screen.rows(),
            cols = screen.cols(),
            "rebuilding text draw batch"
        );
        let vertices = self.atlas.vertices(screen, surf_w as f32, surf_h as f32);
        self.vertex_count = vertices.len() as u32;
        self.vertex_buffer = gpu::create_vertex_buffer(gpu.device(), &vertices);
    }

    fn draw(&self, pass: &mut wgpu::RenderPass) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.gpu_atlas.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..self.vertex_count, 0..1);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{GlyphAtlas, TextMetrics};
    use crate::{font::load_system_fonts, terminal::Terminal};

    fn test_font_book() -> crate::font::FontBook {
        load_system_fonts().expect("load test font")
    }

    fn test_atlas(fonts: &crate::font::FontBook) -> GlyphAtlas {
        GlyphAtlas::new(TextMetrics::new(fonts))
    }

    #[test]
    fn atlas_contains_each_visible_glyph_once() {
        let fonts = test_font_book();
        let mut terminal = Terminal::new(2, 5);

        terminal.put_str("aa b\nc a");
        let mut atlas = test_atlas(&fonts);
        atlas.update(&fonts, terminal.screen());

        assert_eq!(atlas.glyphs.len(), 3);
        assert!(atlas.glyphs.contains_key(&'a'));
        assert!(atlas.glyphs.contains_key(&'b'));
        assert!(atlas.glyphs.contains_key(&'c'));
        assert!(!atlas.glyphs.contains_key(&' '));
        assert_eq!(atlas.pixels.len(), (atlas.width * atlas.height) as usize);
    }

    #[test]
    fn vertices_emit_one_quad_per_visible_cell() {
        let fonts = test_font_book();
        let mut terminal = Terminal::new(2, 4);

        terminal.put_str("a b\n c ");
        let mut atlas = test_atlas(&fonts);
        atlas.update(&fonts, terminal.screen());
        let vertices = atlas.vertices(terminal.screen(), 800.0, 600.0);

        assert_eq!(vertices.len(), 18);
        assert!(vertices.iter().all(|vertex| {
            vertex.position[0] >= -1.0
                && vertex.position[0] <= 1.0
                && vertex.position[1] >= -1.0
                && vertex.position[1] <= 1.0
        }));
        assert!(vertices.iter().all(|vertex| {
            vertex.tex_coords[0] >= 0.0
                && vertex.tex_coords[0] <= 1.0
                && vertex.tex_coords[1] >= 0.0
                && vertex.tex_coords[1] <= 1.0
        }));
    }

    #[test]
    fn atlas_rasterizes_cjk_glyph_from_fallback_font() {
        let fonts = test_font_book();
        let mut terminal = Terminal::new(1, 4);

        terminal.put_str("中");
        let mut atlas = test_atlas(&fonts);
        atlas.update(&fonts, terminal.screen());

        let glyph = atlas.glyphs.get(&'中').expect("CJK glyph in atlas");
        assert!(glyph.width > 0);
        assert!(glyph.height > 0);
    }

    #[test]
    fn empty_grid_builds_empty_draw_batch() {
        let fonts = test_font_book();
        let terminal = Terminal::new(2, 4);

        let mut atlas = test_atlas(&fonts);
        atlas.update(&fonts, terminal.screen());
        let vertices = atlas.vertices(terminal.screen(), 800.0, 600.0);

        assert!(atlas.glyphs.is_empty());
        assert_eq!((atlas.width, atlas.height), (1, 1));
        assert_eq!(atlas.pixels, vec![0]);
        assert!(vertices.is_empty());
    }

    #[test]
    fn cached_atlas_reuses_existing_glyphs() {
        let fonts = test_font_book();
        let mut terminal = Terminal::new(1, 4);
        let mut atlas = test_atlas(&fonts);

        terminal.put_str("aa");
        atlas.update(&fonts, terminal.screen());
        assert_eq!(atlas.glyphs.len(), 1);

        terminal.put_str("\x1b[1;1Ha ");
        atlas.update(&fonts, terminal.screen());
        assert_eq!(atlas.glyphs.len(), 1);

        terminal.put_str("\x1b[1;1Hab");
        atlas.update(&fonts, terminal.screen());
        assert_eq!(atlas.glyphs.len(), 2);
    }
}
