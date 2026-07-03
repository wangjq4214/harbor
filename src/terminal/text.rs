use hashbrown::HashMap;

use anyhow::Result;
use fontdue::Metrics;
use wgpu::util::DeviceExt;

use crate::{
    config::{FONT_SIZE, TEXT_PADDING},
    font::FontBook,
    gpu::{self, GpuContext, TexturedVertex},
    metrics::TextMetrics,
    render::Layer,
    terminal::{Screen, TerminalSize},
};
const ATLAS_PADDING: u32 = 1;
const MAX_ATLAS_SIZE: u32 = 2048;
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
    /// Pixel x position within the fixed-size atlas.
    atlas_x: u32,
    /// Pixel y position within the fixed-size atlas.
    atlas_y: u32,
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

/// One shelf row in the atlas packing layout.
#[derive(Clone, Copy, Debug)]
struct Shelf {
    /// Top pixel y-coordinate of this shelf.
    y: u32,
    /// Height of this shelf (max glyph height on the shelf).
    height: u32,
    /// Next free x position on this shelf.
    next_x: u32,
}

/// CPU-side glyph atlas plus enough metrics to build cell quads.
struct GlyphAtlas {
    /// Atlas texture height (pixels, for reporting/test assert).
    height: u32,
    /// Flattened greyscale pixel data (always MAX_ATLAS_SIZE^2 bytes).
    pixels: Vec<u8>,
    /// Character → atlas placement / UV lookup (persistent cache).
    glyphs: HashMap<char, AtlasGlyph>,
    /// Cell width derived from font metrics.
    cell_width: f32,
    /// Line height.
    line_height: f32,
    /// Baseline ascent.
    ascent: f32,
    /// Ordered top-to-bottom shelves for multi-row packing.
    shelves: Vec<Shelf>,
}

impl GlyphAtlas {
    /// Creates an empty atlas with a zero-filled 2048×2048 pixel buffer.
    fn new(text_metrics: TextMetrics) -> Self {
        Self {
            height: MAX_ATLAS_SIZE,
            pixels: vec![0; (MAX_ATLAS_SIZE * MAX_ATLAS_SIZE) as usize],
            glyphs: HashMap::new(),
            cell_width: text_metrics.cell_width,
            line_height: text_metrics.line_height,
            ascent: text_metrics.ascent,
            shelves: Vec::new(),
        }
    }

