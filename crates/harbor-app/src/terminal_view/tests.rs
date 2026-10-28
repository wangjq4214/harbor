#[path = "tests/gpu.rs"]
mod gpu;

use super::*;
use harbor_widget::input::event::{
    Key as WidgetKey, KeyboardEvent, Modifiers, PointerButton, PointerEvent, PointerPhase, UiEvent,
};
use harbor_widget::layout::{Point, Rect, Size};
use harbor_widget::renderer::Viewport;
use harbor_widget::scene::primitive::ExternalDrawContext;
use harbor_widget::winit::WindowPresenter;
use std::cell::Cell;
use std::sync::atomic::AtomicBool;

fn decoration_range(
    terminal: &Terminal,
    row: usize,
    units: std::ops::Range<usize>,
) -> harbor_terminal::read_model::ContentRange {
    use harbor_terminal::read_model::{ReadBudget, ReadScope};
    terminal
        .read_content(
            ReadScope::Visible {
                start_row: row,
                row_count: 1,
            },
            ReadBudget::default(),
        )
        .unwrap()
        .fragments[0]
        .range(units)
        .unwrap()
}

fn assert_cached_projection(cache: &mut DecorationProjection, terminal: &Terminal) {
    let expected = terminal.project_content_ranges(&cache.ranges);
    assert_eq!(cache.project(terminal), expected);
    let count = cache.projections;
    assert_eq!(cache.project(terminal), expected);
    assert_eq!(
        cache.projections, count,
        "unchanged frames must not reproject"
    );
}

#[test]
fn decoration_projection_caches_and_invalidates_viewport_and_publication() {
    use harbor_terminal::read_model::CellSpan;
    let mut terminal = Terminal::new_headless(2, 8);
    terminal.process_output("ab界\r\nnext".as_bytes());
    let range = decoration_range(&terminal, 0, 2..3);
    let mut cache = DecorationProjection::default();
    cache.set_ranges(vec![range.clone()]);
    assert_eq!(
        cache.project(&terminal),
        [CellSpan {
            row: 0,
            start_col: 2,
            end_col: 4
        }]
    );
    assert_cached_projection(&mut cache, &terminal);
    assert_eq!(cache.projections, 1);

    // Unrelated row output revalidates proofs but keeps the original match.
    terminal.process_output(b"!");
    assert_cached_projection(&mut cache, &terminal);
    assert_eq!(cache.projections, 2);
    assert!(!cache.spans.is_empty());

    terminal.process_output(b"\r\nlast");
    assert_cached_projection(&mut cache, &terminal);
    assert!(cache.spans.is_empty(), "match is now above the viewport");
    let version = terminal.content_version();
    terminal.scroll_viewport_up(1);
    assert_eq!(
        terminal.content_version(),
        version,
        "scrolling is not a semantic edit"
    );
    assert_cached_projection(&mut cache, &terminal);
    assert_eq!(cache.spans[0].row, 0);
    let count = cache.projections;
    terminal.scroll_viewport_to_bottom();
    assert_cached_projection(&mut cache, &terminal);
    assert_eq!(cache.projections, count + 1);
    assert!(cache.spans.is_empty());

    terminal.scroll_viewport_up(1);
    assert_cached_projection(&mut cache, &terminal);
    cache.set_ranges(vec![decoration_range(&terminal, 1, 0..2)]);
    assert_cached_projection(&mut cache, &terminal);
    assert_eq!(
        cache.spans,
        [CellSpan {
            row: 1,
            start_col: 0,
            end_col: 2
        }]
    );
    cache.set_ranges(vec![]);
    assert_cached_projection(&mut cache, &terminal);
    assert!(
        cache.spans.is_empty(),
        "query cancellation removes cached spans"
    );
}

#[test]
fn decoration_projection_never_reuses_stale_proofs_across_content_lifetimes() {
    // Overwrite, erase, reset, alternate entry, resize/reflow and eviction must
    // all replace a populated cache with fresh (empty) geometry.
    for change in 0..7 {
        let mut terminal = Terminal::new_headless(2, 8);
        terminal.process_output(b"needle");
        let range = decoration_range(&terminal, 0, 0..6);
        let mut cache = DecorationProjection::default();
        cache.set_ranges(vec![range]);
        assert_cached_projection(&mut cache, &terminal);
        assert!(!cache.spans.is_empty());
        match change {
            0 => terminal.process_output(b"\rxxxxxx"),
            1 => terminal.process_output(b"\r\x1b[2K"),
            2 => terminal.process_output(b"\x1bcneedle"),
            3 => terminal.process_output(b"\x1b[?1049hneedle"),
            4 => {
                terminal
                    .try_resize_if_changed(TerminalSize { rows: 2, cols: 4 })
                    .unwrap();
            }
            5 => {
                terminal
                    .try_resize_if_changed(TerminalSize { rows: 3, cols: 8 })
                    .unwrap();
            }
            6 => terminal.process_output("other\r\n".repeat(1100).as_bytes()),
            _ => unreachable!(),
        }
        assert_cached_projection(&mut cache, &terminal);
        assert_eq!(
            cache.projections, 2,
            "change {change} must miss the CPU cache"
        );
        assert!(
            cache.spans.is_empty(),
            "change {change} retained stale geometry"
        );
        if change == 3 {
            terminal.process_output(b"\x1b[?1049l");
            assert_cached_projection(&mut cache, &terminal);
            // The restored primary's unchanged proof can legitimately be visible again.
            assert_eq!(cache.projections, 3);
        }
    }

    let mut original = Terminal::new_headless(2, 8);
    original.process_output(b"needle");
    let mut cache = DecorationProjection::default();
    cache.set_ranges(vec![decoration_range(&original, 0, 0..6)]);
    assert!(!cache.project(&original).is_empty());
    let mut replacement = Terminal::new_headless(2, 8);
    replacement.process_output(b"needle");
    assert_cached_projection(&mut cache, &replacement);
    assert!(
        cache.spans.is_empty(),
        "same text in a new session is not the same proof"
    );
}

