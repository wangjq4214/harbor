//! Whole-unit terminal projection, fitted geometry and lazy GPU composition.
use super::{
    color_atlas::{COLOR_ATLAS_SIDE, ColorAtlas},
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

#[cfg(test)]
mod allocation_tests;
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
        // Classification depends on the lead and selector/join/keycap presence.
        // Inspect the complete suffix without copying an arbitrarily long source.
        let mut source = [0u8; 16];
        let mut len = first.len();
        source[..len].copy_from_slice(first.as_bytes());
        for marker in ['\u{fe0e}', '\u{fe0f}', '\u{200d}', '\u{20e3}'] {
            if cell.suffix.contains(marker) {
                len += marker.encode_utf8(&mut source[len..]).len();
            }
        }
        classify_unit(
            std::str::from_utf8(&source[..len]).unwrap(),
            PresentationIntent::Auto,
        ) == UnitKind::Emoji
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Resolved {
    Tile(TileBounds, u32, u32),
    Scalar,
    Missing,
}

struct Candidate {
    col: usize,
    ch: char,
    width: u8,
    color: [f32; 4],
    request: Option<SequenceRequest>,
    resolved: Option<Resolved>,
}

impl Candidate {
    fn same_request(&self, col: usize, cell: &Cell, color: [f32; 4]) -> bool {
        self.col == col
            && self.ch == cell.ch
            && self.color == color
            && self.request.as_ref().map_or_else(
                || {
                    cell.ch.len_utf16() + cell.suffix.chars().map(char::len_utf16).sum::<usize>()
                        > SEQUENCE_MAX_UTF16
                },
                |r| {
                    let mut encoded = [0; 4];
                    r.source
                        .strip_prefix(cell.ch.encode_utf8(&mut encoded) as &str)
                        == Some(cell.suffix.as_str())
                        && r.style.bold == cell.attrs.contains(CellAttrs::BOLD)
                        && r.style.italic == cell.attrs.contains(CellAttrs::ITALIC)
                },
            )
    }

    fn tile(&self, atlas: &ColorAtlas) -> Option<Resolved> {
        self.request
            .as_ref()
            .and_then(|r| atlas.entries.get(r))
            .filter(|e| matches!(e.outcome, super::color_atlas::CachedOutcome::Complete(_)))
            .and_then(|e| e.tile.as_ref().zip(e.placement))
            .map(|(tile, p)| Resolved::Tile(tile.bounds, p.x, p.y))
    }
}

#[derive(Default)]
struct SequenceRow {
    cells: Vec<Candidate>,
    vertices: Vec<TexturedVertex>,
    missing: Vec<ColoredVertex>,
    suppressed: HashSet<usize>,
    quads: Option<VertexBuffer<TexturedVertex>>,
    cues: Option<VertexBuffer<ColoredVertex>>,
}

#[derive(Clone, PartialEq)]
struct ProjectionKey {
    rows: usize,
    cols: usize,
    viewport: RenderViewport,
    palette: Palette,
    scale: f32,
    generation: harbor_text::PresentationGeneration,
}
/// At most one viewport's flags/lookups and one row's construction scratch.
/// Cleared at use and reset on grid changes; keys never accumulate across frames.
#[derive(Default)]
struct SequenceScratch {
    touched: Vec<bool>,
    cells: Vec<Candidate>,
    vertices: Vec<TexturedVertex>,
    missing: Vec<ColoredVertex>,
    suppressed: HashSet<usize>,
    usability: HashMap<char, bool>,
}

#[derive(Default)]
pub(super) struct SequenceLayer {
    pub atlas: ColorAtlas,
    pub emoji_cells: HashSet<usize>,
    pub tile_cells: HashSet<usize>,
    rows: Vec<SequenceRow>,
    key: Option<ProjectionKey>,
    scratch: SequenceScratch,
    gpu: Option<ColorGpu>,
    missing: Option<MissingGpu>,
}

fn sequence_request(
    cell: &Cell,
    fonts: &FontBook,
    palette: &Palette,
    scale: f32,
) -> Option<SequenceRequest> {
    if cell.ch.len_utf16() + cell.suffix.chars().map(char::len_utf16).sum::<usize>()
        > SEQUENCE_MAX_UTF16
    {
        return None;
    }
    // Font size is physical pixels; convert to DIP without scaling it twice.
    let mut request = SequenceRequest::new(
        cell.raw_text(),
        FontSize::new(fonts.size() / scale).unwrap(),
        FontSize::new(96.0 * scale).unwrap(),
    );
    request.style = SequenceStyle {
        bold: cell.attrs.contains(CellAttrs::BOLD),
        italic: cell.attrs.contains(CellAttrs::ITALIC),
    };
    request.foreground = glyph_color_with_palette(palette, cell.fg, cell.bg, cell.attrs)
        .map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
    Some(request)
}

impl SequenceLayer {
    #[cfg(test)]
    pub fn atlas_generation_changed(&self, fonts: &FontBook) -> bool {
        self.key
            .as_ref()
            .is_none_or(|k| k.generation != fonts.presentation_generation())
    }
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    fn prepare(
        &mut self,
        gpu: TerminalGpuAccess<'_>,
        fonts: &FontBook,
        scalar: &GlyphAtlas,
        snap: &TerminalSnapshot,
        viewport: &RenderViewport,
        palette: &Palette,
        scale: f32,
    ) {
        self.prepare_with_dirty(
            gpu,
            fonts,
            scalar,
            snap,
            viewport,
            palette,
            scale,
            &[],
            true,
            true,
        );
    }

    /// Content damage is borrowed, never consumed. Returns extra scalar suppression damage.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_with_dirty(
        &mut self,
        gpu: TerminalGpuAccess<'_>,
        fonts: &FontBook,
        scalar: &GlyphAtlas,
        snap: &TerminalSnapshot,
        viewport: &RenderViewport,
        palette: &Palette,
        scale: f32,
        dirty: &[crate::model::DirtyRange],
        force_full: bool,
        scalar_changed: bool,
    ) -> Vec<crate::model::DirtyRange> {
        #[cfg(test)]
        {
            super::observations::reset_sequence();
        }
        let key = ProjectionKey {
            rows: snap.rows,
            cols: snap.cols,
            viewport: *viewport,
            palette: *palette,
            scale,
            generation: fonts.presentation_generation(),
        };
        let full = force_full || self.key.as_ref() != Some(&key);
        if !full && dirty.is_empty() && !scalar_changed {
            return Vec::new();
        }
        if self.rows.len() != snap.rows || self.key.as_ref().is_some_and(|k| k.cols != snap.cols) {
            self.rows = (0..snap.rows).map(|_| SequenceRow::default()).collect();
            self.emoji_cells = HashSet::new();
            self.tile_cells = HashSet::new();
            self.scratch = SequenceScratch::default();
        }
        let scratch = &mut self.scratch;
        let touched = &mut scratch.touched;
        touched.resize(snap.rows, false);
        touched.fill(full);
        for range in dirty {
            if range.start_col < range.end_col && range.row < snap.rows {
                touched[range.row] = true;
            }
        }
        for (row, changed) in touched.iter_mut().enumerate() {
            if !*changed {
                continue;
            }
            let retained = &mut self.rows[row];
            let cells = &mut scratch.cells;
            cells.clear();
            let mut same = !full;
            for col in 0..snap.cols {
                #[cfg(test)]
                {
                    super::observations::record_sequence(|work| work.classified_cells += 1);
                }
                let cell = snap.cell(row, col);
                if emoji_candidate(cell) {
                    let color = glyph_color_with_palette(palette, cell.fg, cell.bg, cell.attrs);
                    let old = retained.cells.get_mut(cells.len());
                    let reuse = !full
                        && old
                            .as_ref()
                            .is_some_and(|c| c.same_request(col, cell, color));
                    same &= reuse && old.as_ref().is_some_and(|c| c.width == cell.grid_width());
                    let (request, resolved) = if reuse {
                        let old = old.unwrap();
                        (old.request.take(), old.resolved)
                    } else {
                        (sequence_request(cell, fonts, palette, scale), None)
                    };
                    cells.push(Candidate {
                        col,
                        ch: cell.ch,
                        width: cell.grid_width(),
                        color,
                        request,
                        resolved,
                    });
                }
            }
            same &= cells.len() == retained.cells.len();
            *changed = full || !same;
            if !same {
                for cell in &retained.cells {
                    self.emoji_cells.remove(&(row * snap.cols + cell.col));
                }
                for cell in cells.iter() {
                    self.emoji_cells.insert(row * snap.cols + cell.col);
                }
            }
            std::mem::swap(&mut retained.cells, cells);
            cells.clear();
        }
        // Pending damage can be replayed before acknowledgement. Identical
        // candidates keep both resolved dependencies and geometry in that case.
        if !full && !scalar_changed && !touched.iter().any(|&row| row) {
            return Vec::new();
        }
        // Only retained candidates are visited here, not every snapshot cell. Keep
        // row-major admission deterministic, including promotion after overflow.
        let requests = self
            .rows
            .iter()
            .flat_map(|r| &r.cells)
            .filter_map(|c| c.request.as_ref());
        let changes = self.atlas.sync(requests, key.generation, |r| {
            (*fonts.present_sequence(r)).clone()
        });
        let mut scalar_damage = Vec::new();
        scratch.usability.clear();
        for (row, retained) in self.rows.iter_mut().enumerate() {
            if !touched[row]
                && !scalar_changed
                && !retained.cells.iter().any(|c| {
                    let tile = c.tile(&self.atlas);
                    if matches!(c.resolved, Some(Resolved::Tile(..))) {
                        c.resolved != tile
                    } else {
                        tile.is_some()
                    }
                })
            {
                continue;
            }
            if retained.cells.is_empty()
                && retained.vertices.is_empty()
                && retained.missing.is_empty()
            {
                continue;
            }
            let mut rebuild = touched[row];
            for candidate in &mut retained.cells {
                let cell = snap.cell(row, candidate.col);
                let resolved = if let Some(tile) = candidate.tile(&self.atlas) {
                    tile
                } else if *scratch
                    .usability
                    .entry(cell.ch)
                    .or_insert_with(|| usable_scalar(scalar, cell.ch))
                {
                    Resolved::Scalar
                } else {
                    Resolved::Missing
                };
                rebuild |= candidate.resolved != Some(resolved);
                candidate.resolved = Some(resolved);
            }
            if !rebuild {
                continue;
            }
            #[cfg(test)]
            {
                super::observations::record_sequence(|work| work.built_rows.push(row));
            }
            std::mem::swap(&mut scratch.suppressed, &mut retained.suppressed);
            retained.suppressed.clear();
            let old_suppression = &scratch.suppressed;
            for idx in old_suppression {
                self.tile_cells.remove(idx);
            }
            let vertices = &mut scratch.vertices;
            let missing = &mut scratch.missing;
            vertices.clear();
            missing.clear();
            for candidate in &retained.cells {
                let col = candidate.col;
                let idx = row * snap.cols + col;
                let cell = snap.cell(row, col);
                let (x, y) = viewport.cell_pos(row, col);
                let width = usize::from(cell.grid_width()).min(snap.cols - col) as f32
                    * viewport.cell_width;
                let rect = [x, y, width, viewport.line_height];
                if let Some(Resolved::Tile(bounds, px, py)) = candidate.resolved {
                    let side = COLOR_ATLAS_SIDE as f32;
                    let uv = [
                        px as f32 / side,
                        py as f32 / side,
                        (px + bounds.width) as f32 / side,
                        (py + bounds.height) as f32 / side,
                    ];
                    if let Some(quad) =
                        fitted_quad(bounds, uv, rect, viewport.surface_dimensions(), [1.0; 4])
                    {
                        vertices.extend(quad);
                        self.tile_cells.insert(idx);
                        retained.suppressed.insert(idx);
                        continue;
                    }
                }
                // A complete tile which cannot fit retains the previous fallback policy.
                if candidate.resolved == Some(Resolved::Missing)
                    || matches!(candidate.resolved, Some(Resolved::Tile(..)))
                        && !usable_scalar(scalar, cell.ch)
                {
                    self.tile_cells.insert(idx);
                    retained.suppressed.insert(idx);
                    append_missing_cue(
                        missing,
                        rect,
                        viewport.surface_dimensions(),
                        glyph_color_with_palette(palette, cell.fg, cell.bg, cell.attrs),
                    );
                }
            }
            // Placement-only changes never invalidate unrelated scalar rows.
            let suppression_changed = old_suppression
                .iter()
                .any(|idx| !self.tile_cells.contains(idx))
                || retained.cells.iter().any(|c| {
                    let idx = row * snap.cols + c.col;
                    self.tile_cells.contains(&idx) && !old_suppression.contains(&idx)
                });
            if suppression_changed {
                scalar_damage.push(crate::model::DirtyRange {
                    row,
                    start_col: 0,
                    end_col: snap.cols,
                });
            }
            let tile_bytes =
                update_row_buffer(gpu, &mut retained.quads, &mut retained.vertices, vertices);
            let cue_bytes =
                update_row_buffer(gpu, &mut retained.cues, &mut retained.missing, missing);
            #[cfg(test)]
            if tile_bytes + cue_bytes > 0 {
                super::observations::record_sequence(|work| {
                    work.writes.push((row, tile_bytes, cue_bytes))
                });
            }
            #[cfg(not(test))]
            let _ = (tile_bytes, cue_bytes);
        }
        if self.rows.iter().any(|r| !r.vertices.is_empty()) && self.gpu.is_none() {
            self.gpu = Some(ColorGpu::new(gpu));
            // The texture may have accumulated tiles before its first visible quad.
            if !self.atlas.pixels.is_empty() {
                self.gpu.as_ref().unwrap().upload_full(gpu, &self.atlas);
                #[cfg(test)]
                {
                    super::observations::record_sequence(|work| {
                        work.texture_bytes += self.atlas.pixels.len()
                    });
                }
            }
        } else if let Some(color) = &self.gpu {
            if changes.full && !self.atlas.pixels.is_empty() {
                color.upload_full(gpu, &self.atlas);
                #[cfg(test)]
                {
                    super::observations::record_sequence(|work| {
                        work.texture_bytes += self.atlas.pixels.len()
                    });
                }
            } else {
                for request in &changes.added {
                    color.upload_tile(gpu, &self.atlas, request);
                    #[cfg(test)]
                    {
                        super::observations::record_sequence(|work| {
                            work.texture_bytes += self.atlas.entries[request]
                                .tile
                                .as_ref()
                                .unwrap()
                                .rgba
                                .len();
                        });
                    }
                }
            }
        }
        if self.rows.iter().any(|r| !r.missing.is_empty()) && self.missing.is_none() {
            self.missing = Some(MissingGpu::new(gpu));
        }
        self.key = Some(key);
        scalar_damage
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass) {
        if let Some(color) = &self.gpu {
            for row in &self.rows {
                if let Some(quads) = &row.quads {
                    quads.draw(pass, &color.pipeline, Some(&color.bind_group));
                }
            }
        }
        if let Some(cue) = &self.missing {
            for row in &self.rows {
                if let Some(quads) = &row.cues {
                    quads.draw(pass, &cue.pipeline, None);
                }
            }
        }
    }
}

