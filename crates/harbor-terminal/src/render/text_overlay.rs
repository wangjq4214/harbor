//! Retained projections with distinct cell-content and IME lifecycles.
use std::collections::{BTreeSet, HashSet};

use super::*;

#[derive(Clone, Copy, PartialEq)]
struct ProjectionKey {
    rows: usize,
    cols: usize,
    viewport: RenderViewport,
    metrics: [f32; 3],
    palette: Palette,
}

impl ProjectionKey {
    fn new(snap: &TerminalSnapshot, view: &OverlayView<'_>) -> Self {
        Self {
            rows: snap.rows,
            cols: snap.cols,
            viewport: *view.viewport,
            metrics: [
                view.metrics.cell_width,
                view.metrics.line_height,
                view.metrics.ascent,
            ],
            palette: *view.palette,
        }
    }
}

pub(super) struct OverlayView<'a> {
    pub atlas: &'a GlyphAtlas,
    pub metrics: &'a TextMetrics,
    pub palette: &'a Palette,
    pub viewport: &'a RenderViewport,
}

/// Paint dependencies include absent glyphs, so later admission can restore them.
/// Do not use the atlas's overall generation: unrelated stable additions need
/// neither geometry rebuilding nor uploads.
struct GlyphDependency {
    ch: char,
    placement: Option<[u32; 8]>,
}

impl GlyphDependency {
    fn placement(atlas: &GlyphAtlas, ch: char) -> Option<[u32; 8]> {
        atlas.glyph_by_char(ch).map(|g| {
            [
                g.uv.left.to_bits(),
                g.uv.top.to_bits(),
                g.uv.right.to_bits(),
                g.uv.bottom.to_bits(),
                g.width,
                g.height,
                g.bearing_x as u32,
                g.bearing_y as u32,
            ]
        })
    }

    fn new(atlas: &GlyphAtlas, ch: char) -> Self {
        Self {
            ch,
            placement: Self::placement(atlas, ch),
        }
    }

    fn changed(&self, atlas: &GlyphAtlas) -> bool {
        self.placement != Self::placement(atlas, self.ch)
    }
}

#[derive(Default)]
struct Batch {
    vertices: Vec<TexturedVertex>,
    buffer: Option<VertexBuffer<TexturedVertex>>,
}

impl Batch {
    /// Keep CPU and GPU storage when unchanged. Scratch is reused between builds;
    /// no concatenation moves or uploads unrelated rows.
    fn update(&mut self, gpu: TerminalGpuAccess<'_>, scratch: &mut Vec<TexturedVertex>) -> usize {
        if bytemuck::cast_slice::<_, u8>(&self.vertices) == bytemuck::cast_slice::<_, u8>(scratch) {
            return 0;
        }
        let bytes = std::mem::size_of_val(scratch.as_slice());
        if scratch.is_empty() {
            self.buffer = None;
        } else {
            self.buffer
                .get_or_insert_with(|| VertexBuffer::new(gpu.device()))
                .write(gpu, scratch);
        }
        std::mem::swap(&mut self.vertices, scratch);
        // Removed rows release their historical high-water capacity. The shared
        // scratch still amortizes the next touched row's construction.
        if self.vertices.is_empty() {
            self.vertices = Vec::new();
        }
        bytes
    }

    fn draw(
        &self,
        pass: &mut wgpu::RenderPass,
        pipeline: &wgpu::RenderPipeline,
        group: &wgpu::BindGroup,
    ) {
        if let Some(buffer) = &self.buffer {
            buffer.draw(pass, pipeline, Some(group));
        }
    }
}

#[derive(Default)]
struct OverlayRow {
    batch: Batch,
    glyphs: Vec<GlyphDependency>,
}

#[derive(Default)]
pub(super) struct CellOverlays {
    key: Option<ProjectionKey>,
    rows: Vec<OverlayRow>,
    // Empty rows need no dependency checks on an unchanged preparation.
    dependent_rows: BTreeSet<usize>,
    // Retain ownership, not naked addresses: synthetic mutations detach shared
    // rows, and old allocations cannot be recycled into false identity matches.
    cells: Option<crate::SnapshotCells>,
    suppressed: HashSet<usize>,
    scratch: Vec<TexturedVertex>,
}