#[test]
fn bridge_alternate_screen_query_drains_queued_output_without_snapshot_or_acknowledgement() {
    struct QueuedReader(std::sync::mpsc::Receiver<Vec<u8>>);
    impl std::io::Read for QueuedReader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let Ok(bytes) = self.0.recv() else {
                return Ok(0);
            };
            assert!(bytes.len() <= buf.len());
            buf[..bytes.len()].copy_from_slice(&bytes);
            Ok(bytes.len())
        }
    }
    let (output_tx, output_rx) = std::sync::mpsc::channel();
    let (wake_tx, wake_rx) = std::sync::mpsc::channel();
    #[allow(clippy::arc_with_non_send_sync)]
    let terminal = Arc::new(Mutex::new(Terminal::new_headless_with_io(
        2,
        8,
        QueuedReader(output_rx),
        std::io::sink(),
        move || wake_tx.send(()).is_ok(),
    )));
    let bridge = TerminalWidgetBridge::new(1, terminal.clone(), Arc::new(AtomicBool::new(false)));
    for (bytes, expected) in [
        (b"\x1b[?1049h".to_vec(), true),
        (b"\x1b[?1049l".to_vec(), false),
    ] {
        output_tx.send(bytes).unwrap();
        wake_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        assert_eq!(terminal.lock().unwrap().is_alt_screen(), !expected);
        assert_eq!(
            bridge.is_alt_screen(),
            expected,
            "availability must ingest queued modes"
        );
        assert!(!terminal.lock().unwrap().snapshot().dirty_ranges.is_empty());
        assert_eq!(bridge.is_alt_screen(), expected);
    }
    drop(output_tx);
}

fn context(logical: Rect, physical: (u32, u32), scale: f32) -> ExternalDrawContext {
    ExternalDrawContext::new(logical, Viewport::new(physical.0, physical.1, scale))
}

fn metrics() -> TextMetrics {
    TextMetrics {
        cell_width: 10.0,
        line_height: 20.0,
        ascent: 16.0,
        underline_position: 16.0,
        underline_thickness: 2.0,
        strikethrough_position: 10.0,
        strikethrough_thickness: 2.0,
    }
}

#[test]
fn skipped_widget_frame_preserves_update_until_live_projection_and_rechecks_dpi() {
    let now = std::time::Instant::now();
    let metrics = metrics();
    let target = RenderTarget::new((0.0, 0.0), (100, 40), (100, 40));
    let viewport = RenderViewport::from_target(target, &metrics);
    let size = viewport.compute_grid_size();
    let mut terminal = Terminal::new_headless(size.rows, size.cols);
    assert!(terminal.acknowledge_update(&terminal.read_update(now)));
    terminal.process_output(b"hi");
    assert!(!needs_live_projection(
        ExternalDrawMode::Retain,
        &terminal,
        viewport,
        target,
        &metrics,
    ));
    assert_eq!(terminal.read_update(now).snapshot.cell_char(0, 0), 'h');
    assert!(!terminal.read_update(now).snapshot.dirty_ranges.is_empty());
    assert!(needs_live_projection(
        ExternalDrawMode::Live,
        &terminal,
        viewport,
        target,
        &metrics,
    ));
    let scale_only = RenderTarget::new_with_scale((0.0, 0.0), (100, 40), (100, 40), 2.0);
    assert!(needs_live_projection(
        ExternalDrawMode::Retain,
        &terminal,
        viewport,
        scale_only,
        &metrics,
    ));
    let (same_viewport, changed) = terminal.prepare_render_frame(scale_only, &metrics, now);
    assert!(!changed);
    assert_eq!(same_viewport, viewport);
    assert!(!needs_live_projection(
        ExternalDrawMode::Retain,
        &terminal,
        viewport,
        scale_only,
        &metrics,
    ));
    let dpi_target = RenderTarget::new_with_scale((0.0, 0.0), (200, 80), (200, 80), 2.0);
    assert!(needs_live_projection(
        ExternalDrawMode::Retain,
        &terminal,
        viewport,
        dpi_target,
        &metrics,
    ));
    let (new_viewport, _) = terminal.prepare_render_frame(dpi_target, &metrics, now);
    assert!(terminal.acknowledge_update(&terminal.read_update(now)));
    assert!(!needs_live_projection(
        ExternalDrawMode::Retain,
        &terminal,
        new_viewport,
        dpi_target,
        &metrics,
    ));
}