/// Row-local storage never moves another row's vertices. Empty rows release
/// capacity so historical maximum sequence density does not accumulate.
fn update_row_buffer<T: bytemuck::Pod + Default>(
    gpu: TerminalGpuAccess<'_>,
    buffer: &mut Option<VertexBuffer<T>>,
    old: &mut Vec<T>,
    new: &mut Vec<T>,
) -> usize {
    if bytemuck::cast_slice::<T, u8>(old) == bytemuck::cast_slice::<T, u8>(new) {
        new.clear();
        return 0;
    }
    let bytes = std::mem::size_of_val(new.as_slice());
    if new.is_empty() {
        *buffer = None;
    } else {
        buffer
            .get_or_insert_with(|| VertexBuffer::new(gpu.device()))
            .write(gpu, new);
    }
    std::mem::swap(old, new);
    new.clear();
    if old.is_empty() {
        *old = Vec::new();
    }
    bytes
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
}
struct MissingGpu {
    pipeline: wgpu::RenderPipeline,
}
impl MissingGpu {
    fn new(gpu: TerminalGpuAccess<'_>) -> Self {
        Self {
            pipeline: gpu::create_colored_quad_pipeline(
                gpu.device(),
                gpu.format(),
                "emoji missing cue",
            ),
        }
    }
}

#[cfg(test)]
mod tests;
