use super::*;

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn gpu_modern_underline_styles_readback_and_incremental_reprojection() {
    let (device, queue) = crate::test_gpu::device();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let pipeline = Arc::new(crate::render::gpu::create_colored_quad_pipeline(
        &device,
        gpu.format(),
        "T0001 pipeline",
    ));
    let mut screen = Screen::new(6, 10);
    let mut parser = TerminalParser::default();
    for style in 1..=5 {
        parser.put_bytes(
            &mut screen,
            format!("\x1b[{style};1H\x1b[4:{style};58;2;255;0;0mA 界      ").as_bytes(),
        );
    }
    let snap = screen.terminal_snapshot();
    let mut decoration = Decoration::new(
        gpu,
        pipeline.clone(),
        (256, 192),
        &snap,
        metrics(),
        Palette::default(),
    );
    decoration.prepare_with_dirty(gpu, &snap, &snap.dirty_ranges, &viewport());
    let pixels = raster(&device, &queue, &decoration);
    let mut masks = Vec::new();
    for row in 0..5 {
        let mut mask = Vec::new();
        for y in row * 24..(row + 1) * 24 {
            for x in 0..128 {
                let p = &pixels[(y * 256 + x) * 4..(y * 256 + x) * 4 + 4];
                assert!(p == [0, 0, 0, 0] || p == [255, 0, 0, 255]);
                mask.push(p[3]);
            }
        }
        assert!(mask.iter().any(|&a| a != 0), "row {row} rasterized");
        masks.push(mask);
    }
    for a in 0..5 {
        for b in a + 1..5 {
            assert_ne!(masks[a], masks[b], "styles distinguishable in GPU pixels");
        }
    }
    for y in 0..192 {
        for x in 128..256 {
            assert_eq!(pixels[(y * 256 + x) * 4 + 3], 0, "scissor clipping");
        }
    }

    // Verify default/effective color and explicit color under inverse in pixels,
    // not only in semantic cells or CPU vertices.
    let inverse = snapshot("\x1b[4:3;31;44;7mA\x1b[58;2;255;0;0mB\x1b[59mC", 1, 3);
    let mut inverse_layer = Decoration::new(
        gpu,
        pipeline.clone(),
        (256, 192),
        &inverse,
        metrics(),
        Palette::default(),
    );
    inverse_layer.invalidate_projection();
    inverse_layer.prepare_with_dirty(gpu, &inverse, &[], &viewport());
    let inverse_pixels = raster(&device, &queue, &inverse_layer);
    let blue = Palette::default()
        .resolve(Color::Named(4))
        .map(|v| (v * 255.0).round() as u8);
    for (col, expected) in [(0, blue), (1, [255, 0, 0, 255]), (2, blue)] {
        let mut painted = false;
        for y in 0..24 {
            for x in col * 16..(col + 1) * 16 {
                let pixel = &inverse_pixels[(y * 256 + x) * 4..(y * 256 + x) * 4 + 4];
                if pixel[3] != 0 {
                    painted = true;
                    assert_eq!(pixel, expected, "inverse underline color in cell {col}");
                }
            }
        }
        assert!(painted);
    }

    // Exercise a tiny dirty-range update in a retained buffer, then compare a full rebuild.
    screen.clear_dirty();
    parser.put_bytes(&mut screen, b"\x1b[1;2H\x1b[4:5;58;5;2mX");
    let changed = screen.terminal_snapshot();
    decoration.prepare_with_dirty(gpu, &changed, &changed.dirty_ranges, &viewport());
    let incremental = raster(&device, &queue, &decoration);
    let mut full = Decoration::new(
        gpu,
        pipeline.clone(),
        (256, 192),
        &changed,
        metrics(),
        Palette::default(),
    );
    full.invalidate_projection();
    full.prepare_with_dirty(gpu, &changed, &[], &viewport());
    assert_eq!(incremental, raster(&device, &queue, &full));

    // Narrow/widen and invalidate viewport + palette projection.
    for cols in [5, 12] {
        screen.resize(6, cols);
        let snap = screen.terminal_snapshot();
        let mut view = viewport();
        view.cell_width *= 1.5;
        view.line_height *= 1.5;
        view.allocation_origin = (3.0, 5.0);
        let mut palette = Palette::default();
        palette.normal[2] = harbor_config::Rgba::new(0.0, 0.0, 1.0, 1.0);
        decoration.set_palette(palette);
        decoration.invalidate_projection();
        decoration.prepare_with_dirty(gpu, &snap, &[], &view);
        let retained = raster(&device, &queue, &decoration);
        let mut rebuilt =
            Decoration::new(gpu, pipeline.clone(), (256, 192), &snap, metrics(), palette);
        rebuilt.invalidate_projection();
        rebuilt.prepare_with_dirty(gpu, &snap, &[], &view);
        assert_eq!(retained, raster(&device, &queue, &rebuilt));
    }
    // Font metrics belong to a layer's lifetime. A recreated layer must project
    // the same semantic snapshot at the new underline position and thickness.
    let changed_metrics = TextMetrics {
        underline_position: 7.0,
        underline_thickness: 2.0,
        ..metrics()
    };
    let snap = screen.terminal_snapshot();
    let mut old_font = Decoration::new(
        gpu,
        pipeline.clone(),
        (256, 192),
        &snap,
        metrics(),
        Palette::default(),
    );
    let mut new_font = Decoration::new(
        gpu,
        pipeline.clone(),
        (256, 192),
        &snap,
        changed_metrics,
        Palette::default(),
    );
    old_font.invalidate_projection();
    new_font.invalidate_projection();
    old_font.prepare_with_dirty(gpu, &snap, &[], &viewport());
    new_font.prepare_with_dirty(gpu, &snap, &[], &viewport());
    assert_ne!(
        raster(&device, &queue, &old_font),
        raster(&device, &queue, &new_font)
    );

    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    eprintln!(
        "PASS: T0001 five distinct GPU styles, spaces/wide slots, exact color, scissor, incremental/full equivalence, resize/DPI/palette invalidation"
    );
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn gpu_conceal_overline_readback_dirty_removal_and_reprojection() {
    let (device, queue) = crate::test_gpu::device();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let pipeline = Arc::new(crate::render::gpu::create_colored_quad_pipeline(
        &device,
        gpu.format(),
        "T0002",
    ));
    let mut screen = Screen::new(4, 10);
    let mut parser = TerminalParser::default();
    parser.put_bytes(
        &mut screen,
        "\x1b[53;38;2;255;0;0mA 界      \x1b[0m\x1b[2;1H\x1b]8;;https://example.test\x1b\\\x1b[8;4:3;58;2;255;0;0;9;53;7mAe\u{301}界\x1b[3;1H\x1b[0;53;31mA      ".as_bytes(),
    );
    let snap = screen.terminal_snapshot();
    let mut decoration = Decoration::new(
        gpu,
        pipeline.clone(),
        (256, 192),
        &snap,
        metrics(),
        Palette::default(),
    );
    decoration.invalidate_projection();
    decoration.prepare_with_dirty(gpu, &snap, &[], &viewport());
    let pixels = raster(&device, &queue, &decoration);
    for x in 0..128 {
        assert_eq!(
            &pixels[x * 4..x * 4 + 4],
            &[255, 0, 0, 255],
            "continuous overline, including blank/wide slots"
        );
    }
    for y in 24..48 {
        for x in 0..128 {
            assert_eq!(
                pixels[(y * 256 + x) * 4 + 3],
                0,
                "concealed row has no decorations"
            );
        }
    }
    for y in 0..192 {
        for x in 128..256 {
            assert_eq!(
                pixels[(y * 256 + x) * 4 + 3],
                0,
                "scissor clips the overlay"
            );
        }
    }
    screen.clear_dirty();
    parser.put_bytes(&mut screen, b"\x1b[1;1H\x1b[8;53mA\x1b[55;28m ");
    let snap = screen.terminal_snapshot();
    decoration.prepare_with_dirty(gpu, &snap, &snap.dirty_ranges, &viewport());
    let incremental = raster(&device, &queue, &decoration);
    assert_eq!(incremental[3], 0, "conceal removes stale overline");
    assert_eq!(incremental[16 * 4 + 3], 0, "55 removes stale overline");
    let mut full = Decoration::new(
        gpu,
        pipeline.clone(),
        (256, 192),
        &snap,
        metrics(),
        Palette::default(),
    );
    full.invalidate_projection();
    full.prepare_with_dirty(gpu, &snap, &[], &viewport());
    assert_eq!(incremental, raster(&device, &queue, &full));
    for cols in [5, 12] {
        screen.resize(4, cols);
        let snap = screen.terminal_snapshot();
        let mut view = viewport();
        view.cell_width *= 1.5;
        view.line_height *= 1.5;
        view.allocation_origin = (3.0, 5.0);
        let mut palette = Palette::default();
        palette.normal[1] = harbor_config::Rgba::new(0.0, 1.0, 0.0, 1.0);
        decoration.set_palette(palette);
        decoration.invalidate_projection();
        decoration.prepare_with_dirty(gpu, &snap, &[], &view);
        let retained = raster(&device, &queue, &decoration);
        assert!(
            retained.as_chunks::<4>().0.contains(&[0, 255, 0, 255]),
            "visible indexed overline must use the updated active palette"
        );
        let mut full =
            Decoration::new(gpu, pipeline.clone(), (256, 192), &snap, metrics(), palette);
        full.invalidate_projection();
        full.prepare_with_dirty(gpu, &snap, &[], &view);
        assert_eq!(retained, raster(&device, &queue, &full));
    }
    // Actual text atlas/encode/readback, including suffix and isolated-mark overlays.
    let fonts = harbor_text::load_system_fonts(&harbor_config::FontSettings::default()).unwrap();
    let font_metrics = TextMetrics::from_font_metrics(fonts.font_metrics());
    let view = RenderViewport {
        padding: 0.0,
        ..RenderViewport::with_surface(
            font_metrics.cell_width,
            font_metrics.line_height,
            (256, 192),
            (256, 192),
        )
    };
    let mut text_screen = Screen::new(2, 10);
    let mut text_parser = TerminalParser::default();
    text_parser.put_bytes(
        &mut text_screen,
        "\x1b[8;7;44mAe\u{301}界\x1b[2;1H\u{301}".as_bytes(),
    );
    let hidden = text_screen.terminal_snapshot();
    let mut text_layer = crate::render::text::Text::new(
        gpu,
        fonts,
        font_metrics,
        &hidden,
        &view,
        Palette::default(),
    )
    .unwrap();
    text_layer.prepare_with_dirty(gpu, &hidden, &hidden.dirty_ranges, &view, None);
    text_layer.prepare_preedit(gpu, None, &hidden, &view);
    let pixels = raster_draw(&device, &queue, |pass| text_layer.draw(pass));
    assert!(
        pixels.iter().all(|&x| x == 0),
        "no concealed base, suffix or dotted-circle glyphs, even under inverse"
    );
    text_screen.clear_dirty();
    text_parser.put_bytes(
        &mut text_screen,
        "\x1b[1;1H\x1b[28;27;49mAe\u{301}界".as_bytes(),
    );
    let visible = text_screen.terminal_snapshot();
    text_layer.prepare_with_dirty(gpu, &visible, &visible.dirty_ranges, &view, None);
    text_layer.prepare_preedit(gpu, None, &visible, &view);
    let pixels = raster_draw(&device, &queue, |pass| text_layer.draw(pass));
    assert!(
        pixels.as_chunks::<4>().0.iter().any(|p| p[3] != 0),
        "reveal can populate an initially empty atlas"
    );
    text_screen.clear_dirty();
    text_parser.put_bytes(&mut text_screen, "\x1b[1;1H\x1b[8;7mAe\u{301}界".as_bytes());
    let hidden_again = text_screen.terminal_snapshot();
    text_layer.prepare_with_dirty(gpu, &hidden_again, &hidden_again.dirty_ranges, &view, None);
    text_layer.prepare_preedit(gpu, None, &hidden_again, &view);
    assert!(
        raster_draw(&device, &queue, |pass| text_layer.draw(pass))
            .iter()
            .all(|&x| x == 0),
        "dirty conceal clears retained base and suffix glyphs"
    );
    eprintln!(
        "PASS: T0002 GPU overline/space/wide/color/scissor, dirty conceal/off removal, resize/palette/DPI reprojection, base/suffix/isolated glyph suppression and reveal"
    );
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn gpu_osc_palette_retained_layers_recolor_and_replay() {
    use crate::{Terminal, TerminalAppearance};
    use std::time::Instant;
    let (device, queue) = crate::test_gpu::device();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let fonts = harbor_text::load_system_fonts(&harbor_config::FontSettings::default()).unwrap();
    let mut terminal = Terminal::new_headless_with_appearance(
        6,
        8,
        TerminalAppearance::from_palette(Palette {
            background: harbor_config::Rgba::from_rgb8(0, 0, 0),
            ..Palette::default()
        }),
    );
    terminal
        .put_bytes(b"\x1b[?25l\x1b]4;42;#ff0000;43;#00ff00;44;#0000ff;1;#ffff00;15;#00ffff\x07");
    terminal.put_bytes(b"\x1b[38;5;42mXXX\x1b[2;1H\x1b[0;48;5;43m   \x1b[3;1H\x1b[0;4;58;5;44m   \x1b[4;1H\x1b[0;31mXXX\x1b[5;1H\x1b[0;107m   \x1b[6;1H\x1b[0;48;2;17;34;51m   ");
    let now = Instant::now();
    let first = terminal.read_update(now);
    let mut renderer = crate::render::pipeline::TerminalRenderPipeline::new(
        gpu,
        (256, 192),
        fonts,
        metrics(),
        &first,
    )
    .unwrap();
    renderer.sync_viewport(viewport(), false);
    renderer.prepare(gpu, &first, true);
    assert!(terminal.acknowledge_update(&first));
    let before = raster_draw(&device, &queue, |pass| renderer.draw(pass));
    let row_pixels = |pixels: &[u8], row: usize| -> Vec<[u8; 4]> {
        (row * 24..(row + 1) * 24)
            .flat_map(|y| {
                (0..48).map(move |x| {
                    pixels[(y * 256 + x) * 4..(y * 256 + x) * 4 + 4]
                        .try_into()
                        .unwrap()
                })
            })
            .collect()
    };
    for (row, color) in [
        (0, [255, 0, 0, 255]),
        (1, [0, 255, 0, 255]),
        (2, [0, 0, 255, 255]),
        (3, [255, 255, 0, 255]),
        (4, [0, 255, 255, 255]),
        (5, [17, 34, 51, 255]),
    ] {
        assert!(
            row_pixels(&before, row).contains(&color),
            "initial row {row}"
        );
    }
    let old = terminal.read_update(now);
    // Read while hidden/skipped: neither reading nor skipping consumes palette damage.
    terminal
        .put_bytes(b"\x1b[?2026h\x1b]4;42;#ff00ff;43;#ffaa00;44;#ffffff;1;#8800ff;15;#0088ff\x07");
    let skipped = terminal.read_update(now);
    assert!(!skipped.frame_demand.ordinary_present_eligible);
    assert!(!terminal.acknowledge_update(&old));
    terminal.put_bytes(b"\x1b[?2026l");
    let updated = terminal.read_update(now);
    assert_eq!(first.snapshot.cells, updated.snapshot.cells);
    renderer.sync_palette(updated.appearance.palette());
    assert!(renderer.background.is_dirty());
    assert!(renderer.text.is_dirty());
    assert!(renderer.decoration.is_dirty());
    renderer.prepare(gpu, &updated, true);
    assert!(terminal.acknowledge_update(&updated));
    let after = raster_draw(&device, &queue, |pass| renderer.draw(pass));
    for (row, color) in [
        (0, [255, 0, 255, 255]),
        (1, [255, 170, 0, 255]),
        (2, [255, 255, 255, 255]),
        (3, [136, 0, 255, 255]),
        (4, [0, 136, 255, 255]),
    ] {
        let pixels = row_pixels(&after, row);
        assert!(pixels.contains(&color), "recolored row {row}");
        assert_ne!(pixels, row_pixels(&before, row), "retained row {row}");
    }
    assert_eq!(
        row_pixels(&after, 5),
        row_pixels(&before, 5),
        "truecolor unchanged"
    );
    terminal.put_bytes(b"\x1b]104\x07");
    let reset = terminal.read_update(now);
    renderer.prepare(gpu, &reset, true);
    assert!(terminal.acknowledge_update(&reset));
    let reset_pixels = raster_draw(&device, &queue, |pass| renderer.draw(pass));
    assert_ne!(row_pixels(&reset_pixels, 1), row_pixels(&after, 1));
    assert_eq!(row_pixels(&reset_pixels, 5), row_pixels(&before, 5));
    eprintln!(
        "PASS: OSC palette retained foreground/background/underline, ANSI aliases, unchanged semantic cells/truecolor, skipped-frame replay, stale acknowledgement and reset readback"
    );
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn t0002_gpu_retained_edits_stride_projection_and_invalid_recovery_match_full() {
    let (device, queue) = crate::render::text::sequence_tests::test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let pipeline = Arc::new(crate::render::gpu::create_colored_quad_pipeline(
        &device,
        gpu.format(),
        "T0002 decoration",
    ));
    let mut engine = crate::Terminal::new_headless(4, 8);
    engine.put_str("\x1b[4:3;9;53;31mA 界 ABC\r\nABCDEFG\r\nABCDEFG");
    let initial = engine.read_update(std::time::Instant::now());
    let mut layer = Decoration::new(
        gpu,
        pipeline.clone(),
        (256, 192),
        &initial.snapshot,
        metrics(),
        Palette::default(),
    );
    let mut view = viewport();
    layer.prepare_with_dirty(
        gpu,
        &initial.snapshot,
        &initial.snapshot.dirty_ranges,
        &view,
    );
    assert!(raster(&device, &queue, &layer).iter().any(|&v| v != 0));
    assert!(engine.acknowledge_update(&initial));
    // Skipped/hidden output is read but not acknowledged; both edits remain pending.
    engine.put_str("\x1b[1;1H\x1b[8mX");
    let skipped = engine.read_update(std::time::Instant::now());
    engine.put_str("\x1b[1;2H\x1b[28;4:5;58;2;0;255;0mY");
    let changed = engine.read_update(std::time::Instant::now());
    assert!(!changed.snapshot.dirty_ranges.is_empty());
    assert!(!engine.acknowledge_update(&skipped));
    layer.prepare_with_dirty(
        gpu,
        &changed.snapshot,
        &changed.snapshot.dirty_ranges,
        &view,
    );
    let compare = |layer: &Decoration, snap: &TerminalSnapshot, view: &RenderViewport| {
        let mut full = Decoration::new(
            gpu,
            pipeline.clone(),
            (256, 192),
            snap,
            metrics(),
            Palette::default(),
        );
        full.prepare_with_dirty(gpu, snap, &[], view);
        assert_eq!(
            raster(&device, &queue, layer),
            raster(&device, &queue, &full)
        );
    };
    compare(&layer, &changed.snapshot, &view);
    assert!(engine.acknowledge_update(&changed));
    for (rows, cols) in [(8, 4), (4, 4), (3, 8), (4, 8), (5, 9)] {
        engine.resize(rows, cols);
        let update = engine.read_update(std::time::Instant::now());
        layer.prepare_with_dirty(gpu, &update.snapshot, &[], &view);
        compare(&layer, &update.snapshot, &view);
        for storage in [&layer.underline, &layer.strikethrough, &layer.overline] {
            let (d, g, c) = storage.storage_bytes();
            assert!(g <= 4 * d && c <= 4 * d);
        }
        assert!(engine.acknowledge_update(&update));
        engine.put_str("\x1b[1;2H\x1b[24;29;55mQ");
        let edit = engine.read_update(std::time::Instant::now());
        layer.prepare_with_dirty(gpu, &edit.snapshot, &edit.snapshot.dirty_ranges, &view);
        compare(&layer, &edit.snapshot, &view);
        assert!(engine.acknowledge_update(&edit));
    }
    let update = engine.read_update(std::time::Instant::now());
    for (cell_width, line_height, origin, surface) in [
        (12.75, 21.25, (3.35, 4.45), (256, 192)),
        (10.5, 19.5, (1.25, 2.75), (240, 180)),
        (16.0, 24.0, (0.0, 0.0), (256, 192)),
    ] {
        view.cell_width = cell_width;
        view.line_height = line_height;
        view.allocation_origin = origin;
        view.surface_size = surface;
        view.allocation_size = (200, 150);
        // No explicit invalidation and no engine damage: identity must force reprojection.
        layer.prepare_with_dirty(gpu, &update.snapshot, &[], &view);
        compare(&layer, &update.snapshot, &view);
    }
    for range in [
        DirtyRange {
            row: usize::MAX,
            start_col: 0,
            end_col: 1,
        },
        DirtyRange {
            row: 0,
            start_col: 4,
            end_col: 2,
        },
        DirtyRange {
            row: 0,
            start_col: 0,
            end_col: usize::MAX,
        },
    ] {
        layer.prepare_with_dirty(gpu, &update.snapshot, &[range], &view);
        compare(&layer, &update.snapshot, &view);
    }
    let valid_buffer = layer.underline.buffers();
    for mut invalid in [
        RenderViewport {
            line_height: f32::NAN,
            ..view
        },
        RenderViewport {
            allocation_size: (0, 192),
            ..view
        },
        RenderViewport {
            surface_size: (256, 0),
            ..view
        },
    ] {
        layer.prepare_with_dirty(gpu, &update.snapshot, &[], &invalid);
        assert!(layer.is_dirty());
        assert_eq!(layer.underline.buffers(), valid_buffer);
        assert!(raster(&device, &queue, &layer).iter().all(|&v| v == 0));
        invalid = view;
        layer.prepare_with_dirty(gpu, &update.snapshot, &[], &invalid);
        assert!(!layer.is_dirty());
        compare(&layer, &update.snapshot, &view);
    }
    let mut malformed = (*update.snapshot).clone();
    malformed.rows = usize::MAX;
    layer.prepare_with_dirty(gpu, &malformed, &[], &view);
    assert!(layer.is_dirty());
    assert_eq!(layer.underline.buffers(), valid_buffer);
    layer.prepare_with_dirty(gpu, &update.snapshot, &[], &view);
    compare(&layer, &update.snapshot, &view);
    eprintln!(
        "PASS: T0002 decoration GPU dirty replay, stride/capacity, viewport identity, invalid ranges/targets and retained recovery"
    );
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn gpu_decoration_drawability_survives_invalidation_and_recovers_from_rejection() {
    let (device, queue) = crate::test_gpu::device();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let pipeline = Arc::new(crate::render::gpu::create_colored_quad_pipeline(
        &device,
        gpu.format(),
        "decoration drawability transitions",
    ));
    let view = RenderViewport::with_surface(16.0, 24.0, (256, 192), (256, 192));
    let snap = snapshot("\x1b[4:3;9;53;31mA B", 2, 4);
    let mut palette = Palette::default();
    let fresh = |snap: &TerminalSnapshot, palette: Palette| {
        Decoration::new(gpu, pipeline.clone(), (256, 192), snap, metrics(), palette)
    };
    let mut layer = fresh(&snap, palette);
    let original = raster(&device, &queue, &layer);
    assert!(original.iter().any(|&v| v != 0));
    assert!(!layer.is_dirty());

    // Invalidating or changing palette marks a future rebuild, but the last
    // successful projection is still drawable until a preparation is rejected.
    layer.invalidate_projection();
    assert!(layer.is_dirty());
    assert_eq!(raster(&device, &queue, &layer), original);
    palette.normal[1] = harbor_config::Rgba::from_rgb8(0, 255, 0);
    layer.set_palette(palette);
    layer.prepare(gpu, None, &view);
    assert!(layer.is_dirty());
    assert_eq!(raster(&device, &queue, &layer), original);
    layer.prepare_with_dirty(gpu, &snap, &[], &view);
    let recolored = raster(&device, &queue, &layer);
    assert_ne!(recolored, original);
    assert_eq!(recolored, raster(&device, &queue, &fresh(&snap, palette)));
    assert!(!layer.is_dirty());

    let retained = (
        layer.underline.buffers(),
        layer.strikethrough.buffers(),
        layer.overline.buffers(),
    );
    let invalid_view = RenderViewport {
        line_height: f32::NAN,
        ..view
    };
    layer.prepare_with_dirty(gpu, &snap, &[], &invalid_view);
    assert!(layer.is_dirty());
    assert!(raster(&device, &queue, &layer).iter().all(|&v| v == 0));
    layer.prepare(gpu, None, &view);
    layer.invalidate_projection();
    layer.set_palette(palette);
    assert!(layer.is_dirty());
    assert!(raster(&device, &queue, &layer).iter().all(|&v| v == 0));
    layer.prepare_with_dirty(gpu, &snap, &[], &view);
    assert!(!layer.is_dirty());
    assert_eq!(raster(&device, &queue, &layer), recolored);
    assert_eq!(layer.underline.buffers(), retained.0);
    assert_eq!(layer.strikethrough.buffers(), retained.1);
    assert_eq!(layer.overline.buffers(), retained.2);

    // Empty and malformed snapshots also reject drawing without throwing away
    // retained buffers; a no-damage valid retry must rebuild and recover.
    let mut invalid = snap.clone();
    for (rows, cols) in [(0, 4), (2, 0), (usize::MAX, 2), (3, 4)] {
        invalid.rows = rows;
        invalid.cols = cols;
        layer.prepare_with_dirty(gpu, &invalid, &[], &view);
        assert!(layer.is_dirty(), "rejected {rows}x{cols}");
        assert!(raster(&device, &queue, &layer).iter().all(|&v| v == 0));
        layer.prepare_with_dirty(gpu, &snap, &[], &view);
        assert!(!layer.is_dirty());
        assert_eq!(raster(&device, &queue, &layer), recolored);
        assert_eq!(layer.underline.buffers(), retained.0);
    }
    invalid.rows = 0;
    let mut initially_invalid = fresh(&invalid, palette);
    assert!(initially_invalid.is_dirty());
    assert!(
        raster(&device, &queue, &initially_invalid)
            .iter()
            .all(|&v| v == 0)
    );
    initially_invalid.prepare_with_dirty(gpu, &snap, &[], &view);
    assert!(!initially_invalid.is_dirty());
    assert_eq!(raster(&device, &queue, &initially_invalid), recolored);

    let invalid_metrics = TextMetrics {
        underline_thickness: 0.0,
        ..metrics()
    };
    let mut invalid_font = Decoration::new(
        gpu,
        pipeline.clone(),
        (256, 192),
        &snap,
        invalid_metrics,
        palette,
    );
    invalid_font.prepare_with_dirty(gpu, &snap, &[], &view);
    assert!(invalid_font.is_dirty());
    assert!(
        raster(&device, &queue, &invalid_font)
            .iter()
            .all(|&v| v == 0)
    );
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn gpu_background_initial_resize_and_retained_rebuild_match_initialized_reference() {
    use crate::render::Background;

    let (device, queue) = crate::test_gpu::device();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let pipeline = Arc::new(crate::render::gpu::create_colored_quad_pipeline(
        &device,
        gpu.format(),
        "background initialized reference",
    ));
    let view = RenderViewport::with_surface(16.0, 24.0, (256, 192), (256, 192));
    let tint = [0.2, 0.3, 0.4, 0.5];
    let mut palette = Palette::default();
    palette.normal[1] = harbor_config::Rgba::from_rgb8(255, 0, 0);
    let reference = |snap: &TerminalSnapshot, palette: &Palette| {
        let vertices: Vec<_> = (0..snap.rows)
            .flat_map(|row| {
                Background::build_background_row_vertices_with_palette(row, snap, &view, palette)
            })
            .collect();
        let full_rect = Background::build_full_rect_vertices(&view, tint);
        let cell_buffer = crate::render::gpu::create_colored_vertex_buffer(&device, &vertices);
        let rect_buffer = crate::render::gpu::create_colored_vertex_buffer(&device, &full_rect);
        raster_draw(&device, &queue, |pass| {
            pass.set_pipeline(&pipeline);
            pass.set_vertex_buffer(0, rect_buffer.slice(..));
            pass.draw(0..6, 0..1);
            pass.set_vertex_buffer(0, cell_buffer.slice(..));
            if !vertices.is_empty() {
                pass.draw(0..vertices.len() as u32, 0..1);
            }
        })
    };
    let initial = snapshot("\x1b[41mA\x1b[0m B\x1b[7mC", 2, 4);
    let mut layer = Background::new(
        gpu,
        pipeline.clone(),
        (256, 192),
        &initial,
        16.0,
        24.0,
        tint,
        palette,
    );
    let original = raster_draw(&device, &queue, |pass| layer.draw(pass));
    assert_eq!(original, reference(&initial, &palette));
    assert!(original.as_chunks::<4>().0.contains(&[255, 0, 0, 255]));

    layer.invalidate_projection();
    palette.normal[1] = harbor_config::Rgba::from_rgb8(0, 255, 0);
    layer.set_palette(palette);
    layer.prepare(gpu, None, &view, tint);
    assert!(layer.is_dirty());
    assert_eq!(
        raster_draw(&device, &queue, |pass| layer.draw(pass)),
        original
    );
    layer.prepare_with_dirty(gpu, &initial, &[], &view, tint);
    assert!(!layer.is_dirty());
    assert_eq!(
        raster_draw(&device, &queue, |pass| layer.draw(pass)),
        reference(&initial, &palette)
    );

    for (rows, cols) in [(4, 7), (2, 2), (3, 5), (1, 3)] {
        let snap = snapshot("\x1b[41mA\x1b[0m \x1b[44mB", rows, cols);
        layer.prepare_with_dirty(gpu, &snap, &[], &view, tint);
        let pixels = raster_draw(&device, &queue, |pass| layer.draw(pass));
        assert_eq!(pixels, reference(&snap, &palette), "retained {rows}x{cols}");
        let fresh = Background::new(
            gpu,
            pipeline.clone(),
            (256, 192),
            &snap,
            16.0,
            24.0,
            tint,
            palette,
        );
        assert_eq!(
            pixels,
            raster_draw(&device, &queue, |pass| fresh.draw(pass))
        );
    }

    let mut empty = initial.clone();
    empty.rows = 0;
    layer.prepare_with_dirty(gpu, &empty, &[], &view, tint);
    assert_eq!(
        raster_draw(&device, &queue, |pass| layer.draw(pass)),
        reference(&empty, &palette)
    );
    let empty_fresh = Background::new(
        gpu,
        pipeline.clone(),
        (256, 192),
        &empty,
        16.0,
        24.0,
        tint,
        palette,
    );
    assert_eq!(
        raster_draw(&device, &queue, |pass| empty_fresh.draw(pass)),
        reference(&empty, &palette)
    );
    layer.prepare_with_dirty(gpu, &initial, &[], &view, tint);
    assert_eq!(
        raster_draw(&device, &queue, |pass| layer.draw(pass)),
        reference(&initial, &palette)
    );
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn gpu_background_oversize_initialized_buffer_has_distinct_scoped_failure() {
    use crate::render::Background;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("explicit GPU test requires an available adapter");
    let limits = wgpu::Limits {
        max_buffer_size: 4096,
        ..Default::default()
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_limits: limits,
        ..Default::default()
    }))
    .expect("explicit GPU test requires a usable device");
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let pipeline = Arc::new(crate::render::gpu::create_colored_quad_pipeline(
        &device,
        gpu.format(),
        "background scoped size failure",
    ));
    let snap = snapshot("\x1b[41mA", 1, 32);
    let size = (snap.rows * snap.cols * 6 * std::mem::size_of::<ColoredVertex>()) as u64;
    assert!(size > device.limits().max_buffer_size);

    // Background's public constructor does not preflight this limit. The old
    // initialized helper maps immediately; a sized allocation does not, even
    // though both report the same validation failure to the enclosing scope.
    let initialized_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let initialized = catch_unwind(AssertUnwindSafe(|| {
        Background::new(
            gpu,
            pipeline,
            (256, 192),
            &snap,
            16.0,
            24.0,
            [0.0; 4],
            Palette::default(),
        )
    }));
    assert!(pollster::block_on(initialized_scope.pop()).is_some());
    assert!(initialized.is_err());

    let sized_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let sized = catch_unwind(AssertUnwindSafe(|| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("terminal vertex buffer"),
            size,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }));
    assert!(pollster::block_on(sized_scope.pop()).is_some());
    assert!(sized.is_ok());
}

#[test]
#[ignore = "gpu: demand-bounded decoration capacities and dense-reference readback"]
fn gpu_demand_bounded_decorations_reclaim_each_layer_and_preserve_local_rows() {
    let (device, queue) = crate::test_gpu::device();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let pipeline = Arc::new(crate::render::gpu::create_colored_quad_pipeline(
        &device,
        gpu.format(),
        "demand bounded decorations",
    ));
    let view = viewport();
    let palette = Palette::default();
    let initial = snapshot("", 8, 16);
    let mut layer = Decoration::new(
        gpu,
        pipeline.clone(),
        (256, 192),
        &initial,
        metrics(),
        palette,
    );
    let compare_dense = |layer: &Decoration, snap: &TerminalSnapshot| {
        let geometry = [
            build_underline_vertices(&metrics(), snap, &view, &palette),
            build_strikethrough_vertices(&metrics(), snap, &view, &palette),
            build_overline_vertices(&metrics(), snap, &view, &palette),
        ];
        let buffers: Vec<_> = geometry
            .iter()
            .map(|v| crate::render::gpu::create_colored_vertex_buffer(&device, v))
            .collect();
        assert_eq!(
            raster(&device, &queue, layer),
            raster_draw(&device, &queue, |p| {
                p.set_pipeline(&pipeline);
                for (v, buffer) in geometry.iter().zip(&buffers) {
                    p.set_vertex_buffer(0, buffer.slice(..));
                    p.draw(0..v.len() as u32, 0..1);
                }
            })
        );
    };
    let mut sparse_bytes = None;
    for (rows, cols) in [(4, 8), (32, 64)] {
        let empty = snapshot("", rows, cols);
        layer.prepare_with_dirty(gpu, &empty, &[], &view);
        for storage in [&layer.underline, &layer.strikethrough, &layer.overline] {
            assert_eq!(storage.storage_bytes(), (0, 0, 0));
        }
        let sparse = snapshot("\x1b[4:3;9;53mA", rows, cols);
        layer.prepare_with_dirty(
            gpu,
            &sparse,
            &[DirtyRange {
                row: 0,
                start_col: 0,
                end_col: 1,
            }],
            &view,
        );
        let bytes = [
            layer.underline.storage_bytes(),
            layer.strikethrough.storage_bytes(),
            layer.overline.storage_bytes(),
        ];
        assert_eq!(
            *sparse_bytes.get_or_insert(bytes),
            bytes,
            "grid-independent geometry"
        );
        compare_dense(&layer, &sparse);
    }

    for _ in 0..3 {
        let mut dense = snapshot("", 8, 16);
        let decorated = snapshot("\x1b[4:3;9;53mA", 1, 1).cells[0].clone();
        for cell in dense.cells.iter_mut() {
            *cell = decorated.clone();
        }
        layer.prepare_with_dirty(gpu, &dense, &dense.dirty_ranges, &view);
        compare_dense(&layer, &dense);
        let previous = [
            layer.underline.storage_bytes(),
            layer.strikethrough.storage_bytes(),
            layer.overline.storage_bytes(),
        ];
        let ranges: Vec<_> = (0..8)
            .map(|row| DirtyRange {
                row,
                start_col: 0,
                end_col: 16,
            })
            .collect();
        let mut sparse = snapshot("\x1b[4:3;9;53mA", 8, 16);
        layer.prepare_with_dirty(gpu, &sparse, &ranges, &view);
        for (storage, (_, old_g, old_c)) in
            [&layer.underline, &layer.strikethrough, &layer.overline]
                .into_iter()
                .zip(previous)
        {
            let (d, g, c) = storage.storage_bytes();
            assert!(d > 0 && old_g > 4 * d && old_c > 4 * d);
            assert!(g <= 2 * d && c <= 2 * d);
            eprintln!(
                "decoration old_gpu_cpu=({old_g},{old_c}) sparse_demand_gpu_cpu=({d},{g},{c})"
            );
        }
        compare_dense(&layer, &sparse);
        assert_eq!(
            crate::render::observations::buffer_write_bytes(|| {
                layer.prepare_with_dirty(gpu, &sparse, &[], &view);
            }),
            0
        );
        sparse.cells[16] = decorated.clone();
        let unchanged = [
            layer.underline.buffers()[0].clone(),
            layer.strikethrough.buffers()[0].clone(),
            layer.overline.buffers()[0].clone(),
        ];
        let bytes = crate::render::observations::buffer_write_bytes(|| {
            layer.prepare_with_dirty(
                gpu,
                &sparse,
                &[DirtyRange {
                    row: 1,
                    start_col: 0,
                    end_col: 1,
                }],
                &view,
            );
        });
        let local_demand = layer.underline.storage_bytes().0
            + layer.strikethrough.storage_bytes().0
            + layer.overline.storage_bytes().0;
        assert_eq!(bytes * 2, local_demand, "only the changed row is uploaded");
        assert_eq!(
            [
                layer.underline.buffers()[0].clone(),
                layer.strikethrough.buffers()[0].clone(),
                layer.overline.buffers()[0].clone()
            ],
            unchanged
        );
        compare_dense(&layer, &sparse);
        for (rows, cols) in [(16, 8), (2, 4), (8, 16)] {
            let resized = snapshot("\x1b[4:3;9;53mA", rows, cols);
            layer.prepare_with_dirty(gpu, &resized, &[], &view);
            for storage in [&layer.underline, &layer.strikethrough, &layer.overline] {
                let (d, g, c) = storage.storage_bytes();
                assert!(g <= 4 * d && c <= 4 * d);
            }
            compare_dense(&layer, &resized);
        }
        let empty = snapshot("", 8, 16);
        layer.prepare_with_dirty(gpu, &empty, &ranges, &view);
        for storage in [&layer.underline, &layer.strikethrough, &layer.overline] {
            assert_eq!(storage.storage_bytes(), (0, 0, 0));
        }
        compare_dense(&layer, &empty);
    }
}