fn assert_rendered_bridge_replays_hidden_output_and_retained_resize(
    geometry_metrics: Option<TextMetrics>,
) {
    use harbor_widget::scene::primitive::ExternalDrawGpu;
    let (device, queue) = crate::test_gpu::device();
    let format = wgpu::TextureFormat::Bgra8Unorm;
    let gpu = TerminalGpuAccess::new(&device, &queue, format);
    let fonts = harbor_terminal::load_system_fonts(&harbor_config::FontSettings::default())
        .expect("test system fonts");
    let metrics =
        geometry_metrics.unwrap_or_else(|| TextMetrics::from_font_metrics(fonts.font_metrics()));
    let terminal = headless_terminal(4, 8);
    let renderer = TerminalRenderPipeline::new(
        gpu,
        (160, 120),
        fonts,
        metrics,
        &terminal
            .lock()
            .unwrap()
            .read_update(std::time::Instant::now()),
    )
    .expect("terminal renderer");
    let weak = Arc::downgrade(&terminal);
    let bridge = TerminalWidgetBridge::new_rendered_with_hyperlink_activation(
        73,
        Arc::clone(&terminal),
        renderer,
        Arc::new(AtomicBool::new(false)),
        Arc::new(|_| {}),
    );
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("terminal bridge target"),
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
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let initial = context(
        Rect::from_min_size(Point::ZERO, Size::new(160.0, 120.0)),
        (256, 256),
        1.0,
    );
    let resized = context(
        Rect::from_min_size(Point::ZERO, Size::new(192.0, 128.0)),
        (256, 256),
        1.0,
    );
    let draw = |ctx: &ExternalDrawContext, id: u64, mode| {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("terminal bridge pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            (bridge.handler)(
                id.into(),
                ctx,
                ExternalDrawGpu::new(&device, &queue, format),
                &mut pass,
                mode,
            );
        }
        queue.submit([encoder.finish()]);
    };
    draw(&initial, 73, ExternalDrawMode::Live);
    terminal.lock().unwrap().process_output(b"ab");
    let range = decoration_range(&terminal.lock().unwrap(), 0, 0..2);
    bridge.set_decorations(vec![range]);
    draw(&initial, 73, ExternalDrawMode::Live);
    assert!(!bridge.decorations.lock().unwrap().spans.is_empty());
    let projections = bridge.decorations.lock().unwrap().projections;
    for mode in [
        ExternalDrawMode::Retain,
        ExternalDrawMode::Live,
        ExternalDrawMode::Retain,
    ] {
        draw(&initial, 73, mode);
        assert_eq!(bridge.decorations.lock().unwrap().projections, projections);
    }
    terminal.lock().unwrap().process_output(b"\rxx");
    draw(&initial, 73, ExternalDrawMode::Retain);
    assert!(bridge.decorations.lock().unwrap().spans.is_empty());
    assert_eq!(
        bridge.decorations.lock().unwrap().projections,
        projections + 1
    );
    draw(&initial, 73, ExternalDrawMode::Live);
    assert_eq!(
        bridge.decorations.lock().unwrap().projections,
        projections + 1
    );
    bridge.set_decorations(vec![]);
    terminal
        .lock()
        .unwrap()
        .handle_event(TerminalEvent::Preedit(Preedit::new("x", None)))
        .unwrap();
    assert!(
        (bridge.ime)(73.into(), &initial)
            .unwrap()
            .position
            .is_some()
    );
    assert!((bridge.ime)(99.into(), &initial).is_none());
    terminal.lock().unwrap().clear_preedit();
    terminal.lock().unwrap().process_output(b"hidden");
    draw(&initial, 73, ExternalDrawMode::Retain);
    draw(&initial, 99, ExternalDrawMode::Live);
    assert!(!terminal.lock().unwrap().snapshot().dirty_ranges.is_empty());
    draw(&initial, 73, ExternalDrawMode::Live);
    assert!(terminal.lock().unwrap().snapshot().dirty_ranges.is_empty());
    // Rejected targets must not acknowledge damage. Recovery consumes it
    // only after the renderer accepts the viewport and prepares every layer.
    for rejected in [
        context(
            Rect::from_min_size(Point::ZERO, Size::new(0.0, 120.0)),
            (256, 256),
            1.0,
        ),
        context(
            Rect::from_min_size(Point::ZERO, Size::new(160.0, 120.0)),
            (0, 256),
            1.0,
        ),
    ] {
        terminal
            .lock()
            .unwrap()
            .process_output(b"\x1b[4;9;53mrejected");
        draw(&rejected, 73, ExternalDrawMode::Live);
        assert!(!terminal.lock().unwrap().snapshot().dirty_ranges.is_empty());
        draw(&initial, 73, ExternalDrawMode::Live);
        assert!(terminal.lock().unwrap().snapshot().dirty_ranges.is_empty());
    }
    // The remaining checks exercise successful resize/draw coordination, not
    // the model's rejection of a cursor pushed into history. Styled scroll
    // fills can leave meaningful blanks after the cursor; at tiny font-
    // dependent grids those blanks wrap below it and make a shrink invalid.
    // Reset the pen and erase only that suffix, preserving the printed text
    // and its decorations already exercised by the rejected-target checks.
    terminal.lock().unwrap().process_output(b"\x1b[0m\x1b[0J");
    let scaled = context(
        Rect::from_min_size(Point::ZERO, Size::new(80.0, 60.0)),
        (256, 256),
        2.0,
    );
    terminal.lock().unwrap().process_output(b"dpi");
    draw(&scaled, 73, ExternalDrawMode::Retain);
    assert!(terminal.lock().unwrap().snapshot().dirty_ranges.is_empty());
    let expected_scaled =
        RenderViewport::from_target(render_target_from_context(&scaled), &metrics)
            .compute_grid_size();
    let scaled_snapshot = terminal.lock().unwrap().snapshot();
    assert_eq!(
        (scaled_snapshot.rows, scaled_snapshot.cols),
        (expected_scaled.rows, expected_scaled.cols),
        "host DPI must preserve main's physical font metrics"
    );
    draw(&resized, 73, ExternalDrawMode::Retain);
    let expected = RenderViewport::from_target(render_target_from_context(&resized), &metrics)
        .compute_grid_size();
    let snapshot = terminal.lock().unwrap().snapshot();
    assert_eq!(
        (snapshot.rows, snapshot.cols),
        (expected.rows, expected.cols)
    );
    assert!(snapshot.dirty_ranges.is_empty());
    // The product coordinator broadcasts sizes using startup metrics before the
    // bridge draws. Both phases must agree, including after a skipped draw/DPI change.
    terminal.lock().unwrap().use_host_resize_scheduling();
    for scale in [1.0, 1.25, 1.5, 2.0, 1.5, 1.0] {
        for (width, height) in [(80.0, 60.0), (96.0, 64.0), (80.0, 60.0)] {
            let rect = Rect::from_min_size(Point::ZERO, Size::new(width, height));
            let ctx = context(rect, (256, 256), scale);
            let size = terminal_size_from_allocation(rect, scale, (256, 256), &metrics)
                .expect("drawable allocation");
            terminal
                .lock()
                .unwrap()
                .try_resize_if_changed(size)
                .unwrap();
            let before_admission = terminal.lock().unwrap().snapshot();
            draw(&ctx, 99, ExternalDrawMode::Live); // unmatched/skipped frame
            draw(&ctx, 73, ExternalDrawMode::Retain); // allocation can precede admission
            let waiting = terminal.lock().unwrap().snapshot();
            assert_eq!(
                (waiting.rows, waiting.cols),
                (before_admission.rows, before_admission.cols),
                "drawing must not bypass host admission"
            );
            terminal.lock().unwrap().advance_resize().unwrap();
            let before_draw = terminal.lock().unwrap().snapshot();
            draw(&ctx, 73, ExternalDrawMode::Retain);
            let after_draw = terminal.lock().unwrap().snapshot();
            assert_eq!(
                (after_draw.rows, after_draw.cols),
                (size.rows, size.cols),
                "allocation and drawing disagree at scale {scale}"
            );
            assert_eq!(
                after_draw.cells, before_draw.cells,
                "draw must not reflow again"
            );
            assert!(after_draw.dirty_ranges.is_empty());
        }
    }
    drop(bridge);
    drop(terminal);
    assert!(weak.upgrade().is_none());
}

