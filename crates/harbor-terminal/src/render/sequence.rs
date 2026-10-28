//! Whole-unit terminal projection, fitted geometry and lazy GPU composition.
use super::{
    color_atlas::{COLOR_ATLAS_SIDE, COLOR_CACHE_ENTRIES, ColorAtlas},
    gpu::{self, ColoredVertex, TerminalGpuAccess, TexturedVertex, VertexBuffer},
    text::glyph_color_with_palette,
};
use crate::model::{Cell, TerminalSnapshot};
use crate::{CellAttrs, RenderViewport};
use harbor_config::Palette;
use harbor_text::{
    FontBook, FontSize, GlyphAtlas, PresentationIntent, SEQUENCE_MAX_UTF16, SequenceRequest,
    SequenceStyle, TileBounds, UnitKind, classify_unit,
};
use std::collections::{HashMap, HashSet};

/// Classify source, never width. ASCII ordinary cells need no allocated String.
pub(super) fn emoji_candidate(cell: &Cell) -> bool {
    if cell.wide_continuation || cell.isolated_mark || cell.attrs.contains(CellAttrs::CONCEAL) {
        return false;
    }
    if cell.ch.is_ascii() && cell.suffix.is_empty() {
        return false;
    }
    let mut encoded = [0; 4];
    let first = cell.ch.encode_utf8(&mut encoded);
    if cell.suffix.is_empty() {
        classify_unit(first, PresentationIntent::Auto) == UnitKind::Emoji
    } else {
        // Classify without copying potentially over-limit retained sources.
        let mut source = String::from(first);
        source.extend(cell.suffix.chars().take(SEQUENCE_MAX_UTF16));
        classify_unit(&source, PresentationIntent::Auto) == UnitKind::Emoji
    }
}

/// Uniform downscale, retain native origin/bearings when they fit, translate ink
/// back inside the assigned cell, then final-clip and correct UVs. No advance.
pub(super) fn fitted_quad(
    bounds: TileBounds,
    uv: [f32; 4],
    cell: [f32; 4],
    surface: (f32, f32),
    color: [f32; 4],
) -> Option<[TexturedVertex; 6]> {
    let [cx, cy, cw, ch] = cell;
    if bounds.width == 0 || bounds.height == 0 || cw <= 0.0 || ch <= 0.0 {
        return None;
    }
    let scale = (cw / bounds.width as f32)
        .min(ch / bounds.height as f32)
        .min(1.0);
    let (w, h) = (bounds.width as f32 * scale, bounds.height as f32 * scale);
    let left = (cx + bounds.left as f32 * scale).clamp(cx, (cx + cw - w).max(cx));
    let top = (cy + bounds.top as f32 * scale).clamp(cy, (cy + ch - h).max(cy));
    let right = left + w;
    let bottom = top + h;
    let [l, t, r, b] = [
        left.max(cx),
        top.max(cy),
        right.min(cx + cw),
        bottom.min(cy + ch),
    ];
    if l >= r || t >= b {
        return None;
    }
    let u = |x: f32| uv[0] + (uv[2] - uv[0]) * (x - left) / w;
    let v = |y: f32| uv[1] + (uv[3] - uv[1]) * (y - top) / h;
    Some(TexturedVertex::from_pixel_rect(
        l,
        t,
        r,
        b,
        u(l),
        v(t),
        u(r),
        v(b),
        color,
        surface.0,
        surface.1,
    ))
}

fn usable_scalar(atlas: &GlyphAtlas, ch: char) -> bool {
    atlas.glyph_by_char(ch).is_some_and(|g| {
        g.width > 0
            && g.height > 0
            && (0..g.height).any(|row| {
                let start =
                    ((g.atlas_y + row) * harbor_text::atlas::MAX_ATLAS_SIZE + g.atlas_x) as usize;
                atlas.pixels()[start..start + g.width as usize]
                    .iter()
                    .any(|&a| a != 0)
            })
    })
}

#[derive(Default)]
pub(super) struct SequenceLayer {
    pub atlas: ColorAtlas,
    pub emoji_cells: HashSet<usize>,
    pub tile_cells: HashSet<usize>,
    gpu: Option<ColorGpu>,
    missing: Option<MissingGpu>,
}

