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
    pub decoration: Decoration,
    pub selection: Selection,
    pub cursor: Cursor,
    pub scrollbar: Scrollbar,
    palette: Palette,
}

impl TerminalRenderPipeline {
    pub fn new(
        gpu: TerminalGpuAccess<'_>,
        initial_surface_size: (u32, u32),
        font_book: FontBook,
        metrics: TextMetrics,
        update: &TerminalUpdate,
    ) -> anyhow::Result<Self> {
        let snap = &update.snapshot;
        let palette = update.appearance.palette();
        let tint = update.appearance.clear_rgba(update.backdrop_available);
        let initial_surface_size = (initial_surface_size.0.max(1), initial_surface_size.1.max(1));
        let viewport = RenderViewport::with_surface(
            metrics.cell_width,
            metrics.line_height,
            initial_surface_size,
            initial_surface_size,
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
        let text = Text::new(gpu, font_book, metrics, snap, &viewport, palette)?;
        let decoration = Decoration::new(
            gpu,
            Arc::clone(&colored_quad_pipeline),
            initial_surface_size,
            snap,
            metrics,
            palette,
        );
        let selection = Selection::new(gpu, colored_quad_pipeline, palette.selection);
        let cursor = Cursor::new(gpu, metrics, palette.cursor);
        let scrollbar = Scrollbar::new(gpu, snap, &viewport);

        Ok(Self {
            viewport,
            background,
            text,
            decoration,
            selection,
            cursor,
            scrollbar,
            palette,
        })
    }
    pub fn sync_palette(&mut self, palette: Palette) {
        if self.palette == palette {
            return;
        }
        self.palette = palette;
        self.background.set_palette(palette);
        self.text.set_palette(palette);
        self.decoration.set_palette(palette);
        self.cursor.set_color(palette.cursor);
        self.selection.set_color(palette.selection);
    }

    pub fn sync_viewport(&mut self, viewport: RenderViewport, grid_changed: bool) {
        let viewport_changed = self.viewport != viewport;
        if viewport_changed || grid_changed {
            self.viewport = viewport;
            self.background.invalidate_projection();
            self.text.invalidate_projection();
            self.decoration.invalidate_projection();
            self.selection.invalidate_projection();
            self.cursor.invalidate_projection();
        }
    }

    pub fn viewport(&self) -> RenderViewport {
        self.viewport
    }

    /// Prepare every retained GPU layer from one coherent, read-only engine update.
    /// A successful return leaves the projection sufficient for a subsequent draw.
    pub fn prepare(
        &mut self,
        gpu: TerminalGpuAccess<'_>,
        update: &TerminalUpdate,
        blink_visible: bool,
    ) {
        let snap = &update.snapshot;
        self.sync_palette(update.appearance.palette());
        let viewport = self.viewport;
        let dirty_ranges = update_dirty_ranges(update);
        let tint = update.appearance.clear_rgba(update.backdrop_available);
        self.background
            .prepare_with_dirty(gpu, snap, &dirty_ranges, &viewport, tint);
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
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass) {
        self.background.draw(pass);
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
mod tests {
    use super::*;
    use crate::Terminal;
    use crate::render::gpu::{ColoredVertex, TexturedVertex, UploadMode, UploadPolicy};
    use std::time::Instant;

    #[test]
    fn engine_damage_drives_full_and_incremental_uploads_for_all_grid_layers() {
        let mut terminal = Terminal::new_headless(10, 20);
        let now = Instant::now();
        let initial = terminal.read_update(now);
        let full = update_dirty_ranges(&initial);
        assert_eq!(full.len(), 10);
        assert!(
            full.iter().enumerate().all(|(row, range)| range.row == row
                && range.start_col == 0
                && range.end_col == 20)
        );
        assert!(terminal.acknowledge_update(&initial));

        terminal.put_str("x");
        let incremental = terminal.read_update(now);
        let ranges = update_dirty_ranges(&incremental);
        assert!(matches!(incremental.damage, UpdateDamage::Ranges(_)));
        assert!(!ranges.is_empty());
        let policy = UploadPolicy::default();
        for bytes_per_cell in [
            6 * std::mem::size_of::<ColoredVertex>(),
            6 * std::mem::size_of::<TexturedVertex>(),
        ] {
            assert_eq!(
                policy.decide(10, 20, bytes_per_cell, &full, false).mode,
                UploadMode::Full
            );
            assert_eq!(
                policy.decide(10, 20, bytes_per_cell, &ranges, false).mode,
                UploadMode::Incremental
            );
        }
        // Reads and skipped draws never consume the pending ranges.
        assert_eq!(update_dirty_ranges(&terminal.read_update(now)), ranges);
        terminal.invalidate_update();
        assert!(!terminal.acknowledge_update(&incremental));
        assert_eq!(update_dirty_ranges(&terminal.read_update(now)), full);
    }

    #[test]
    fn gpu_pipeline_prepares_replayed_updates_and_invalidates_projection() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let Ok(adapter) =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: None,
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            }))
        else {
            eprintln!("no GPU adapter; CPU upload policy test still runs");
            return;
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("terminal update test device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::default(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            trace: wgpu::Trace::Off,
        }))
        .expect("test GPU device");
        let format = wgpu::TextureFormat::Bgra8Unorm;
        let gpu = TerminalGpuAccess::new(&device, &queue, format);
        let fonts = harbor_text::load_system_fonts(&harbor_config::FontSettings::default())
            .expect("test system font");
        let metrics = TextMetrics::from_font_metrics(fonts.font_metrics());
        let mut terminal = Terminal::new_headless(4, 8);
        let now = Instant::now();
        let first = terminal.read_update(now);
        let mut renderer =
            TerminalRenderPipeline::new(gpu, (160, 120), fonts, metrics, &first).unwrap();
        renderer.prepare(gpu, &first, true);
        assert!(terminal.acknowledge_update(&first));

        // An unrendered incremental update must still be available on the next live frame.
        terminal.put_str("a");
        let pending = terminal.read_update(now);
        assert!(matches!(pending.damage, UpdateDamage::Ranges(_)));
        renderer.prepare(gpu, &terminal.read_update(now), true);
        assert!(terminal.acknowledge_update(&pending));

        let mut viewport = renderer.viewport();
        viewport.allocation_origin = (3.0, 5.0);
        viewport.surface_size = (256, 256);
        viewport.cell_width *= 1.25; // Physical font metrics after a DPI change.
        viewport.line_height *= 1.25;
        renderer.sync_viewport(viewport, false);
        assert!(renderer.background.is_dirty());
        assert!(renderer.text.is_dirty());
        assert!(renderer.decoration.is_dirty());
        assert!(renderer.selection.is_dirty());
        assert!(renderer.cursor.is_dirty());

        terminal.put_bytes(b"\x1b]11;#445566\x07");
        let updated = terminal.read_update(now);
        renderer.sync_palette(updated.appearance.palette());
        assert!(renderer.background.is_dirty());
        assert!(renderer.text.is_dirty());
        assert!(renderer.decoration.is_dirty());
        renderer.prepare(gpu, &updated, true);
        assert!(terminal.acknowledge_update(&updated));

        let mut recolored = updated.appearance.palette();
        recolored.selection = harbor_config::Rgba::new(0.4, 0.3, 0.2, 0.5);
        renderer.sync_palette(recolored);
        assert!(renderer.selection.is_dirty());
        // The coherent update restores the engine palette before acknowledging.
        renderer.prepare(gpu, &terminal.read_update(now), true);
        assert!(!renderer.selection.is_dirty());

        terminal.put_str("b\nc\nd\ne\nf\n");
        assert!(terminal.read_update(now).snapshot.scroll_count > 0);
        terminal.pointer.set_viewport(viewport);
        for (phase, x) in [
            (crate::TerminalPointerPhase::Down, 20.0),
            (crate::TerminalPointerPhase::Move, 50.0),
            (crate::TerminalPointerPhase::Up, 50.0),
        ] {
            terminal
                .handle_event(crate::TerminalEvent::Pointer(
                    crate::TerminalPointerEvent::new(
                        (x, 20.0),
                        phase,
                        crate::TerminalPointerButton::Left,
                        1,
                    ),
                ))
                .unwrap();
        }
        assert!(terminal.read_update(now).selection.is_some());

        terminal
            .handle_event(crate::TerminalEvent::Preedit(crate::Preedit::new(
                "ime", None,
            )))
            .unwrap();
        terminal.resize_if_changed(crate::TerminalSize { rows: 5, cols: 9 });
        terminal.invalidate_update(); // Simulate a lost/uncertain projection.
        let replay = terminal.read_update(now);
        assert_eq!(replay.damage, UpdateDamage::FullUpload);
        renderer.sync_viewport(viewport, true);
        renderer.prepare(gpu, &replay, true);
        assert!(terminal.acknowledge_update(&replay));
        assert!(!renderer.text.is_dirty());

        // Encode and submit all layers, including preedit and scrollbar, for GPU validation.
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("terminal update test target"),
            size: wgpu::Extent3d {
                width: 256,
                height: 256,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("terminal update test pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            renderer.draw(&mut pass);
        }
        queue.submit(Some(encoder.finish()));
    }
}
