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
    let buffers = (
        layer.underline_buffer.clone(),
        layer.strikethrough_buffer.clone(),
        layer.overline_buffer.clone(),
    );
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
        if rows * cols <= 32 {
            assert_eq!(
                layer.underline_buffer, buffers.0,
                "equal/smaller count reuses capacity"
            );
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
    let valid_buffer = layer.underline_buffer.clone();
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
        assert_eq!(layer.underline_buffer, valid_buffer);
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
    assert_eq!(layer.underline_buffer, valid_buffer);
    layer.prepare_with_dirty(gpu, &update.snapshot, &[], &view);
    compare(&layer, &update.snapshot, &view);
    eprintln!(
        "PASS: T0002 decoration GPU dirty replay, stride/capacity, viewport identity, invalid ranges/targets and retained recovery"
    );
}