#[allow(clippy::arc_with_non_send_sync)]
fn headless_terminal(rows: usize, cols: usize) -> Arc<Mutex<Terminal>> {
    Arc::new(Mutex::new(Terminal::new_headless(rows, cols)))
}

#[test]
fn should_map_1x_scale_physical_allocation_to_render_target() {
    // Arrange
    let ctx = context(
        Rect::from_min_size(Point::new(10.0, 5.0), Size::new(200.0, 100.0)),
        (800, 600),
        1.0,
    );

    // Act
    let target = render_target_from_context(&ctx);

    // Assert
    assert_eq!(target.allocation_origin, (10.0, 5.0));
    assert_eq!(target.allocation_size, (200, 100));
    assert_eq!(target.surface_size, (800, 600));
}

#[test]
fn should_map_2x_scale_physical_allocation_to_render_target() {
    // Arrange
    let ctx = context(
        Rect::from_min_size(Point::new(10.0, 5.0), Size::new(200.0, 100.0)),
        (1600, 1200),
        2.0,
    );

    // Act
    let target = render_target_from_context(&ctx);

    // Assert: logical 200×100 at 2× → physical 400×200
    assert_eq!(target.allocation_origin, (20.0, 10.0));
    assert_eq!(target.allocation_size, (400, 200));
    assert_eq!(target.surface_size, (1600, 1200));
}

#[test]
fn should_preserve_zero_size_physical_allocation() {
    // Arrange
    let ctx = context(
        Rect::from_min_size(Point::new(0.0, 0.0), Size::new(0.0, 0.0)),
        (800, 600),
        1.0,
    );

    // Act
    let target = render_target_from_context(&ctx);

    // Assert
    assert_eq!(target.allocation_origin, (0.0, 0.0));
    assert_eq!(target.allocation_size, (0, 0));
    assert_eq!(target.surface_size, (800, 600));
}

#[test]
fn should_round_fractional_logical_allocation_when_mapping_to_render_target() {
    // Arrange: floor origin, ceil far edge (via ExternalDrawContext::physical_allocation)
    let ctx = context(
        Rect::from_min_size(Point::new(0.4, 0.6), Size::new(100.2, 50.4)),
        (800, 600),
        1.0,
    );

    // Act
    let target = render_target_from_context(&ctx);

    // Assert: floor(0.4)=0, ceil(100.6)=101; floor(0.6)=0, ceil(51.0)=51
    assert_eq!(target.allocation_origin, (0.0, 0.0));
    assert_eq!(target.allocation_size, (101, 51));
    assert_eq!(target.surface_size, (800, 600));
}

#[test]
fn should_round_fractional_logical_allocation_at_2x_scale() {
    // Arrange
    let ctx = context(
        Rect::from_min_size(Point::new(0.4, 0.6), Size::new(100.2, 50.4)),
        (1600, 1200),
        2.0,
    );

    // Act
    let target = render_target_from_context(&ctx);

    // Assert: floor(0.8)=0, ceil(201.2)=202; floor(1.2)=1, ceil(102.0)=102
    assert_eq!(target.allocation_origin, (0.0, 1.0));
    assert_eq!(target.allocation_size, (202, 101));
    assert_eq!(target.surface_size, (1600, 1200));
}

#[test]
fn terminal_size_uses_external_rounding_and_shared_grid_rules() {
    let metrics = metrics();
    let rect = Rect::from_min_size(Point::new(0.4, 0.6), Size::new(100.2, 50.4));
    let size = terminal_size_from_allocation(rect, 1.5, (1200, 900), &metrics).unwrap();
    let physical_width = 151.0_f32;
    let physical_height = 77.0_f32;
    let padding = 2.0 * harbor_config::TEXT_PADDING;
    assert_eq!(
        size.cols,
        ((physical_width - padding) / 10.0).floor() as usize
    );
    assert_eq!(
        size.rows,
        ((physical_height - padding) / 20.0).floor() as usize
    );
}

#[test]
fn terminal_size_uses_only_surface_visible_allocation() {
    let metrics = metrics();
    let clipped = Rect::from_min_size(Point::new(80.0, 40.0), Size::new(120.0, 120.0));
    let context = context(clipped, (140, 120), 1.0);
    let target = render_target_from_context(&context);
    assert_eq!(target.allocation_origin, (80.0, 40.0));
    assert_eq!(target.allocation_size, (60, 80));
    assert_eq!(
        terminal_size_from_allocation(clipped, 1.0, (140, 120), &metrics),
        Some(TerminalSize { rows: 2, cols: 2 })
    );

    let outside = Rect::from_min_size(Point::new(150.0, 40.0), Size::new(120.0, 120.0));
    assert_eq!(
        terminal_size_from_allocation(outside, 1.0, (140, 120), &metrics),
        None
    );
}

#[test]
fn terminal_size_matches_supported_two_column_minimum() {
    let metrics = metrics();
    let tiny = Rect::from_min_size(Point::ZERO, Size::new(35.0, 35.0));
    let size = terminal_size_from_allocation(tiny, 1.0, (140, 120), &metrics);
    assert_eq!(size, Some(TerminalSize { rows: 1, cols: 2 }));
    let mut terminal = Terminal::new_headless(4, 20);
    assert!(terminal.resize_if_changed(size.unwrap()));
    assert_eq!((terminal.screen().rows(), terminal.screen().cols()), (1, 2));
}

#[test]
fn terminal_size_rejects_non_drawable_and_invalid_geometry() {
    let metrics = metrics();
    let valid = Rect::from_min_size(Point::ZERO, Size::new(100.0, 50.0));
    assert_eq!(
        terminal_size_from_allocation(valid, 1.0, (0, 600), &metrics),
        None
    );
    assert_eq!(
        terminal_size_from_allocation(
            Rect::from_min_size(Point::ZERO, Size::ZERO),
            1.0,
            (800, 600),
            &metrics,
        ),
        None
    );
    assert_eq!(
        terminal_size_from_allocation(valid, f32::NAN, (800, 600), &metrics),
        None
    );
}

