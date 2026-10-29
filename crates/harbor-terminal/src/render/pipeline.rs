use super::box_drawing::BoxDrawing;
use crate::TerminalUpdate;
use crate::damage::DirtyRange;
use crate::model::UpdateDamage;
use crate::render::{
    Background, Cursor, Decoration, RenderViewport, Scrollbar, Selection, TerminalGpuAccess, Text,
};
use harbor_config::Palette;
use harbor_text::{FontBook, TextMetrics};
use std::sync::Arc;

/// Encapsulates the GPU rendering pipeline components for the terminal.
pub struct TerminalRenderPipeline {
    viewport: RenderViewport,
    pub background: Background,
    pub text: Text,
    pub(super) boxes: BoxDrawing,
    pub decoration: Decoration,
    highlights: super::highlight::Highlights,
    pub selection: Selection,
    pub cursor: Cursor,
    pub scrollbar: Scrollbar,
    palette: Palette,
    raster_scale: f32,
    grid: (usize, usize),
}

impl TerminalRenderPipeline {
    pub fn new(
        gpu: TerminalGpuAccess<'_>,
        initial_surface_size: (u32, u32),
        font_book: FontBook,
        metrics: TextMetrics,
        update: &TerminalUpdate,
    ) -> anyhow::Result<Self> {
        Self::new_at_scale(gpu, initial_surface_size, font_book, metrics, update, 1.0)
    }

    fn new_at_scale(
        gpu: TerminalGpuAccess<'_>,
        initial_surface_size: (u32, u32),
        font_book: FontBook,
        metrics: TextMetrics,
        update: &TerminalUpdate,
        raster_scale: f32,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(
            super::decoration::valid_metrics(&metrics),
            "invalid terminal text metrics"
        );
        anyhow::ensure!(
            raster_scale.is_finite() && raster_scale > 0.0 && (96.0 * raster_scale).is_finite(),
            "invalid terminal raster scale"
        );
        anyhow::ensure!(
            super::decoration::slot_count(&update.snapshot, gpu.device().limits().max_buffer_size)
                .is_some()
                && super::box_drawing::slot_count(
                    &update.snapshot,
                    gpu.device().limits().max_buffer_size
                )
                .is_some(),
            "terminal grid exceeds resource capacity"
        );
        let snap = &update.snapshot;
        let palette = update.appearance.palette();
        let tint = update.appearance.clear_rgba(update.backdrop_available);
        let viewport = RenderViewport::with_surface(
            metrics.cell_width,
            metrics.line_height,
            initial_surface_size,
            initial_surface_size,
        );
        anyhow::ensure!(
            valid_projection(&viewport, (snap.rows, snap.cols), &metrics),
            "invalid terminal viewport projection"
        );
        let colored_quad_pipeline = Arc::new(crate::render::gpu::create_colored_quad_pipeline(
            gpu.device(),
            gpu.format(),
            "terminal colored-quad pipeline",
        ));
        let background = Background::new(
            gpu,
            Arc::clone(&colored_quad_pipeline),
            initial_surface_size,
            snap,
            metrics.cell_width,
            metrics.line_height,
            tint,
            palette,
        );
        let text = Text::new_at_scale(
            gpu,
            font_book,
            metrics,
            snap,
            &viewport,
            palette,
            raster_scale,
        )?;
        let decoration = Decoration::new(
            gpu,
            Arc::clone(&colored_quad_pipeline),
            initial_surface_size,
            snap,
            metrics,
            palette,
        );
        let mut boxes = BoxDrawing::new(gpu, Arc::clone(&colored_quad_pipeline), palette);
        boxes.prepare_with_dirty(gpu, snap, &[], &viewport);
        let selection = Selection::new(gpu, Arc::clone(&colored_quad_pipeline), palette.selection);
        let cursor = Cursor::new(gpu, metrics, palette.cursor);
        let scrollbar = Scrollbar::new(gpu, snap, &viewport);

        Ok(Self {
            viewport,
            background,
            text,
            boxes,
            decoration,
            highlights: super::highlight::Highlights::new(gpu, colored_quad_pipeline),
            selection,
            cursor,
            scrollbar,
            palette,
            raster_scale,
            grid: (snap.rows, snap.cols),
        })
    }
    /// Installs a prepared replacement for single-renderer callers.
    pub fn replace_fonts(
        &mut self,
        gpu: TerminalGpuAccess<'_>,
        fonts: FontBook,
        update: &TerminalUpdate,
    ) -> anyhow::Result<()> {
        *self = self.prepare_fonts(gpu, fonts, update)?;
        Ok(())
    }
    /// Prepare a replacement projection before committing a font/fallback session.
    /// Detached preparation lets the application validate all sessions before publication.
    pub fn prepare_fonts(
        &self,
        gpu: TerminalGpuAccess<'_>,
        fonts: FontBook,
        update: &TerminalUpdate,
    ) -> anyhow::Result<Self> {
        let metrics = TextMetrics::from_font_metrics(fonts.font_metrics());
        // Candidate resources may fail GPU validation/allocation even after the
        // CPU preflight. Scope every preparation error and publish only afterwards.
        let allocation = gpu
            .device()
            .push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let internal = gpu.device().push_error_scope(wgpu::ErrorFilter::Internal);
        let validation = gpu.device().push_error_scope(wgpu::ErrorFilter::Validation);
        let candidate = (|| {
            let mut replacement = Self::new_at_scale(
                gpu,
                self.viewport.surface_size,
                fonts,
                metrics,
                update,
                self.raster_scale,
            )?;
            let mut viewport = self.viewport;
            viewport.cell_width = metrics.cell_width;
            viewport.line_height = metrics.line_height;
            anyhow::ensure!(
                replacement.sync_viewport(viewport, true),
                "invalid replacement viewport projection"
            );
            anyhow::ensure!(
                replacement.prepare(gpu, update, false),
                "invalid replacement grid projection"
            );
            Ok::<_, anyhow::Error>(replacement)
        })();
        let mut gpu_error = None;
        for scope in [validation, internal, allocation] {
            if let Some(error) = pollster::block_on(scope.pop()) {
                gpu_error = Some(error);
            }
        }
        if let Some(error) = gpu_error {
            anyhow::bail!("terminal font resource preparation failed: {error}");
        }
        candidate
    }