impl CellOverlays {
    pub fn prepare(
        &mut self,
        gpu: TerminalGpuAccess<'_>,
        snap: &TerminalSnapshot,
        view: OverlayView<'_>,
        suppressed: &HashSet<usize>,
        dirty: &[DirtyRange],
        force_full: bool,
    ) {
        #[cfg(test)]
        super::super::observations::reset_cell_overlay();
        let key = ProjectionKey::new(snap, &view);
        let full = force_full || self.key != Some(key);
        if self.rows.len() != snap.rows || self.key.is_some_and(|old| old.cols != snap.cols) {
            self.rows = (0..snap.rows).map(|_| OverlayRow::default()).collect();
            self.dependent_rows.clear();
        }
        let mut touched = BTreeSet::new();
        if full {
            touched.extend(0..snap.rows);
        } else {
            touched.extend(
                dirty
                    .iter()
                    .filter(|r| r.start_col < r.end_col && r.row < snap.rows)
                    .filter(|r| {
                        self.cells.as_ref().is_none_or(|old| {
                            !std::ptr::eq(&old[r.row * snap.cols], snap.cell(r.row, 0))
                        })
                    })
                    .map(|r| r.row),
            );
            // Suppression is independent of source-row identity: atlas admission
            // can change paint on a row whose cell payload is still shared.
            touched.extend(
                self.suppressed
                    .symmetric_difference(suppressed)
                    .map(|&index| index / snap.cols)
                    .filter(|&row| row < snap.rows),
            );
            touched.extend(
                self.dependent_rows
                    .iter()
                    .copied()
                    .filter(|&row| self.rows[row].glyphs.iter().any(|g| g.changed(view.atlas))),
            );
        }
        for row in touched {
            #[cfg(test)]
            super::super::observations::record_cell_overlay(|work| work.built_rows.push(row));
            let retained = &mut self.rows[row];
            retained.glyphs.clear();
            self.scratch.clear();
            for col in 0..snap.cols {
                #[cfg(test)]
                super::super::observations::record_cell_overlay(|work| work.inspected_cells += 1);
                let cell = snap.cell(row, col);
                if cell.wide_continuation
                    || cell.attrs.contains(CellAttrs::CONCEAL)
                    || suppressed.contains(&(row * snap.cols + col))
                {
                    continue;
                }
                // Ordinary cells do not need geometry/color calculation.
                if !cell.isolated_mark && cell.suffix.is_empty() {
                    continue;
                }
                let (x, y) = view.viewport.cell_pos(row, col);
                let cell_view = OverlayCell {
                    x,
                    baseline: y + view.metrics.ascent.ceil(),
                    color: glyph_color_with_palette(view.palette, cell.fg, cell.bg, cell.attrs),
                    surface: view.viewport.surface_dimensions(),
                    width: cell.grid_width(),
                };
                let glyphs = cell
                    .isolated_mark
                    .then_some(('\u{25cc}', false))
                    .into_iter()
                    .chain(
                        suffix_paint_chars(&cell.suffix)
                            .chain(cell.isolated_mark.then_some(cell.ch))
                            .map(|ch| (ch, true)),
                    );
                for (ch, center) in glyphs {
                    retained.glyphs.push(GlyphDependency::new(view.atlas, ch));
                    append_overlay_glyph(
                        view.atlas,
                        view.metrics,
                        &mut self.scratch,
                        ch,
                        &cell_view,
                        center,
                    );
                }
            }
            if retained.glyphs.is_empty() {
                self.dependent_rows.remove(&row);
            } else {
                self.dependent_rows.insert(row);
            }
            let bytes = retained.batch.update(gpu, &mut self.scratch);
            #[cfg(test)]
            if bytes > 0 {
                super::super::observations::record_cell_overlay(|work| {
                    work.writes.push((row, bytes))
                });
            }
            #[cfg(not(test))]
            let _ = bytes;
        }
        self.key = Some(key);
        self.cells = Some(snap.cells.clone());
        if &self.suppressed != suppressed {
            self.suppressed.clone_from(suppressed);
        }
    }