#[test]
fn should_use_caller_provided_draw_id() {
    let terminal = headless_terminal(24, 80);
    let gate = Arc::new(AtomicBool::new(false));

    let bridge = TerminalWidgetBridge::new(41, terminal, gate);

    assert_eq!(bridge.draw_id(), 41);
}

#[test]
fn should_forward_one_hyperlink_activation_to_injected_callback() {
    let received = Arc::new(Mutex::new(Vec::new()));
    let callback_received = Arc::clone(&received);
    let callback: Arc<dyn Fn(String) + Send + Sync> = Arc::new(move |uri| {
        callback_received.lock().unwrap().push(uri);
    });

    forward_hyperlink_activation(Some("https://example.test".to_owned()), callback.as_ref());
    forward_hyperlink_activation(None, callback.as_ref());

    assert_eq!(
        received.lock().unwrap().as_slice(),
        ["https://example.test"]
    );
}

#[test]
fn should_reuse_cached_handler_arc_when_building_multiple_widgets() {
    // Arrange: handler is created once in `new` and cloned into each widget.
    let terminal = headless_terminal(24, 80);
    let bridge = TerminalWidgetBridge::new(42, terminal, Arc::new(AtomicBool::new(false)));
    let cached = Arc::clone(&bridge.handler);
    assert_eq!(Arc::strong_count(&cached), 2);

    // Act
    let widget_a = terminal_widget(bridge.clone());
    let count_after_first = Arc::strong_count(&cached);

    let widget_b = terminal_widget(bridge.clone());
    let count_after_second = Arc::strong_count(&cached);

    // Assert: each widget clones the same Arc (not a freshly allocated handler).
    assert!(Arc::ptr_eq(&cached, &bridge.handler));
    assert!(count_after_first > 2);
    assert!(count_after_second > count_after_first);

    drop(widget_a);
    drop(widget_b);
    assert_eq!(Arc::strong_count(&cached), 2);
}

#[test]
fn should_skip_draw_when_external_draw_id_mismatches() {
    // Arrange
    let ctx = context(
        Rect::from_min_size(Point::ZERO, Size::new(100.0, 100.0)),
        (800, 600),
        1.0,
    );
    let called = Cell::new(false);

    // Act
    dispatch_matched_draw(41, 42, &ctx, |_| {
        called.set(true);
    });

    // Assert
    assert!(!called.get());
}

#[test]
fn should_draw_when_external_draw_id_matches() {
    // Arrange
    let ctx = context(
        Rect::from_min_size(Point::new(10.0, 5.0), Size::new(200.0, 100.0)),
        (800, 600),
        1.0,
    );
    let drawn = Cell::new(None);

    // Act
    dispatch_matched_draw(41, 41, &ctx, |target| {
        drawn.set(Some(target));
    });
    // Assert
    let target = drawn.get().expect("draw invoked");
    assert_eq!(target.allocation_origin, (10.0, 5.0));
    assert_eq!(target.allocation_size, (200, 100));
    assert_eq!(target.surface_size, (800, 600));
}

#[test]
fn should_map_keyboard_key_down_with_modifiers() {
    // Arrange
    let event = UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: WidgetKey::Enter,
        modifiers: Modifiers {
            ctrl: true,
            ..Modifiers::default()
        },
    });

    // Act
    let mapped = terminal_event_from_ui_event(event);

    // Assert
    assert_eq!(
        mapped,
        Some(TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown {
            key: TerminalKey::Enter,
            modifiers: TerminalModifiers {
                ctrl: true,
                ..TerminalModifiers::default()
            },
        }))
    );
}

#[test]
fn should_map_ime_and_focus_and_wheel() {
    // Arrange / Act / Assert
    assert_eq!(
        terminal_event_from_ui_event(UiEvent::Keyboard(KeyboardEvent::Ime("你好".into()))),
        Some(TerminalEvent::Keyboard(TerminalKeyboardEvent::Ime(
            "你好".into()
        )))
    );
    assert_eq!(
        terminal_event_from_ui_event(UiEvent::ImePreedit(
            harbor_widget::input::event::ImePreedit::new("ni", Some((2, 2))),
        )),
        Some(TerminalEvent::Preedit(Preedit::new("ni", Some((2, 2)))))
    );
    assert_eq!(
        terminal_event_from_ui_event(UiEvent::Focus(FocusEvent::Gained)),
        Some(TerminalEvent::Focus(TerminalFocusEvent::Gained))
    );

    assert_eq!(
        terminal_event_from_ui_event(UiEvent::Focus(FocusEvent::GainedVisible)),
        Some(TerminalEvent::Focus(TerminalFocusEvent::Gained))
    );
    assert_eq!(
        terminal_event_from_ui_event(UiEvent::Pointer(PointerEvent::new(
            Point::new(1.0, 2.0),
            PointerPhase::WheelLine { dx: 0.0, dy: -1.0 },
            PointerButton::Left,
            3,
        ))),
        Some(TerminalEvent::Pointer(TerminalPointerEvent::new(
            (1.0, 2.0),
            TerminalPointerPhase::WheelLine { dx: 0.0, dy: -1.0 },
            TerminalPointerButton::Left,
            3,
        )))
    );
}

#[test]
fn should_preserve_both_wheel_axes_and_units_at_terminal_bridge() {
    for (ui_phase, terminal_phase) in [
        (
            PointerPhase::WheelLine { dx: 1.5, dy: -2.0 },
            TerminalPointerPhase::WheelLine { dx: 1.5, dy: -2.0 },
        ),
        (
            PointerPhase::WheelPixel { dx: -0.25, dy: 3.5 },
            TerminalPointerPhase::WheelPixel { dx: -0.25, dy: 3.5 },
        ),
    ] {
        assert_eq!(
            terminal_event_from_ui_event(UiEvent::Pointer(PointerEvent::new(
                Point::new(1.0, 2.0),
                ui_phase,
                PointerButton::Left,
                3
            ))),
            Some(TerminalEvent::Pointer(TerminalPointerEvent::new(
                (1.0, 2.0),
                terminal_phase,
                TerminalPointerButton::Left,
                3
            )))
        );
    }
}