impl SequenceLayer {
    pub fn atlas_generation_changed(&self, fonts: &FontBook) -> bool {
        self.atlas
            .generation_changed(fonts.presentation_generation())
    }
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &mut self,
        gpu: TerminalGpuAccess<'_>,
        fonts: &FontBook,
        scalar: &GlyphAtlas,
        snap: &TerminalSnapshot,
        viewport: &RenderViewport,
        palette: &Palette,
        scale: f32,
    ) {
        self.emoji_cells.clear();
        self.tile_cells.clear();
        let mut requests = Vec::new();
        let mut ids = HashMap::new();
        let mut cells = Vec::new();
        for (idx, cell) in snap.cells.iter().enumerate() {
            if !emoji_candidate(cell) {
                continue;
            }
            self.emoji_cells.insert(idx);
            let id = if cell.suffix.chars().map(char::len_utf16).sum::<usize>()
                + cell.ch.len_utf16()
                <= SEQUENCE_MAX_UTF16
            {
                // Terminal font size is physical pixels, as on main. Convert to
                // DIP for the native request so size * DPI / 96 stays unchanged,
                // while DPI still participates in cache identity/invalidation.
                let mut request = SequenceRequest::new(
                    cell.raw_text(),
                    FontSize::new(fonts.size() / scale).unwrap(),
                    FontSize::new(96.0 * scale).unwrap(),
                );
                request.style = SequenceStyle {
                    bold: cell.attrs.contains(CellAttrs::BOLD),
                    italic: cell.attrs.contains(CellAttrs::ITALIC),
                };
                request.foreground =
                    glyph_color_with_palette(palette, cell.fg, cell.bg, cell.attrs)
                        .map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
                if let Some(&id) = ids.get(&request) {
                    Some(id)
                } else if requests.len() < COLOR_CACHE_ENTRIES {
                    let id = requests.len();
                    ids.insert(request.clone(), id);
                    requests.push(request);
                    Some(id)
                } else {
                    None
                }
            } else {
                None
            };
            cells.push((idx, id));
        }
        let changes = self
            .atlas
            .sync(&requests, fonts.presentation_generation(), |r| {
                (*fonts.present_sequence(r)).clone()
            });
        let mut vertices = Vec::new();
        let mut missing = Vec::new();
        for (idx, id) in cells {
            let cell = &snap.cells[idx];
            let (x, y) = viewport.cell_pos(idx / snap.cols, idx % snap.cols);
            // A wide unit at the final column cannot paint a non-existent neighbor.
            let width = usize::from(cell.grid_width()).min(snap.cols - idx % snap.cols) as f32
                * viewport.cell_width;
            let rect = [x, y, width, viewport.line_height];
            let placed = id
                .and_then(|id| self.atlas.entries.get(&requests[id]))
                .filter(|e| matches!(e.outcome, super::color_atlas::CachedOutcome::Complete(_)))
                .and_then(|e| e.tile.as_ref().zip(e.placement));
            if let Some((tile, p)) = placed {
                let side = COLOR_ATLAS_SIDE as f32;
                let uv = [
                    p.x as f32 / side,
                    p.y as f32 / side,
                    (p.x + tile.bounds.width) as f32 / side,
                    (p.y + tile.bounds.height) as f32 / side,
                ];
                if let Some(quad) = fitted_quad(
                    tile.bounds,
                    uv,
                    rect,
                    viewport.surface_dimensions(),
                    [1.0; 4],
                ) {
                    vertices.extend(quad);
                    self.tile_cells.insert(idx);
                    continue;
                }
            }
            // Unsupported/overflow retains a usable leading pictograph on R8.
            // Otherwise draw a font-independent hollow missing cue, not invisible text.
            if !usable_scalar(scalar, cell.ch) {
                self.tile_cells.insert(idx);
                append_missing_cue(
                    &mut missing,
                    rect,
                    viewport.surface_dimensions(),
                    glyph_color_with_palette(palette, cell.fg, cell.bg, cell.attrs),
                );
            }
        }
        if !vertices.is_empty() && self.gpu.is_none() {
            self.gpu = Some(ColorGpu::new(gpu));
        }
        if let Some(color) = &mut self.gpu {
            if changes.full && !self.atlas.pixels.is_empty() {
                color.upload_full(gpu, &self.atlas);
            } else {
                for request in &changes.added {
                    color.upload_tile(gpu, &self.atlas, request);
                }
            }
            color.upload_vertices(gpu, &vertices);
        }
        if !missing.is_empty() && self.missing.is_none() {
            self.missing = Some(MissingGpu::new(gpu));
        }
        if let Some(cue) = &mut self.missing {
            cue.upload(gpu, &missing);
        }
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass) {
        if let Some(color) = &self.gpu {
            color.draw(pass);
        }
        if let Some(cue) = &self.missing {
            cue.draw(pass);
        }
    }
}