    pub fn draw(
        &self,
        pass: &mut wgpu::RenderPass,
        pipeline: &wgpu::RenderPipeline,
        group: &wgpu::BindGroup,
    ) {
        for &row in &self.dependent_rows {
            self.rows[row].batch.draw(pass, pipeline, group);
        }
    }

    #[cfg(test)]
    pub fn count(&self) -> u32 {
        self.rows
            .iter()
            .map(|r| r.batch.vertices.len() as u32)
            .sum()
    }
}

#[derive(Default)]
pub(super) struct ImeOverlay {
    key: Option<ProjectionKey>,
    cursor: (usize, usize),
    text: String,
    glyphs: Vec<GlyphDependency>,
    batch: Batch,
    scratch: Vec<TexturedVertex>,
}

impl ImeOverlay {
    pub fn prepare(
        &mut self,
        gpu: TerminalGpuAccess<'_>,
        snap: &TerminalSnapshot,
        preedit: Option<&Preedit>,
        view: OverlayView<'_>,
    ) {
        #[cfg(test)]
        super::super::observations::reset_ime_overlay();
        let Some(preedit) = preedit.filter(|p| !p.is_empty()) else {
            if self.key.take().is_some() {
                self.text.clear();
                self.glyphs.clear();
                self.scratch.clear();
                self.batch.update(gpu, &mut self.scratch);
                #[cfg(test)]
                super::super::observations::record_ime_overlay(|work| work.clears += 1);
            }
            return;
        };
        let key = ProjectionKey::new(snap, &view);
        let cursor = (snap.cursor_x, snap.cursor_y);
        // cursor_range affects caret layout, not the painted glyph projection.
        if self.key == Some(key)
            && self.cursor == cursor
            && self.text == preedit.text
            && !self.glyphs.iter().any(|g| g.changed(view.atlas))
        {
            return;
        }
        #[cfg(test)]
        super::super::observations::record_ime_overlay(|work| work.builds += 1);
        self.scratch.clear();
        self.glyphs.clear();
        let (surf_w, surf_h) = view.viewport.surface_dimensions();
        let color = glyph_color_with_palette(
            view.palette,
            Color::Default,
            Color::Default,
            CellAttrs::default(),
        );
        for positioned in layout_preedit(preedit, cursor, snap.rows, snap.cols).glyphs {
            self.glyphs
                .push(GlyphDependency::new(view.atlas, positioned.ch));
            let Some(glyph) = view.atlas.glyph_by_char(positioned.ch) else {
                continue;
            };
            if glyph.width == 0 || glyph.height == 0 {
                continue;
            }
            let (cell_x, cell_y) = view.viewport.cell_pos(positioned.row, positioned.col);
            let baseline = cell_y + view.metrics.ascent.ceil();
            let left = cell_x + glyph.bearing_x as f32;
            let bottom = baseline - glyph.bearing_y as f32;
            self.scratch
                .extend_from_slice(&TexturedVertex::from_pixel_rect(
                    left,
                    bottom - glyph.height as f32,
                    left + glyph.width as f32,
                    bottom,
                    glyph.uv.left,
                    glyph.uv.top,
                    glyph.uv.right,
                    glyph.uv.bottom,
                    color,
                    surf_w,
                    surf_h,
                ));
        }
        let bytes = self.batch.update(gpu, &mut self.scratch);
        #[cfg(test)]
        if bytes > 0 {
            super::super::observations::record_ime_overlay(|work| work.writes.push(bytes));
        }
        #[cfg(not(test))]
        let _ = bytes;
        self.key = Some(key);
        self.cursor = cursor;
        self.text.clone_from(&preedit.text);
    }

    pub fn draw(
        &self,
        pass: &mut wgpu::RenderPass,
        pipeline: &wgpu::RenderPipeline,
        group: &wgpu::BindGroup,
    ) {
        self.batch.draw(pass, pipeline, group);
    }

    #[cfg(test)]
    pub fn count(&self) -> u32 {
        self.batch.vertices.len() as u32
    }

    #[cfg(test)]
    pub fn buffer(&self) -> &wgpu::Buffer {
        self.batch.buffer.as_ref().unwrap().buffer()
    }
}