#[test]
fn should_suppress_non_wheel_when_gate_active() {
    let key = UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: WidgetKey::Enter,
        modifiers: Modifiers::default(),
    });
    let wheel = UiEvent::Pointer(PointerEvent::new(
        Point::ZERO,
        PointerPhase::WheelLine { dx: 0.0, dy: 1.0 },
        PointerButton::Left,
        0,
    ));
    let move_event = UiEvent::Pointer(PointerEvent::new(
        Point::ZERO,
        PointerPhase::Move,
        PointerButton::Left,
        0,
    ));

    assert!(!gate_suppresses_event(false, &key));
    assert!(gate_suppresses_event(true, &key));
    assert!(!gate_suppresses_event(true, &wheel));
    assert!(gate_suppresses_event(true, &move_event));
}

#[test]
fn should_map_key_up_and_focus_lost() {
    // Arrange / Act / Assert
    assert_eq!(
        terminal_event_from_ui_event(UiEvent::Keyboard(KeyboardEvent::KeyUp {
            key: WidgetKey::Escape,
            modifiers: Modifiers {
                alt: true,
                ..Modifiers::default()
            },
        })),
        Some(TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyUp {
            key: TerminalKey::Escape,
            modifiers: TerminalModifiers {
                alt: true,
                ..TerminalModifiers::default()
            },
        }))
    );
    assert_eq!(
        terminal_event_from_ui_event(UiEvent::Focus(FocusEvent::Lost)),
        Some(TerminalEvent::Focus(TerminalFocusEvent::Lost))
    );
}

fn seed_scrollback(terminal: &mut Terminal) {
    for _ in 0..40 {
        terminal.process_output(b"line\r\n");
    }
}

fn runtime_with_bridge(
    terminal: Arc<Mutex<Terminal>>,
    gate: Arc<AtomicBool>,
) -> harbor_widget::runtime::Runtime {
    let bridge = TerminalWidgetBridge::new(1, terminal, gate);
    let mut rt = harbor_widget::runtime::Runtime::new();
    rt.set_root(terminal_widget(bridge));
    rt.update(std::time::Instant::now());
    assert!(rt.focus_first_focusable());
    let _ = rt.drain_external_input();
    rt
}

#[test]
fn should_leave_scrollback_unchanged_when_page_up_reaches_terminal_bridge() {
    // Arrange
    let terminal = headless_terminal(8, 40);
    seed_scrollback(&mut terminal.lock().unwrap());
    let gate = Arc::new(AtomicBool::new(false));
    let mut rt = runtime_with_bridge(Arc::clone(&terminal), Arc::clone(&gate));
    let offset_before = terminal.lock().unwrap().screen().view_offset();

    // Act
    let effects = rt.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: WidgetKey::PageUp,
        modifiers: Modifiers::default(),
    }));

    // Assert: application scroll interception has migrated out of terminal input.
    assert_eq!(
        terminal.lock().unwrap().screen().view_offset(),
        offset_before
    );
    assert!(!effects.request_redraw);
    assert!(rt.drain_external_input().is_empty());
}

#[test]
fn should_not_scroll_when_gate_suppresses_keydown() {
    // Arrange
    let terminal = headless_terminal(8, 40);
    seed_scrollback(&mut terminal.lock().unwrap());
    let gate = Arc::new(AtomicBool::new(true));
    let mut rt = runtime_with_bridge(Arc::clone(&terminal), Arc::clone(&gate));
    let offset_before = terminal.lock().unwrap().screen().view_offset();

    // Act
    let effects = rt.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: WidgetKey::PageUp,
        modifiers: Modifiers::default(),
    }));

    // Assert: gated non-wheel is swallowed without delivery or redraw wake.
    assert_eq!(
        terminal.lock().unwrap().screen().view_offset(),
        offset_before
    );
    assert!(!effects.request_redraw);
}

#[test]
fn should_scroll_when_gate_allows_wheel() {
    // Arrange
    let terminal = headless_terminal(8, 40);
    seed_scrollback(&mut terminal.lock().unwrap());
    let gate = Arc::new(AtomicBool::new(true));
    let mut rt = runtime_with_bridge(Arc::clone(&terminal), Arc::clone(&gate));
    let offset_before = terminal.lock().unwrap().screen().view_offset();

    // Act
    let effects = rt.dispatch(UiEvent::Pointer(PointerEvent::new(
        Point::new(10.0, 10.0),
        PointerPhase::WheelLine { dx: 0.0, dy: 2.0 },
        PointerButton::Left,
        0,
    )));

    // Assert: gated wheel still delivers and wakes when the viewport moves.
    assert!(terminal.lock().unwrap().screen().view_offset() > offset_before);
    assert!(effects.request_redraw);
}

#[test]
fn should_return_empty_schedule_demand_for_mismatched_draw_id() {
    // Arrange
    let terminal = headless_terminal(2, 4);
    let now = std::time::Instant::now();

    // Act
    let demand = schedule_demand_for_terminal(1, 99, &terminal, now);

    // Assert
    assert_eq!(demand, ExternalScheduleDemand::empty());
    assert!(demand.ordinary_present_eligible);
}

#[test]
fn should_map_headless_frame_demand_to_empty_schedule_demand() {
    // Arrange — headless engine still owns cursor blink timing.
    let terminal = headless_terminal(2, 4);
    let now = std::time::Instant::now();

    // Act
    let demand = schedule_demand_for_terminal(1, 1, &terminal, now);

    // Assert
    assert!(!demand.redraw_now);
    assert!(demand.deadline.is_some());
    assert!(demand.ordinary_present_eligible);
}