fn append_missing_cue(
    vertices: &mut Vec<ColoredVertex>,
    rect: [f32; 4],
    surface: (f32, f32),
    color: [f32; 4],
) {
    let [x, y, w, h] = rect;
    let inset = (w.min(h) * 0.15).min(2.0);
    let (l, t, r, b) = (x + inset, y + inset, x + w - inset, y + h - inset);
    let stroke = 1.0_f32.min((r - l) * 0.5).min((b - t) * 0.5);
    for [l, t, r, b] in [
        [l, t, r, t + stroke],
        [l, b - stroke, r, b],
        [l, t + stroke, l + stroke, b - stroke],
        [r - stroke, t + stroke, r, b - stroke],
    ] {
        vertices.extend(ColoredVertex::from_pixel_rect(
            l, t, r, b, color, surface.0, surface.1,
        ));
    }
}

const COLOR_SHADER: &str = r#"
struct In { @location(0) position: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32> }
struct Out { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@group(0) @binding(0) var atlas: texture_2d<f32>;
@group(0) @binding(1) var atlas_sampler: sampler;
@vertex fn vs_main(i: In) -> Out { var o: Out; o.position = vec4<f32>(i.position, 0.0, 1.0); o.uv = i.uv; return o; }
@fragment fn fs_main(i: Out) -> @location(0) vec4<f32> { return textureSample(atlas, atlas_sampler, i.uv); }
"#;

struct ColorGpu {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    quads: VertexBuffer<TexturedVertex>,
}
impl ColorGpu {
    fn new(gpu: TerminalGpuAccess<'_>) -> Self {
        let device = gpu.device();
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("terminal color atlas"),
            size: wgpu::Extent3d {
                width: COLOR_ATLAS_SIDE,
                height: COLOR_ATLAS_SIDE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let layout = gpu::create_texture_bind_group_layout(device);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terminal color atlas"),
            layout: &layout,
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
        let pipeline = super::text::textured_pipeline(
            device,
            gpu.format(),
            &layout,
            COLOR_SHADER,
            wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING,
        );
        Self {
            texture,
            bind_group,
            pipeline,
            quads: VertexBuffer::new(device),
        }
    }
    fn upload_full(&self, gpu: TerminalGpuAccess<'_>, atlas: &ColorAtlas) {
        gpu.queue().write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &atlas.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(COLOR_ATLAS_SIDE * 4),
                rows_per_image: Some(COLOR_ATLAS_SIDE),
            },
            wgpu::Extent3d {
                width: COLOR_ATLAS_SIDE,
                height: COLOR_ATLAS_SIDE,
                depth_or_array_layers: 1,
            },
        );
    }
    fn upload_tile(
        &self,
        gpu: TerminalGpuAccess<'_>,
        atlas: &ColorAtlas,
        request: &SequenceRequest,
    ) {
        let entry = &atlas.entries[request];
        let (Some(tile), Some(p)) = (&entry.tile, entry.placement) else {
            return;
        };
        gpu.queue().write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: p.x,
                    y: p.y,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &tile.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(tile.bounds.width * 4),
                rows_per_image: Some(tile.bounds.height),
            },
            wgpu::Extent3d {
                width: tile.bounds.width,
                height: tile.bounds.height,
                depth_or_array_layers: 1,
            },
        );
    }
    fn upload_vertices(&mut self, gpu: TerminalGpuAccess<'_>, vertices: &[TexturedVertex]) {
        self.quads.write(gpu, vertices);
    }
    fn draw(&self, pass: &mut wgpu::RenderPass) {
        self.quads
            .draw(pass, &self.pipeline, Some(&self.bind_group));
    }
}
struct MissingGpu {
    pipeline: wgpu::RenderPipeline,
    quads: VertexBuffer<ColoredVertex>,
}
impl MissingGpu {
    fn new(gpu: TerminalGpuAccess<'_>) -> Self {
        Self {
            pipeline: gpu::create_colored_quad_pipeline(
                gpu.device(),
                gpu.format(),
                "emoji missing cue",
            ),
            quads: VertexBuffer::new(gpu.device()),
        }
    }
    fn upload(&mut self, gpu: TerminalGpuAccess<'_>, vertices: &[ColoredVertex]) {
        self.quads.write(gpu, vertices);
    }
    fn draw(&self, pass: &mut wgpu::RenderPass) {
        self.quads.draw(pass, &self.pipeline, None);
    }
}

#[cfg(test)]
mod tests;