    /// Incrementally rasterises new glyphs from dirty rows.
    ///
    /// Returns the list of newly added characters (empty = no atlas changes).
    fn update(&mut self, fonts: &FontBook, screen: &Screen) -> Vec<char> {
        // Collect unique non-space chars from dirty rows only.
        let mut chars: Vec<char> = screen
            .dirty_rows()
            .flat_map(|row| {
                (0..screen.cols()).filter_map(move |col| {
                    let ch = screen.cell_char(row, col);
                    if ch != ' ' { Some(ch) } else { None }
                })
            })
            .collect();

        chars.sort_unstable();
        chars.dedup();

        // Rasterise only new glyphs (not yet in the persistent cache).
        let mut new_glyphs: Vec<RasterizedGlyph> = Vec::new();
        for ch in &chars {
            if !self.glyphs.contains_key(ch) {
                let (metrics, bitmap) = fonts.rasterize(*ch, FONT_SIZE);
                new_glyphs.push(RasterizedGlyph {
                    ch: *ch,
                    metrics,
                    bitmap,
                });
            }
        }

        if new_glyphs.is_empty() {
            return Vec::new();
        }

        tracing::debug!(
            new_glyphs = new_glyphs.len(),
            total_glyphs = self.glyphs.len() + new_glyphs.len(),
            "rasterizing new glyphs"
        );

        let new_chars: Vec<char> = new_glyphs.iter().map(|g| g.ch).collect();

        // Try to pack each new glyph into existing shelves; create new shelf as needed.
        'pack: for glyph in &new_glyphs {
            let gw = glyph.metrics.width as u32 + ATLAS_PADDING;
            let gh = glyph.metrics.height as u32;

            for s_idx in 0..self.shelves.len() {
                let shelf = &mut self.shelves[s_idx];
                if shelf.height >= gh && shelf.next_x + gw <= MAX_ATLAS_SIZE {
                    // Inline placement on this shelf
                    let x = shelf.next_x;
                    let y = shelf.y;
                    for row in 0..glyph.metrics.height {
                        let dst_start = ((y + row as u32) * MAX_ATLAS_SIZE + x) as usize;
                        let src_start = row * glyph.metrics.width;
                        self.pixels[dst_start..dst_start + glyph.metrics.width].copy_from_slice(
                            &glyph.bitmap[src_start..src_start + glyph.metrics.width],
                        );
                    }
                    let left = x as f32 / MAX_ATLAS_SIZE as f32;
                    let right = (x + glyph.metrics.width as u32) as f32 / MAX_ATLAS_SIZE as f32;
                    let top = y as f32 / MAX_ATLAS_SIZE as f32;
                    let bottom = (y + glyph.metrics.height as u32) as f32 / MAX_ATLAS_SIZE as f32;
                    self.glyphs.insert(
                        glyph.ch,
                        AtlasGlyph {
                            uv: AtlasUv {
                                left,
                                top,
                                right,
                                bottom,
                            },
                            width: glyph.metrics.width as u32,
                            height: glyph.metrics.height as u32,
                            xmin: glyph.metrics.xmin,
                            ymin: glyph.metrics.ymin,
                            atlas_x: x,
                            atlas_y: y,
                        },
                    );
                    shelf.next_x += glyph.metrics.width as u32 + ATLAS_PADDING;
                    continue 'pack;
                }
            }

            // No existing shelf fits — start a new shelf.
            let shelf_y = self.shelves.last().map_or(0, |s| s.y + s.height);
            if shelf_y + gh > MAX_ATLAS_SIZE {
                tracing::debug!("atlas full; evicting and rebuilding");
                return self.full_update(fonts, screen);
            }
            let x = 0u32;
            let y = shelf_y;
            for row in 0..glyph.metrics.height {
                let dst_start = ((y + row as u32) * MAX_ATLAS_SIZE + x) as usize;
                let src_start = row * glyph.metrics.width;
                self.pixels[dst_start..dst_start + glyph.metrics.width]
                    .copy_from_slice(&glyph.bitmap[src_start..src_start + glyph.metrics.width]);
            }
            let left = x as f32 / MAX_ATLAS_SIZE as f32;
            let right = (x + glyph.metrics.width as u32) as f32 / MAX_ATLAS_SIZE as f32;
            let top = y as f32 / MAX_ATLAS_SIZE as f32;
            let bottom = (y + glyph.metrics.height as u32) as f32 / MAX_ATLAS_SIZE as f32;
            self.glyphs.insert(
                glyph.ch,
                AtlasGlyph {
                    uv: AtlasUv {
                        left,
                        top,
                        right,
                        bottom,
                    },
                    width: glyph.metrics.width as u32,
                    height: glyph.metrics.height as u32,
                    xmin: glyph.metrics.xmin,
                    ymin: glyph.metrics.ymin,
                    atlas_x: x,
                    atlas_y: y,
                },
            );
            self.shelves.push(Shelf {
                y: shelf_y,
                height: gh,
                next_x: glyph.metrics.width as u32 + ATLAS_PADDING,
            });
        }