    /// Refresh DPI-dependent sequence resources without changing physical font
    /// metrics or rebuilding the ordinary-text atlas and GPU pipelines. A new
    /// presentation revision invalidates sequence successes and failures together.
    pub fn sync_raster_scale(
        &mut self,
        gpu: TerminalGpuAccess<'_>,
        scale: f32,
        update: &TerminalUpdate,
    ) -> anyhow::Result<bool> {
        anyhow::ensure!(
            scale.is_finite() && scale > 0.0 && (96.0 * scale).is_finite(),
            "invalid terminal raster scale"
        );
        if self.raster_scale == scale {
            return Ok(false);
        }
        anyhow::ensure!(
            self.can_prepare(gpu, update),
            "invalid terminal raster projection"
        );
        self.text.sync_raster_scale(scale);
        self.raster_scale = scale;
        let prepared = self.prepare(gpu, update, false);
        debug_assert!(prepared);
        Ok(true)
    }

    pub fn sync_palette(&mut self, palette: Palette) {
        if self.palette == palette {
            return;
        }
        self.palette = palette;
        self.background.set_palette(palette);
        self.text.set_palette(palette);
        self.boxes.set_palette(palette);
        self.decoration.set_palette(palette);
        self.cursor.set_color(palette.cursor);
        self.selection.set_color(palette.selection);
    }

    /// Returns false for a rejected target, leaving the valid projection intact.
    pub fn sync_viewport(&mut self, viewport: RenderViewport, grid_changed: bool) -> bool {
        // Malformed records must not replace the last valid projection. A zero
        // target is a skipped frame, not a reason to normalize vertices by zero.
        if !valid_projection(&viewport, self.grid, self.metrics()) {
            return false;
        }
        let viewport_changed = self.viewport != viewport;
        if viewport_changed || grid_changed {
            self.viewport = viewport;
            self.background.invalidate_projection();
            self.text.invalidate_projection();
            self.boxes.invalidate_projection();
            self.decoration.invalidate_projection();
            self.selection.invalidate_projection();
            self.cursor.invalidate_projection();
        }
        true
    }

    pub fn raster_scale(&self) -> f32 {
        self.raster_scale
    }

    /// Grid represented by the last prepared GPU projection, not the latest
    /// allocation viewport (which can change before a reader ACK is admitted).
    pub fn grid_size(&self) -> crate::TerminalSize {
        crate::TerminalSize {
            rows: self.grid.0,
            cols: self.grid.1,
        }
    }

    pub fn viewport(&self) -> RenderViewport {
        self.viewport
    }

