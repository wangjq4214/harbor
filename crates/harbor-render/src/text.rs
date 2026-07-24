use harbor_types::TerminalSnapshot;
use hashbrown::HashMap;

use anyhow::Result;
use fontdue::Metrics;
use wgpu::util::DeviceExt;

use crate::{
    Component, UploadMode,
    font::FontBook,
    gpu::{self, GpuContext, TexturedVertex},
};
use harbor_config::{FONT_SIZE, TEXT_PADDING};
use harbor_terminal::{CellAttrs, Color, DirtyRange, TerminalSize};

const ATLAS_PADDING: u32 = 1;
const MAX_ATLAS_SIZE: u32 = 2048;
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

/// Fixed measurements used to map window pixels to terminal cells.
#[derive(Clone, Copy)]
pub struct TextMetrics {
    pub cell_width: f32,
    pub line_height: f32,
    pub ascent: f32,
    /// Distance from cell top to underline top edge (px).
    pub underline_position: f32,
    pub underline_thickness: f32,
    /// Distance from cell top to strikethrough center (px).
    pub strikethrough_position: f32,
    pub strikethrough_thickness: f32,
}

impl TextMetrics {
    pub fn new(fonts: &FontBook) -> Self {
        let (cell_width, line_height, ascent) = fonts.terminal_metrics();
        let (underline_position, strikethrough_position) = fonts
            .primary_horizontal_line_metrics(harbor_config::FONT_SIZE)
            .map(|lm| {
                let descent = lm.descent.abs();
                (line_height - descent + 1.0, (line_height - descent) * 0.45)
            })
            .unwrap_or((line_height * 0.8, line_height * 0.45));

        Self {
            cell_width,
            line_height,
            ascent,
            underline_position,
            underline_thickness: 1.5,
            strikethrough_position,
            strikethrough_thickness: 1.5,
        }
    }

    pub fn terminal_size(self, width: u32, height: u32) -> TerminalSize {
        let text_width = (width as f32 - TEXT_PADDING * 2.0).max(self.cell_width);
        let text_height = (height as f32 - TEXT_PADDING * 2.0).max(self.line_height);

        TerminalSize {
            rows: (text_height / self.line_height).floor().max(1.0) as usize,
            cols: (text_width / self.cell_width).floor().max(1.0) as usize,
        }
    }
}

// ── CPU-side glyph atlas ──────────────────────────────────────────────────

/// Atlas placement and metrics for one rasterized glyph.
#[derive(Clone, Copy)]
pub struct AtlasGlyph {
    /// UV sub-region (unit texture coordinates).
    pub uv: AtlasUv,
    /// Glyph pixel width.
    pub width: u32,
    /// Glyph pixel height.
    pub height: u32,
    /// fontdue horizontal offset (sub-pixel).
    pub xmin: i32,
    /// fontdue vertical offset (sub-pixel).
    pub ymin: i32,
    /// Pixel x position within the fixed-size atlas.
    pub atlas_x: u32,
    /// Pixel y position within the fixed-size atlas.
    pub atlas_y: u32,
}

/// UV rectangle within the atlas texture.
#[derive(Clone, Copy)]
pub struct AtlasUv {
    /// Left UV boundary [0, 1].
    pub left: f32,
    /// Top UV boundary [0, 1].
    pub top: f32,
    /// Right UV boundary [0, 1].
    pub right: f32,
    /// Bottom UV boundary [0, 1].
    pub bottom: f32,
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
    /// Returns `(new_chars, evicted)` where `evicted` indicates the atlas was
    /// fully cleared and all UVs changed (atlas overflow → full rebuild).
    fn update(&mut self, fonts: &FontBook, snap: &TerminalSnapshot) -> (Vec<char>, bool) {
        self.update_with_dirty(fonts, snap, &snap.dirty_ranges)
    }

    fn update_with_dirty(
        &mut self,
        fonts: &FontBook,
        snap: &TerminalSnapshot,
        dirty_ranges: &[DirtyRange],
    ) -> (Vec<char>, bool) {
        // Collect unique non-space chars from dirty rows only.
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
            return (Vec::new(), false);
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
                return (self.full_update(fonts, snap), true);
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
        (new_chars, false)
    }