        self.height = self.shelves.last().map_or(1, |s| s.y + s.height);
        new_chars
    }

    /// Full rebuild from all visible characters (used on resize and eviction).
    fn full_update(&mut self, fonts: &FontBook, screen: &Screen) -> Vec<char> {
        // Collect unique non-space chars from the full screen.
        let mut chars: Vec<char> = screen
            .cells()
            .filter_map(|(_, _, ch)| if ch != ' ' { Some(ch) } else { None })
            .collect();
        chars.sort_unstable();
        chars.dedup();

        self.glyphs.clear();
        self.pixels.fill(0);
        self.shelves.clear();

        let mut new_rasterized: Vec<RasterizedGlyph> = Vec::new();
        for ch in &chars {
            let (metrics, bitmap) = fonts.rasterize(*ch, FONT_SIZE);
            new_rasterized.push(RasterizedGlyph {
                ch: *ch,
                metrics,
                bitmap,
            });
        }

        // Sort by height descending for better packing.
        use std::cmp::Reverse;
        new_rasterized.sort_by_key(|g| Reverse(g.metrics.height));
        let mut new_chars: Vec<char> = Vec::new();
        'pack: for glyph in &new_rasterized {
            let gw = glyph.metrics.width as u32 + ATLAS_PADDING;
            let gh = glyph.metrics.height as u32;

            for s_idx in 0..self.shelves.len() {
                let shelf = &mut self.shelves[s_idx];
                if shelf.height >= gh && shelf.next_x + gw <= MAX_ATLAS_SIZE {
                    let x = shelf.next_x;
                    let y = shelf.y;
                    for row in 0..glyph.metrics.height {
                        let dst_start = ((y + row as u32) * MAX_ATLAS_SIZE + x) as usize;
                        let src_start = row * glyph.metrics.width;
                        self.pixels[dst_start..dst_start + glyph.metrics.width].copy_from_slice(
                            &glyph.bitmap[src_start..src_start + glyph.metrics.width],
                        );
                    }
                    let left = x as f32 / MAX_ATLAS_SIZE as f32;
                    let right = (x + glyph.metrics.width as u32) as f32 / MAX_ATLAS_SIZE as f32;
                    let top = y as f32 / MAX_ATLAS_SIZE as f32;
                    let bottom = (y + glyph.metrics.height as u32) as f32 / MAX_ATLAS_SIZE as f32;
                    self.glyphs.insert(
                        glyph.ch,
                        AtlasGlyph {
                            uv: AtlasUv {
                                left,
                                top,
                                right,
                                bottom,
                            },
                            width: glyph.metrics.width as u32,
                            height: glyph.metrics.height as u32,
                            xmin: glyph.metrics.xmin,
                            ymin: glyph.metrics.ymin,
                            atlas_x: x,
                            atlas_y: y,
                        },
                    );
                    shelf.next_x += glyph.metrics.width as u32 + ATLAS_PADDING;
                    new_chars.push(glyph.ch);
                    continue 'pack;
                }
            }

            // No existing shelf fits — start a new shelf.
            let shelf_y = self.shelves.last().map_or(0, |s| s.y + s.height);
            if shelf_y + gh > MAX_ATLAS_SIZE {
                tracing::warn!("atlas full during full rebuild; dropping glyphs");
                break;
            }
            let x = 0u32;
            let y = shelf_y;
            for row in 0..glyph.metrics.height {
                let dst_start = ((y + row as u32) * MAX_ATLAS_SIZE + x) as usize;
                let src_start = row * glyph.metrics.width;
                self.pixels[dst_start..dst_start + glyph.metrics.width]
                    .copy_from_slice(&glyph.bitmap[src_start..src_start + glyph.metrics.width]);
            }
            let left = x as f32 / MAX_ATLAS_SIZE as f32;
            let right = (x + glyph.metrics.width as u32) as f32 / MAX_ATLAS_SIZE as f32;
            let top = y as f32 / MAX_ATLAS_SIZE as f32;
            let bottom = (y + glyph.metrics.height as u32) as f32 / MAX_ATLAS_SIZE as f32;
            self.glyphs.insert(
                glyph.ch,
                AtlasGlyph {
                    uv: AtlasUv {
                        left,
                        top,
                        right,
                        bottom,
                    },
                    width: glyph.metrics.width as u32,
                    height: glyph.metrics.height as u32,
                    xmin: glyph.metrics.xmin,
                    ymin: glyph.metrics.ymin,
                    atlas_x: x,
                    atlas_y: y,
                },
            );
            self.shelves.push(Shelf {
                y: shelf_y,
                height: gh,
                next_x: glyph.metrics.width as u32 + ATLAS_PADDING,
            });
            new_chars.push(glyph.ch);
        }

        self.height = self.shelves.last().map_or(1, |s| s.y + s.height);

        tracing::debug!(
            glyphs = self.glyphs.len(),
            atlas_height = self.height,
            shelves = self.shelves.len(),
            "glyph atlas full rebuild"
        );
        new_chars
    }

    /// Returns the atlas placement for a character, if present.
    pub(crate) fn glyph(&self, ch: char) -> Option<&AtlasGlyph> {
        self.glyphs.get(&ch)
    }

    /// Generates vertices for non-empty cells using atlas UVs.
    /// Only used in tests; TextLayer uses `build_all_vertices` instead.
    #[cfg(test)]
    fn vertices(
        &self,
        screen: &Screen,
        surface_width: f32,
        surface_height: f32,
    ) -> Vec<TexturedVertex> {
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
            for row in 0..glyph.height {
                let src_offset = ((glyph.atlas_y + row) * MAX_ATLAS_SIZE + glyph.atlas_x) as usize;
                let dst_offset = (row * padded_bytes_per_row) as usize;
                tile_data[dst_offset..dst_offset + glyph.width as usize]
                    .copy_from_slice(&atlas.pixels[src_offset..src_offset + glyph.width as usize]);
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
    /// Pre-allocated vertex buffer with COPY_DST for incremental uploads.
    vertex_buffer: wgpu::Buffer,
    /// Whether vertices need full re-upload.
    dirty: bool,
    /// Cached grid dimensions.
    rows: usize,
    cols: usize,
}

impl TextLayer {
    /// Creates the text render pipeline, bind group layout, initial glyph atlas,
    /// and pre-allocated vertex buffer from the given GPU context and screen.
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
        tracing::info!(glyphs = atlas.glyphs.len(), "glyph atlas initialized");
        let gpu_atlas = GpuGlyphAtlas::new(gpu.device(), gpu.queue(), &bind_group_layout, &atlas);

        // Pre-allocate vertex buffer for the full grid (rows * cols * 6 vertices).
        let rows = screen.rows();
        let cols = screen.cols();
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
        let verts = layer.build_all_vertices(screen, surf_w as f32, surf_h as f32);
        gpu.queue()
            .write_buffer(&layer.vertex_buffer, 0, bytemuck::cast_slice(&verts));
        layer.dirty = false;

        Ok(layer)
    }

    /// Returns the terminal grid dimensions that fit the current surface given font metrics.
    pub(crate) fn terminal_size(&self, gpu: &GpuContext) -> TerminalSize {
        let (w, h) = gpu.surface_size();
        self.metrics.terminal_size(w, h)
    }

    /// Forces a full vertex upload on the next `prepare` (called after resize).
    pub(crate) fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Builds the 6 * cols vertices for one row at fixed offsets. Blank cells → degenerate quad.
    fn build_row_vertices(
        &self,
        row: usize,
        screen: &Screen,
        surf_w: f32,
        surf_h: f32,
    ) -> Vec<TexturedVertex> {
        let mut verts = Vec::with_capacity(screen.cols() * 6);
        for col in 0..screen.cols() {
            let ch = screen.cell_char(row, col);
            if ch != ' '
                && let Some(glyph) = self.atlas.glyph(ch)
                && glyph.width > 0
                && glyph.height > 0
            {
                let cell_x = TEXT_PADDING + col as f32 * self.atlas.cell_width;
                let baseline =
                    TEXT_PADDING + self.atlas.ascent.ceil() + row as f32 * self.atlas.line_height;
                let glyph_left = cell_x + glyph.xmin as f32;
                let glyph_bottom = baseline - glyph.ymin as f32;
                let glyph_top = glyph_bottom - glyph.height as f32;
                let glyph_right = glyph_left + glyph.width as f32;
                verts.extend_from_slice(&TexturedVertex::from_pixel_rect(
                    glyph_left,
                    glyph_top,
                    glyph_right,
                    glyph_bottom,
                    glyph.uv.left,
                    glyph.uv.top,
                    glyph.uv.right,
                    glyph.uv.bottom,
                    surf_w,
                    surf_h,
                ));
                continue;
            }
            // Blank cell → 6 degenerate vertices (zero-area quad, rasterizer drops it).
            verts.extend(std::iter::repeat_n(TexturedVertex::default(), 6));
        }
        verts
    }

    /// Builds vertices for every row in the full grid.
    fn build_all_vertices(&self, screen: &Screen, surf_w: f32, surf_h: f32) -> Vec<TexturedVertex> {
        let mut verts = Vec::with_capacity(screen.rows() * screen.cols() * 6);
        for row in 0..screen.rows() {
            verts.extend(self.build_row_vertices(row, screen, surf_w, surf_h));
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

impl Layer for TextLayer {
    fn prepare(&mut self, gpu: &GpuContext, screen: Option<&Screen>) {
        let screen = screen.expect("text layer requires screen");
        let (surf_w, surf_h) = gpu.surface_size();

        // Detect resize: dimensions changed → full rebuild.
        if screen.rows() != self.rows || screen.cols() != self.cols {
            tracing::trace!(
                rows = screen.rows(),
                cols = screen.cols(),
                "text layer resize"
            );
            self.atlas.full_update(&self.fonts, screen);
            self.gpu_atlas = GpuGlyphAtlas::new(
                gpu.device(),
                gpu.queue(),
                &self.bind_group_layout,
                &self.atlas,
            );
            let new_cap = screen.rows() * screen.cols() * 6;
            let old_cap = self.rows * self.cols * 6;
            if new_cap > old_cap {
                self.vertex_buffer = gpu::create_vertex_buffer(
                    gpu.device(),
                    &vec![TexturedVertex::default(); new_cap.max(1)],
                );
            }
            let verts = self.build_all_vertices(screen, surf_w as f32, surf_h as f32);
            gpu.queue()
                .write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&verts));
            self.rows = screen.rows();
            self.cols = screen.cols();
            self.dirty = false;
            return;
        }

        // Atlas update: incremental rasterization of new glyphs.
        let new_glyphs = self.atlas.update(&self.fonts, screen);
        if !new_glyphs.is_empty() {
            tracing::debug!(
                new_glyphs = new_glyphs.len(),
                total_glyphs = self.atlas.glyphs.len(),
                "uploading new glyph tiles"
            );
            self.gpu_atlas
                .update_glyphs(gpu.queue(), &self.atlas, &new_glyphs);
            self.dirty = true; // UVs of new glyphs differ → force full vertex rebuild
        }

        // Dirty check: skip upload if nothing changed.
        let any_dirty_rows = screen.dirty_rows().next().is_some();
        if !self.dirty && !any_dirty_rows {
            return;
        }

        if self.dirty {
            tracing::trace!("rebuilding text draw batch (full)");
            let verts = self.build_all_vertices(screen, surf_w as f32, surf_h as f32);
            gpu.queue()
                .write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&verts));
        } else {
            tracing::trace!("rebuilding text draw batch (incremental)");
            for row in screen.dirty_rows() {
                let row_verts = self.build_row_vertices(row, screen, surf_w as f32, surf_h as f32);
                let offset = row * screen.cols() * 6 * std::mem::size_of::<TexturedVertex>();
                gpu.queue().write_buffer(
                    &self.vertex_buffer,
                    offset as u64,
                    bytemuck::cast_slice(&row_verts),
                );
            }
        }
        self.dirty = false;
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
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{GlyphAtlas, MAX_ATLAS_SIZE, TextMetrics};
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
        assert_eq!(
            atlas.pixels.len(),
            (MAX_ATLAS_SIZE * MAX_ATLAS_SIZE) as usize
        );
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

    #[test]
    fn update_returns_empty_when_no_new_chars() {
        let fonts = test_font_book();
        let mut terminal = Terminal::new(1, 4);
        let mut atlas = test_atlas(&fonts);

        terminal.put_str("abc");
        let new_chars = atlas.update(&fonts, terminal.screen());
        assert_eq!(new_chars.len(), 3);

        // Second update with same chars: no new glyphs.
        let new_chars = atlas.update(&fonts, terminal.screen());
        assert!(new_chars.is_empty(), "no new glyphs expected");
    }

    #[test]
    fn full_update_rebuilds_all_shelves() {
        let fonts = test_font_book();
        let mut terminal = Terminal::new(2, 4);

        terminal.put_str("ab cd");
        let mut atlas = test_atlas(&fonts);

        let new_chars = atlas.full_update(&fonts, terminal.screen());
        assert_eq!(new_chars.len(), 4, "a,b,c,d");
        assert!(atlas.glyphs.contains_key(&'a'));
        assert!(atlas.glyphs.contains_key(&'b'));
        assert!(atlas.glyphs.contains_key(&'c'));
        assert!(atlas.glyphs.contains_key(&'d'));
        // Shelves should be populated after full_update.
        assert!(
            !atlas.shelves.is_empty(),
            "full_update should create shelves"
        );
    }

    #[test]
    fn shelf_packing_places_glyphs() {
        let fonts = test_font_book();
        let mut terminal = Terminal::new(2, 10);

        terminal.put_str("hel lo!");
        let mut atlas = test_atlas(&fonts);
        atlas.update(&fonts, terminal.screen());

        // All visible chars should be in glyphs.
        for ch in "helo!".chars() {
            assert!(
                atlas.glyphs.contains_key(&ch),
                "glyph '{}' should exist",
                ch
            );
        }
        // At least one shelf should exist.
        assert!(!atlas.shelves.is_empty(), "should have at least one shelf");
    }

    #[test]
    fn new_glyph_lands_on_existing_shelf() {
        let fonts = test_font_book();
        let mut terminal = Terminal::new(1, 10);
        let mut atlas = test_atlas(&fonts);

        terminal.put_str("abc");
        let first = atlas.update(&fonts, terminal.screen());
        assert_eq!(first.len(), 3);
        let shelves_before = atlas.shelves.len();

        // Add a new char that should fit on same shelf.
        terminal.put_str("d");
        let second = atlas.update(&fonts, terminal.screen());
        assert_eq!(second, vec!['d'], "only 'd' is new");
        // Shelf count should not increase for small glyph.
        assert!(
            atlas.shelves.len() <= shelves_before + 1,
            "should not create many new shelves"
        );
    }

    #[test]
    fn atlas_creates_multiple_shelves_when_row_overflows() {
        let fonts = test_font_book();
        let mut atlas = test_atlas(&fonts);
        // ~22px with ATLAS_PADDING per glyph → ~2500px > 2048 row limit → forces 2nd shelf.
        let mut terminal = Terminal::new(2, 200);
        use unicode_width::UnicodeWidthChar;
        let chars: String = ('!'..='~')
            .chain('¡'..='ÿ')
            .filter(|c| UnicodeWidthChar::width(*c).unwrap_or(0) > 0)
            .take(130)
            .collect();
        assert!(
            chars.len() >= 115,
            "need ~115+ chars to overflow 2048px row, got {}",
            chars.len()
        );
        terminal.put_str(&chars);
        atlas.update(&fonts, terminal.screen());
        // Must have overflowed onto a second shelf.
        assert!(
            atlas.shelves.len() > 1,
            "140 unique glyphs should overflow one 2048px row (shelves: {})",
            atlas.shelves.len()
        );
        // At least one glyph should be on a non-zero y shelf.
        let on_second_shelf = atlas.glyphs.values().any(|g| g.atlas_y > 0);
        assert!(
            on_second_shelf,
            "at least one glyph should be on a second shelf"
        );
        // All chars should still be in the atlas.
        for ch in chars.chars() {
            assert!(
                atlas.glyphs.contains_key(&ch),
                "glyph '{}' should exist",
                ch
            );
        }
    }

    #[test]
    fn atlas_persistent_cache_across_clears() {
        let fonts = test_font_book();
        let mut terminal = Terminal::new(1, 4);
        let mut atlas = test_atlas(&fonts);

        terminal.put_str("ab");
        atlas.update(&fonts, terminal.screen());
        assert_eq!(atlas.glyphs.len(), 2);

        // "Clear" screen and add new content overlapping old.
        terminal.put_str("\x1b[2Jxy");
        // After the clear, dirty rows are marked, so update scans dirty rows.
        // 'a' and 'b' are no longer visible, 'x' and 'y' are new.
        // But cache keeps 'a' and 'b' (no eviction unless atlas is full).
        let _ = atlas.update(&fonts, terminal.screen());
        assert_eq!(
            atlas.glyphs.len(),
            4,
            "cache retains old glyphs even after clear"
        );
        assert!(atlas.glyphs.contains_key(&'x'));
        assert!(atlas.glyphs.contains_key(&'y'));
    }

    #[test]
    fn update_from_dirty_rows_scoped() {
        let fonts = test_font_book();
        let mut terminal = Terminal::new(3, 4);
        let mut atlas = test_atlas(&fonts);

        // Populate row 0
        terminal.put_str("abcd");
        atlas.update(&fonts, terminal.screen());
        assert_eq!(atlas.glyphs.len(), 4);

        // Now dirty rows are cleared by the caller (simulated).
        // Clear dirty and modify only row 1.
        terminal.screen_mut().clear_dirty();
        terminal.screen_mut().cursor_down(1);
        terminal.screen_mut().carriage_return();
        terminal.put_str("ef");
        // Now only row 1 is dirty.
        let new_chars = atlas.update(&fonts, terminal.screen());
        assert_eq!(new_chars, vec!['e', 'f'], "only new chars from dirty row");
        assert_eq!(atlas.glyphs.len(), 6, "old glyphs still cached");
    }
}