#[test]
fn should_copy_frame_demand_fields_when_draw_id_matches() {
    // Arrange
    let terminal = headless_terminal(2, 4);
    let now = std::time::Instant::now();
    let expected = terminal.lock().unwrap().frame_demand(now);

    // Act
    let demand = schedule_demand_for_terminal(1, 1, &terminal, now);

    // Assert
    assert_eq!(demand.redraw_now, expected.redraw_now);
    assert_eq!(demand.deadline, expected.deadline);
    assert_eq!(
        demand.ordinary_present_eligible,
        expected.ordinary_present_eligible
    );
    assert!(demand.ordinary_present_eligible);
}

#[test]
fn should_return_empty_schedule_demand_when_terminal_mutex_is_poisoned() {
    // Arrange — poison on this thread (Terminal is !Send)
    let terminal = headless_terminal(2, 4);
    let now = std::time::Instant::now();
    let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = terminal.lock().unwrap();
        panic!("poison schedule lock");
    }));
    assert!(poisoned.is_err());

    // Act
    let demand = schedule_demand_for_terminal(1, 1, &terminal, now);

    // Assert
    assert_eq!(demand, ExternalScheduleDemand::empty());
    assert!(demand.ordinary_present_eligible);
}

#[test]
fn should_emit_wait_until_from_runtime_when_bridge_registers_schedule() {
    use harbor_widget::effects::ControlFlowEffect;
    use std::time::Duration;

    // Arrange — fake schedule via ExternalSurface-equivalent bridge registration
    // is exercised through Runtime with a scripted provider in widget tests;
    // here verify bridge root registers a schedule that headless empties.
    let terminal = headless_terminal(2, 4);
    let mut rt = runtime_with_bridge(terminal, Arc::new(AtomicBool::new(false)));
    let now = std::time::Instant::now();

    // Act — clean idle turn after initial build consumed dirty work
    let _ = rt.update(now);
    let idle = rt.update(now + Duration::from_millis(1));

    // Assert — blink timing is scheduled even for a headless engine.
    assert!(!idle.request_redraw);
    assert_ne!(idle.control_flow, Some(ControlFlowEffect::Poll));
    assert!(matches!(
        idle.control_flow,
        Some(ControlFlowEffect::WaitUntil(_))
    ));
}

#[test]
fn should_copy_ineligible_demand_when_synchronized_output_is_enabled() {
    // Arrange
    let terminal = headless_terminal(2, 20);
    terminal.lock().unwrap().process_output(b"\x1b[?2026hhello");
    let now = std::time::Instant::now();

    // Act
    let demand = schedule_demand_for_terminal(1, 1, &terminal, now);

    // Assert
    assert!(!demand.ordinary_present_eligible);
    assert!(!demand.redraw_now);
    assert!(terminal.lock().unwrap().row_text(0).contains("hello"));
}

#[test]
fn should_copy_eligible_demand_when_matching_2026_disable_has_been_applied() {
    // Arrange
    let terminal = headless_terminal(2, 20);
    terminal
        .lock()
        .unwrap()
        .process_output(b"\x1b[?2026hhello\x1b[?2026l");
    let now = std::time::Instant::now();

    // Act
    let demand = schedule_demand_for_terminal(1, 1, &terminal, now);

    // Assert
    assert!(demand.ordinary_present_eligible);
    assert!(terminal.lock().unwrap().row_text(0).contains("hello"));
}

#[test]
fn should_return_empty_eligible_demand_when_draw_id_mismatches_ineligible_terminal() {
    // Arrange
    let terminal = headless_terminal(2, 20);
    terminal.lock().unwrap().process_output(b"\x1b[?2026hhello");
    let now = std::time::Instant::now();

    // Act
    let demand = schedule_demand_for_terminal(1, 99, &terminal, now);

    // Assert — mismatch ignores the ineligible terminal snapshot
    assert_eq!(demand, ExternalScheduleDemand::empty());
    assert!(demand.ordinary_present_eligible);
}

#[test]
fn should_skip_ordinary_redraw_when_about_to_wait_while_2026_is_enabled() {
    // Arrange
    let terminal = headless_terminal(2, 20);
    terminal.lock().unwrap().process_output(b"\x1b[?2026hhello");
    let mut rt = runtime_with_bridge(Arc::clone(&terminal), Arc::new(AtomicBool::new(false)));
    let now = std::time::Instant::now();
    let _ = rt.update(now);
    let mut presenter = WindowPresenter::new(800, 600, 1.0);

    // Act
    let idle = presenter.about_to_wait(&mut rt, now, None);

    // Assert
    assert!(!idle.request_redraw);
    assert!(idle.ordinary_present_eligible);
    assert!(idle.has_deferred_externals);
    assert!(!idle.force_present);
}

#[test]
fn should_force_commit_when_about_to_wait_after_recovery_while_2026_is_nested() {
    use harbor_widget::effects::ControlFlowEffect;
    use std::time::Duration;

    // Arrange
    let terminal = headless_terminal(2, 20);
    terminal
        .lock()
        .unwrap()
        .process_output(b"\x1b[?2026h\x1b[?2026hhello");
    let mut rt = runtime_with_bridge(Arc::clone(&terminal), Arc::new(AtomicBool::new(false)));
    let now = std::time::Instant::now();
    let _ = rt.update(now);
    let mut presenter = WindowPresenter::new(800, 600, 1.0);
    let _ = presenter.about_to_wait(&mut rt, now, None);

    // Act
    let due = presenter.about_to_wait(&mut rt, now + Duration::from_millis(100), None);

    // Assert — recovery live-commits without resetting nested DECRQM Set
    assert!(due.request_redraw);
    assert!(due.force_present);
    assert!(due.has_deferred_externals);
    assert!(matches!(
        due.control_flow,
        Some(ControlFlowEffect::WaitUntil(_))
    ));
    assert!(
        !terminal
            .lock()
            .unwrap()
            .frame_demand(now)
            .ordinary_present_eligible
    );
    assert!(terminal.lock().unwrap().row_text(0).contains("hello"));
}