    fn can_prepare(&self, gpu: TerminalGpuAccess<'_>, update: &TerminalUpdate) -> bool {
        let snap = &update.snapshot;
        super::decoration::slot_count(snap, gpu.device().limits().max_buffer_size).is_some()
            && super::box_drawing::slot_count(snap, gpu.device().limits().max_buffer_size).is_some()
            && valid_projection(&self.viewport, (snap.rows, snap.cols), self.metrics())
    }

    /// Spans must come from the active engine's validated current-geometry projection.
    pub fn prepare_highlights(
        &mut self,
        gpu: TerminalGpuAccess<'_>,
        spans: &[crate::read_model::CellSpan],
    ) {
        self.highlights.prepare(gpu, spans, self.viewport);
    }

    /// Prepare every retained GPU layer from one coherent, read-only engine update.
    /// A true return leaves the projection sufficient for a subsequent draw.
    /// On false, the caller must not draw/acknowledge this update.
    pub fn prepare(
        &mut self,
        gpu: TerminalGpuAccess<'_>,
        update: &TerminalUpdate,
        blink_visible: bool,
    ) -> bool {
        let snap = &update.snapshot;
        if !self.can_prepare(gpu, update) {
            return false;
        }
        self.grid = (snap.rows, snap.cols);
        self.sync_palette(update.appearance.palette());
        let viewport = self.viewport;
        let dirty_ranges = update_dirty_ranges(update);
        let tint = update.appearance.clear_rgba(update.backdrop_available);
        self.background
            .prepare_with_dirty(gpu, snap, &dirty_ranges, &viewport, tint);
        self.boxes
            .prepare_with_dirty(gpu, snap, &dirty_ranges, &viewport);
        self.text
            .prepare_with_dirty(gpu, snap, &dirty_ranges, &viewport, update.preedit.as_ref());
        self.decoration
            .prepare_with_dirty(gpu, snap, &dirty_ranges, &viewport);
        self.text
            .prepare_preedit(gpu, update.preedit.as_ref(), snap, &viewport);
        self.selection.set_bounds(update.selection);
        self.selection.prepare(gpu, Some(snap), &viewport);
        self.cursor.prepare(
            gpu,
            Some(snap),
            &viewport,
            update.preedit.is_some(),
            blink_visible,
        );
        self.scrollbar.prepare(gpu, Some(snap), &viewport);
        true
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass) {
        self.background.draw(pass);
        self.highlights.draw(pass);
        self.boxes.draw(pass);
        self.text.draw(pass);
        self.decoration.draw(pass);
        self.selection.draw(pass);
        self.cursor.draw(pass);
        self.scrollbar.draw(pass);
    }

    pub fn metrics(&self) -> &TextMetrics {
        self.text.metrics()
    }
}

/// Other retained layers still use f32 pixel arithmetic. Do not publish a
/// projection whose cell/baseline or NDC arithmetic would overflow those layers.
fn valid_projection(view: &RenderViewport, grid: (usize, usize), metrics: &TextMetrics) -> bool {
    if !view.is_drawable() {
        return false;
    }
    let (rows, cols) = grid;
    let x = f64::from(view.allocation_origin.0) + f64::from(view.padding);
    let y = f64::from(view.allocation_origin.1) + f64::from(view.padding);
    let right = x + cols as f64 * f64::from(view.cell_width);
    let bottom = y + rows as f64 * f64::from(view.line_height) + f64::from(metrics.ascent);
    let limit = f64::from(f32::MAX) / 4.0;
    [x, y, right, bottom].into_iter().all(|v| v.abs() < limit)
        && [x, right]
            .into_iter()
            .all(|v| v.abs() / f64::from(view.surface_size.0) < limit)
        && [y, bottom]
            .into_iter()
            .all(|v| v.abs() / f64::from(view.surface_size.1) < limit)
}

/// Translate explicit full damage into complete ranges for the existing layer upload policy.
fn update_dirty_ranges(update: &TerminalUpdate) -> std::borrow::Cow<'_, [DirtyRange]> {
    match &update.damage {
        UpdateDamage::Ranges(ranges) => std::borrow::Cow::Borrowed(ranges),
        UpdateDamage::FullUpload => std::borrow::Cow::Owned(
            (0..update.snapshot.rows)
                .map(|row| DirtyRange {
                    row,
                    start_col: 0,
                    end_col: update.snapshot.cols,
                })
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests;