    /// Full rebuild from all visible characters (used on resize and eviction).
    fn full_update(&mut self, fonts: &FontBook, snap: &TerminalSnapshot) -> Vec<char> {
        // Collect unique non-space chars from the full snap.
        let mut chars: Vec<char> = snap
            .cells
            .iter()
            .filter_map(|cell| if cell.ch != ' ' { Some(cell.ch) } else { None })
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
    pub fn glyph(&self, ch: char) -> Option<&AtlasGlyph> {
        self.glyphs.get(&ch)
    }

    /// Ensures the given characters are rasterized and packed into the atlas.
    /// Returns the list of newly added characters (empty if all were already cached).
    /// Returns `None` if the atlas is full and a rebuild would be needed
    /// (dialog chars are skipped — not worth evicting the snap atlas).
    pub fn ensure_chars(&mut self, chars: &[char], fonts: &FontBook) -> Vec<char> {
        // Rasterize only new glyphs
        let mut new_glyphs: Vec<RasterizedGlyph> = Vec::new();
        for ch in chars {
            if !self.glyphs.contains_key(ch) && *ch != ' ' {
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
        let new_chars: Vec<char> = new_glyphs.iter().map(|g| g.ch).collect();
        // Pack into shelves
        for glyph in &new_glyphs {
            let gw = glyph.metrics.width as u32 + ATLAS_PADDING;
            let gh = glyph.metrics.height as u32;
            let mut placed = false;
            for shelf in &mut self.shelves {
                if shelf.height >= gh && shelf.next_x + gw <= MAX_ATLAS_SIZE {
                    let x = shelf.next_x;
                    let y = shelf.y;
                    for row in 0..glyph.metrics.height {
                        let dst = ((y + row as u32) * MAX_ATLAS_SIZE + x) as usize;
                        let src = row * glyph.metrics.width;
                        self.pixels[dst..dst + glyph.metrics.width]
                            .copy_from_slice(&glyph.bitmap[src..src + glyph.metrics.width]);
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
                    placed = true;
                    break;
                }
            }
            if !placed {
                let shelf_y = self.shelves.last().map_or(0, |s| s.y + s.height);
                if shelf_y + gh > MAX_ATLAS_SIZE {
                    tracing::warn!("glyph atlas full; skipping dialog chars");
                    break;
                }
                let x = 0u32;
                let y = shelf_y;
                for row in 0..glyph.metrics.height {
                    let dst = ((y + row as u32) * MAX_ATLAS_SIZE + x) as usize;
                    let src = row * glyph.metrics.width;
                    self.pixels[dst..dst + glyph.metrics.width]
                        .copy_from_slice(&glyph.bitmap[src..src + glyph.metrics.width]);
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
        }
        self.height = self.shelves.last().map_or(1, |s| s.y + s.height);
        new_chars
    }

    /// Generates vertices for non-empty cells using atlas UVs.
    /// Only used in tests; TextLayer uses `build_all_vertices` instead.
    #[cfg(test)]
    fn vertices(
        &self,
        snap: &TerminalSnapshot,
        surface_width: f32,
        surface_height: f32,
    ) -> Vec<TexturedVertex> {
        let mut vertices = Vec::new();

        for (idx, cell) in snap.cells.iter().enumerate() {
            if cell.ch == ' ' {
                continue;
            }
            let row = idx / snap.cols;
            let col = idx % snap.cols;
            let ch = cell.ch;
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
                [1.0; 4],
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

    /// Re-uploads the CPU atlas into the existing texture after a terminal resize.
    fn update_full(&self, queue: &wgpu::Queue, atlas: &GlyphAtlas) -> usize {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self._texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &atlas.pixels,
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
        atlas.pixels.len()
    }

    /// Uploads new glyph tiles into the pre-allocated 2048×2048 texture.
    fn update_glyphs(&self, queue: &wgpu::Queue, atlas: &GlyphAtlas, new_chars: &[char]) -> usize {
        let mut uploaded = 0usize;
        for ch in new_chars {
            let Some(glyph) = atlas.glyph(*ch) else {
                continue;
            };
            if glyph.width == 0 || glyph.height == 0 {
                continue;
            }
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
            uploaded = uploaded.saturating_add(tile_data.len());
        }
        uploaded
    }
}

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
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Creates the text render pipeline, bind group layout, initial glyph atlas,
    /// and pre-allocated vertex buffer from the given GPU context and snap.
    pub fn new(
        gpu: &GpuContext,
        fonts: FontBook,
        metrics: TextMetrics,
        snap: &TerminalSnapshot,
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
        let mut atlas = GlyphAtlas::new(metrics);
        let _ = atlas.update(&fonts, snap);
        tracing::info!(glyphs = atlas.glyphs.len(), "glyph atlas initialized");
        let gpu_atlas = GpuGlyphAtlas::new(gpu.device(), gpu.queue(), &bind_group_layout, &atlas);

        // Pre-allocate vertex buffer for the full grid without a CPU-side zeroed copy.
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
        // Build initial vertex data and upload via write_buffer.
        let verts = layer.build_all_vertices(snap, surf_w as f32, surf_h as f32);
        gpu.write_buffer(&layer.vertex_buffer, 0, bytemuck::cast_slice(&verts));
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
    pub fn ensure_glyphs(&mut self, text: &str, gpu: &GpuContext) {
        let mut chars: Vec<char> = text.chars().filter(|&c| c != ' ').collect();
        chars.sort_unstable();
        chars.dedup();
        let new_chars = self.atlas.ensure_chars(&chars, &self.fonts);
        if !new_chars.is_empty() {
            self.gpu_atlas
                .update_glyphs(gpu.queue(), &self.atlas, &new_chars);
        }
    }
    /// Builds the 6 * cols vertices for one row at fixed offsets. Blank cells → degenerate quad.
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
                && let Some(glyph) = self.atlas.glyph(cell.ch)
                && glyph.width > 0
                && glyph.height > 0
            {
                let cell_x = TEXT_PADDING + col as f32 * self.atlas.cell_width;
                let baseline = TEXT_PADDING
                    + self.atlas.ascent.ceil()
                    + range.row as f32 * self.atlas.line_height;
                let mut glyph_left = cell_x + glyph.xmin as f32;
                let glyph_bottom = baseline - glyph.ymin as f32;
                let glyph_top = glyph_bottom - glyph.height as f32;
                let mut glyph_right = glyph_left + glyph.width as f32;

                // Italic: shift glyph right for a subtle lean.
                if cell.attrs.contains(CellAttrs::ITALIC) {
                    let offset = self.atlas.cell_width * 0.15;
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
        snap: &TerminalSnapshot,
        dirty_ranges: &[DirtyRange],
    ) {
        let (surf_w, surf_h) = gpu.surface_size();
        let resized = snap.rows != self.rows || snap.cols != self.cols;
        let bytes_per_cell = 6 * std::mem::size_of::<TexturedVertex>();

        if resized {
            tracing::trace!(rows = snap.rows, cols = snap.cols, "text layer resize");
            self.atlas.full_update(&self.fonts, snap);
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

        let (new_glyphs, evicted) = self
            .atlas
            .update_with_dirty(&self.fonts, snap, dirty_ranges);
        if !new_glyphs.is_empty() {
            tracing::debug!(
                new_glyphs = new_glyphs.len(),
                total_glyphs = self.atlas.glyphs.len(),
                "uploading new glyph tiles"
            );
            self.gpu_atlas
                .update_glyphs(gpu.queue(), &self.atlas, &new_glyphs);
        }
        if evicted {
            self.dirty = true;
        }

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
}

impl Component for Text {
    fn prepare(&mut self, gpu: &GpuContext, snap: Option<&TerminalSnapshot>) {
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
    use super::{GlyphAtlas, MAX_ATLAS_SIZE, TextMetrics, glyph_color};
    use harbor_terminal::{CellAttrs, Color, Terminal};

    fn test_font_book() -> crate::font::FontBook {
        crate::font::load_system_fonts().expect("load test font")
    }

    fn test_atlas(fonts: &crate::font::FontBook) -> GlyphAtlas {
        GlyphAtlas::new(TextMetrics::new(fonts))
    }

    #[test]
    fn atlas_contains_each_visible_glyph_once() {
        let fonts = test_font_book();
        let mut terminal = Terminal::new(2, 5);

        terminal.put_str("aa b\r\nc a");
        let mut atlas = test_atlas(&fonts);
        let _ = atlas.update(&fonts, &terminal.snapshot());

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

        terminal.put_str("a b\r\n c ");
        let mut atlas = test_atlas(&fonts);
        let _ = atlas.update(&fonts, &terminal.snapshot());
        let vertices = atlas.vertices(&terminal.snapshot(), 800.0, 600.0);

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
        let _ = atlas.update(&fonts, &terminal.snapshot());

        let glyph = atlas.glyphs.get(&'中').expect("CJK glyph in atlas");
        assert!(glyph.width > 0);
        assert!(glyph.height > 0);
    }

    #[test]
    fn empty_grid_builds_empty_draw_batch() {
        let fonts = test_font_book();
        let terminal = Terminal::new(2, 4);

        let mut atlas = test_atlas(&fonts);
        let _ = atlas.update(&fonts, &terminal.snapshot());
        let vertices = atlas.vertices(&terminal.snapshot(), 800.0, 600.0);

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
        let _ = atlas.update(&fonts, &terminal.snapshot());
        assert_eq!(atlas.glyphs.len(), 1);

        terminal.put_str("\x1b[1;1Ha ");
        let _ = atlas.update(&fonts, &terminal.snapshot());
        assert_eq!(atlas.glyphs.len(), 1);

        terminal.put_str("\x1b[1;1Hab");
        let _ = atlas.update(&fonts, &terminal.snapshot());
        assert_eq!(atlas.glyphs.len(), 2);
    }

    #[test]
    fn update_returns_empty_when_no_new_chars() {
        let fonts = test_font_book();
        let mut terminal = Terminal::new(1, 4);
        let mut atlas = test_atlas(&fonts);

        terminal.put_str("abc");
        let (new_chars, _) = atlas.update(&fonts, &terminal.snapshot());
        assert_eq!(new_chars.len(), 3);

        // Second update with same chars: no new glyphs.
        let (new_chars, _) = atlas.update(&fonts, &terminal.snapshot());
        assert!(new_chars.is_empty(), "no new glyphs expected");
    }

    #[test]
    fn full_update_rebuilds_all_shelves() {
        let fonts = test_font_book();
        let mut terminal = Terminal::new(2, 4);

        terminal.put_str("ab cd");
        let mut atlas = test_atlas(&fonts);

        let new_chars = atlas.full_update(&fonts, &terminal.snapshot());
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
        let _ = atlas.update(&fonts, &terminal.snapshot());

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
        let (first, _) = atlas.update(&fonts, &terminal.snapshot());
        assert_eq!(first.len(), 3);
        let shelves_before = atlas.shelves.len();

        // Add a new char that should fit on same shelf.
        terminal.put_str("d");
        let (second, _) = atlas.update(&fonts, &terminal.snapshot());
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
        let _ = atlas.update(&fonts, &terminal.snapshot());
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
        let _ = atlas.update(&fonts, &terminal.snapshot());
        assert_eq!(atlas.glyphs.len(), 2);

        // "Clear" snap and add new content overlapping old.
        terminal.put_str("\x1b[2Jxy");
        // After the clear, dirty rows are marked, so update scans dirty rows.
        // 'a' and 'b' are no longer visible, 'x' and 'y' are new.
        // But cache keeps 'a' and 'b' (no eviction unless atlas is full).
        let _ = atlas.update(&fonts, &terminal.snapshot());
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
        let _ = atlas.update(&fonts, &terminal.snapshot());
        assert_eq!(atlas.glyphs.len(), 4);

        // Now dirty rows are cleared by the caller (simulated).
        // Clear dirty and modify only row 1.
        terminal.screen_mut().clear_dirty();
        terminal.screen_mut().cursor_down(1);
        terminal.screen_mut().carriage_return();
        terminal.put_str("ef");
        // Now only row 1 is dirty.
        let (new_chars, _) = atlas.update(&fonts, &terminal.snapshot());
        assert_eq!(new_chars, vec!['e', 'f'], "only new chars from dirty row");
        assert_eq!(atlas.glyphs.len(), 6, "old glyphs still cached");
    }

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
}