#[test]
fn should_report_eligible_effects_when_about_to_wait_after_matching_disable() {
    // Arrange
    let terminal = headless_terminal(2, 20);
    terminal
        .lock()
        .unwrap()
        .process_output(b"\x1b[?2026hhello\x1b[?2026l");
    let mut rt = runtime_with_bridge(Arc::clone(&terminal), Arc::new(AtomicBool::new(false)));
    let mut presenter = WindowPresenter::new(800, 600, 1.0);

    // Act
    let idle = presenter.about_to_wait(&mut rt, std::time::Instant::now(), None);

    // Assert
    assert!(idle.ordinary_present_eligible);
    assert!(!idle.force_present);
}

#[test]
fn should_stay_eligible_when_trailing_disables_precede_later_output() {
    // Arrange
    let terminal = headless_terminal(2, 20);
    terminal
        .lock()
        .unwrap()
        .process_output(b"\x1b[?2026l\x1b[?2026llater");
    let now = std::time::Instant::now();

    // Act
    let demand = schedule_demand_for_terminal(1, 1, &terminal, now);

    // Assert
    assert!(demand.ordinary_present_eligible);
    assert!(terminal.lock().unwrap().row_text(0).contains("later"));
}

#[test]
fn should_cancel_deferred_externals_when_ris_clears_nested_2026() {
    use harbor_widget::effects::ControlFlowEffect;
    use std::time::Duration;

    // Arrange
    let terminal = headless_terminal(2, 20);
    terminal
        .lock()
        .unwrap()
        .process_output(b"\x1b[?2026h\x1b[?2026hhello");
    let mut rt = runtime_with_bridge(Arc::clone(&terminal), Arc::new(AtomicBool::new(false)));
    let now = std::time::Instant::now();
    let _ = rt.update(now);
    let mut presenter = WindowPresenter::new(800, 600, 1.0);
    let armed = presenter.about_to_wait(&mut rt, now, None);
    assert!(armed.has_deferred_externals);
    assert_eq!(
        armed.control_flow,
        Some(ControlFlowEffect::WaitUntil(
            now + Duration::from_millis(100)
        ))
    );

    // Act
    terminal.lock().unwrap().process_output(b"\x1bc");
    let idle = presenter.about_to_wait(&mut rt, now, None);

    // Assert
    assert!(idle.ordinary_present_eligible);
    assert!(!idle.has_deferred_externals);
    assert!(!idle.force_present);
    assert!(idle.request_redraw);
    assert_ne!(
        idle.control_flow,
        Some(ControlFlowEffect::WaitUntil(
            now + Duration::from_millis(100)
        ))
    );
    assert!(
        terminal
            .lock()
            .unwrap()
            .frame_demand(now)
            .ordinary_present_eligible
    );
}

#[test]
fn should_keep_deferred_externals_when_decstr_leaves_nested_2026() {
    use harbor_widget::effects::ControlFlowEffect;
    use std::time::Duration;

    // Arrange
    let terminal = headless_terminal(2, 20);
    terminal.lock().unwrap().process_output(b"\x1b[?2026hhello");
    let mut rt = runtime_with_bridge(Arc::clone(&terminal), Arc::new(AtomicBool::new(false)));
    let now = std::time::Instant::now();
    let _ = rt.update(now);
    let mut presenter = WindowPresenter::new(800, 600, 1.0);
    let _ = presenter.about_to_wait(&mut rt, now, None);

    // Act
    terminal.lock().unwrap().process_output(b"\x1b[!p");
    let idle = presenter.about_to_wait(&mut rt, now, None);

    // Assert
    assert!(idle.has_deferred_externals);
    assert!(!idle.force_present);
    assert_eq!(
        idle.control_flow,
        Some(ControlFlowEffect::WaitUntil(
            now + Duration::from_millis(100)
        ))
    );
    assert!(
        !terminal
            .lock()
            .unwrap()
            .frame_demand(now)
            .ordinary_present_eligible
    );
    assert!(terminal.lock().unwrap().row_text(0).contains("hello"));
}

#[test]
fn should_cancel_deferred_externals_when_pty_eof_clears_nested_2026() {
    use harbor_widget::effects::ControlFlowEffect;
    use std::io::Read;
    use std::time::Duration;

    struct PermitEofReader {
        permit: std::sync::mpsc::Receiver<()>,
        exited: std::sync::mpsc::Sender<()>,
    }
    impl Read for PermitEofReader {
        fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
            let _ = self.permit.recv();
            Ok(0)
        }
    }
    impl Drop for PermitEofReader {
        fn drop(&mut self) {
            let _ = self.exited.send(());
        }
    }

    let (permit_tx, permit_rx) = std::sync::mpsc::channel();
    let (exited_tx, exited_rx) = std::sync::mpsc::channel();
    #[allow(clippy::arc_with_non_send_sync)]
    let terminal = Arc::new(Mutex::new(Terminal::new_headless_with_io(
        2,
        20,
        PermitEofReader {
            permit: permit_rx,
            exited: exited_tx,
        },
        std::io::sink(),
        || true,
    )));
    terminal
        .lock()
        .unwrap()
        .process_output(b"\x1b[?2026h\x1b[?2026hhello");
    let mut rt = runtime_with_bridge(Arc::clone(&terminal), Arc::new(AtomicBool::new(false)));
    let now = std::time::Instant::now();
    let _ = rt.update(now);
    let mut presenter = WindowPresenter::new(800, 600, 1.0);
    let armed = presenter.about_to_wait(&mut rt, now, None);
    assert!(armed.has_deferred_externals);
    assert_eq!(
        armed.control_flow,
        Some(ControlFlowEffect::WaitUntil(
            now + Duration::from_millis(100)
        ))
    );

    permit_tx.send(()).expect("reader should still be waiting");
    exited_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("reader must drop before drain can observe disconnect");
    let idle = presenter.about_to_wait(&mut rt, now, None);

    assert!(idle.ordinary_present_eligible);
    assert!(!idle.has_deferred_externals);
    assert!(!idle.force_present);
    assert!(idle.request_redraw);
    assert_ne!(
        idle.control_flow,
        Some(ControlFlowEffect::WaitUntil(
            now + Duration::from_millis(100)
        ))
    );
    assert!(
        terminal
            .lock()
            .unwrap()
            .frame_demand(now)
            .ordinary_present_eligible
    );
    assert!(terminal.lock().unwrap().row_text(0).contains("hello"));
}
