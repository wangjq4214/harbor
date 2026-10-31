use super::*;

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn gpu_repertoire_sharp_pixels_and_fractional_horizontal_vertical_junction_joins() {
    let (device, queue) = test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let pipeline = Arc::new(crate::render::gpu::create_colored_quad_pipeline(
        &device,
        gpu.format(),
        "box test",
    ));
    let mut snap = snapshot("", 8, 16);
    for (i, c) in snap.cells.iter_mut().enumerate() {
        *c = cell(char::from_u32(0x2500 + i as u32).unwrap());
    }
    let view = RenderViewport {
        cell_width: 14.0,
        line_height: 24.0,
        ..viewport()
    };
    let pixels = rebuilt_pixels(gpu, &pipeline, &snap, &view, Palette::default());
    assert!(
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| *p == [0, 0, 0, 0] || *p == [255, 0, 0, 255]),
        "hard edged, exact high contrast color"
    );
    for i in 0..128 {
        assert!(
            (i / 16 * 24..(i / 16 + 1) * 24)
                .any(|y| (i % 16 * 14..(i % 16 + 1) * 14).any(|x| pixel(&pixels, x, y)[3] != 0)),
            "GPU U+{:04X}",
            0x2500 + i
        );
    }
    // Connected components are an independent raster oracle for junction topology:
    // solid/mixed junctions stay joined; pure double corners/tees/crosses retain
    // two/three/four separate lanes. Dash counts remain intentional.
    for i in 0..128 {
        let x0 = i % 16 * 14;
        let y0 = i / 16 * 24;
        let mask: Vec<_> = (0..24)
            .flat_map(|y| (0..14).map(move |x| (x, y)))
            .map(|(x, y)| pixel(&pixels, x0 + x, y0 + y)[3] != 0)
            .collect();
        let expected = match 0x2500 + i {
            0x2504..=0x2507 => 3,
            0x2508..=0x250b => 4,
            0x254c..=0x254f => 2,
            0x2550 | 0x2551 | 0x2554 | 0x2557 | 0x255a | 0x255d => 2,
            0x2560 | 0x2563 | 0x2566 | 0x2569 => 3,
            0x256c => 4,
            _ => 1,
        };
        assert_eq!(
            components(mask, 14, 24),
            expected,
            "GPU junction/lane topology U+{:04X}",
            0x2500 + i
        );
    }
    for (width, height, origin) in [
        (16.0, 24.0, (0.0, 0.0)),
        (9.3, 17.7, (3.35, 4.45)),
        (12.75, 21.25, (0.5, 1.5)),
    ] {
        let view = RenderViewport {
            cell_width: width,
            line_height: height,
            allocation_origin: origin,
            ..viewport()
        };
        let mut snap = snapshot("", 8, 10);
        for (row, line) in [
            "┌─┬─┐",
            "│ │ │",
            "└─┴─┘",
            "╔═╦═╗",
            "║ ║ ║",
            "╚═╩═╝",
            "┏━┳━┓",
            "┗━┻━┛",
        ]
        .iter()
        .enumerate()
        {
            for (col, ch) in line.chars().enumerate() {
                snap.cells[row * 10 + col] = cell(ch);
            }
        }
        let pixels = rebuilt_pixels(gpu, &pipeline, &snap, &view, Palette::default());
        let edge = |o: f32, size: f32, index: usize| {
            (f64::from(o) + index as f64 * f64::from(size)).round() as usize
        };
        // All four seams in the horizontal border rows have identical nonempty coverage.
        for row in [0, 2, 3, 5, 6, 7] {
            for col in 1..5 {
                let x = edge(origin.0, width, col);
                let left: Vec<_> = (edge(origin.1, height, row)..edge(origin.1, height, row + 1))
                    .map(|y| pixel(&pixels, x - 1, y)[3])
                    .collect();
                let right: Vec<_> = (edge(origin.1, height, row)..edge(origin.1, height, row + 1))
                    .map(|y| pixel(&pixels, x, y)[3])
                    .collect();
                assert_eq!(left, right, "GPU horizontal seam row {row} col {col}");
                assert!(left.iter().any(|a| *a != 0));
            }
        }
        for row in [1, 2, 4, 5] {
            for col in [0, 2, 4] {
                let y = edge(origin.1, height, row);
                let top: Vec<_> = (edge(origin.0, width, col)..edge(origin.0, width, col + 1))
                    .map(|x| pixel(&pixels, x, y - 1)[3])
                    .collect();
                let bottom: Vec<_> = (edge(origin.0, width, col)..edge(origin.0, width, col + 1))
                    .map(|x| pixel(&pixels, x, y)[3])
                    .collect();
                assert_eq!(top, bottom, "GPU vertical seam row {row} col {col}");
                assert!(top.iter().any(|a| *a != 0));
            }
        }
    }
    let gaps = snapshot("\x1b[38;2;255;0;0m╌╴╶╬", 1, 4);
    let pixels = rebuilt_pixels(gpu, &pipeline, &gaps, &viewport(), Palette::default());
    assert_eq!(pixel(&pixels, 7, 12)[3], 0, "dash gap");
    assert_eq!(pixel(&pixels, 28, 12)[3], 0, "left half endpoint");
    assert_eq!(pixel(&pixels, 33, 12)[3], 0, "right half endpoint");
    assert_eq!(pixel(&pixels, 56, 12)[3], 0, "double cross center gap");
    let clipped = RenderViewport {
        allocation_size: (250, 250),
        ..viewport()
    };
    let wide = snapshot(&"─".repeat(16), 1, 16);
    let pixels = rebuilt_pixels(gpu, &pipeline, &wide, &clipped, Palette::default());
    assert_eq!(pixel(&pixels, 247, 12)[3], 255);
    assert_eq!(pixel(&pixels, 248, 12)[3], 0, "render-pass scissor");
    eprintln!(
        "PASS: box GPU all 128 forms; sharp pixels; integer/fractional horizontal/vertical border joins and junctions; intentional gaps; scissor"
    );
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn gpu_retained_updates_resize_stride_palette_and_skipped_projection_match_rebuilds() {
    let (device, queue) = test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let pipeline = Arc::new(crate::render::gpu::create_colored_quad_pipeline(
        &device,
        gpu.format(),
        "retained box",
    ));
    let mut terminal = Terminal::new_headless(4, 8);
    let now = Instant::now();
    terminal.put_str("\x1b[31m┌──┐\x1b[2;1H│╳ │\x1b[3;1H└──┘");
    let initial = terminal.read_update(now);
    let view = viewport();
    let mut layer = BoxDrawing::new(gpu, Arc::clone(&pipeline), initial.appearance.palette());
    layer.prepare_with_dirty(gpu, &initial.snapshot, &[], &view);
    assert!(terminal.acknowledge_update(&initial));
    for stream in [
        "\x1b[1;2H━",
        "\x1b[1;2H\x1b[8m━",
        "\x1b[1;2H\x1b[28;32m─",
        "\x1b[1;2H\x1b[7;44m═",
        "\x1b[1;2H\x1b[0mA",
        "\x1b[2;1H\x1b[K",
        "\x1b[1;1H\x1b[31m╔",
        "\x1b]4;1;#0000ff\x07",
    ] {
        terminal.put_str(stream);
        let update = terminal.read_update(now);
        let ranges = match &update.damage {
            crate::model::UpdateDamage::Ranges(r) => r.clone(),
            _ => (0..update.snapshot.rows)
                .map(|row| DirtyRange {
                    row,
                    start_col: 0,
                    end_col: update.snapshot.cols,
                })
                .collect(),
        };
        layer.set_palette(update.appearance.palette());
        layer.prepare_with_dirty(gpu, &update.snapshot, &ranges, &view);
        let incremental = raster(&device, &queue, |p| layer.draw(p));
        assert_eq!(
            incremental,
            rebuilt_pixels(
                gpu,
                &pipeline,
                &update.snapshot,
                &view,
                update.appearance.palette()
            ),
            "edit {stream:?}"
        );
        assert!(terminal.acknowledge_update(&update));
    }
    let snap = terminal.read_update(now).snapshot.clone();
    let blue = Palette::default()
        .resolve(Color::Named(4))
        .map(|v| (v * 255.0).round() as u8);
    let inverse = snapshot("\x1b[31;44;7m─", 1, 2);
    let pixels = rebuilt_pixels(gpu, &pipeline, &inverse, &view, Palette::default());
    assert_eq!(pixel(&pixels, 1, 12), blue, "effective inverse foreground");
    // Both dimensions matter: 4x8 -> 8x4 preserves total cells, then shrink/grow.
    for (rows, cols) in [(8, 4), (4, 4), (3, 8), (4, 8)] {
        terminal.resize_if_changed(crate::TerminalSize { rows, cols });
        let update = terminal.read_update(now);
        layer.prepare_with_dirty(gpu, &update.snapshot, &[], &view);
        let (demand, held_gpu, held_cpu) = layer.geometry.storage_bytes();
        assert!(held_gpu >= demand && held_gpu <= 4 * demand);
        assert!(held_cpu >= demand && held_cpu <= 4 * demand);
        assert_eq!(
            raster(&device, &queue, |p| layer.draw(p)),
            rebuilt_pixels(
                gpu,
                &pipeline,
                &update.snapshot,
                &view,
                update.appearance.palette()
            )
        );
        assert!(terminal.acknowledge_update(&update));
    }
    let new_view = RenderViewport {
        cell_width: 10.25,
        line_height: 19.75,
        allocation_origin: (3.3, 4.7),
        ..view
    };
    layer.prepare_with_dirty(gpu, &snap, &[], &new_view);
    assert_eq!(
        raster(&device, &queue, |p| layer.draw(p)),
        rebuilt_pixels(
            gpu,
            &pipeline,
            &snap,
            &new_view,
            terminal.read_update(now).appearance.palette()
        )
    );
    terminal.put_str("\x1b[1;2H╬");
    let pending = terminal.read_update(now);
    let invalid = RenderViewport {
        surface_size: (0, 0),
        ..new_view
    };
    layer.prepare_with_dirty(gpu, &pending.snapshot, &[], &invalid);
    assert!(layer.dirty);
    assert!(layer.viewport.is_none());
    assert!(
        raster(&device, &queue, |p| layer.draw(p))
            .iter()
            .all(|p| *p == 0)
    );
    assert_eq!(
        terminal.read_update(now).damage,
        pending.damage,
        "skipped frames did not consume engine damage"
    );
    layer.prepare_with_dirty(gpu, &pending.snapshot, &[], &new_view);
    assert!(!layer.dirty);
    assert_eq!(
        raster(&device, &queue, |p| layer.draw(p)),
        rebuilt_pixels(
            gpu,
            &pipeline,
            &pending.snapshot,
            &new_view,
            pending.appearance.palette()
        )
    );
    // Invalid range bounds force a safe rebuild rather than indexing/uploading it.
    layer.prepare_with_dirty(
        gpu,
        &pending.snapshot,
        &[DirtyRange {
            row: usize::MAX,
            start_col: usize::MAX,
            end_col: 0,
        }],
        &new_view,
    );
    assert_eq!(
        raster(&device, &queue, |p| layer.draw(p)),
        rebuilt_pixels(
            gpu,
            &pipeline,
            &pending.snapshot,
            &new_view,
            pending.appearance.palette()
        )
    );
    let valid_size = layer.geometry.storage_bytes().1;
    for bad in [
        RenderViewport {
            cell_width: f32::NAN,
            ..new_view
        },
        RenderViewport {
            line_height: -1.0,
            ..new_view
        },
        RenderViewport {
            padding: f32::INFINITY,
            ..new_view
        },
        RenderViewport {
            allocation_origin: (f32::NAN, 0.0),
            ..new_view
        },
        RenderViewport {
            allocation_size: (0, 0),
            ..new_view
        },
    ] {
        layer.prepare_with_dirty(gpu, &pending.snapshot, &[], &bad);
        assert!(layer.dirty);
        assert!(layer.viewport.is_none());
        assert_eq!(
            layer.geometry.storage_bytes().1,
            valid_size,
            "invalid geometry preserves allocated valid resources"
        );
    }
    for size in [f32::MIN_POSITIVE, 0.3, 1e20, f32::MAX] {
        let extreme = RenderViewport {
            cell_width: size,
            line_height: size,
            ..new_view
        };
        layer.prepare_with_dirty(gpu, &pending.snapshot, &[], &extreme);
        assert_eq!(
            raster(&device, &queue, |p| layer.draw(p)),
            rebuilt_pixels(
                gpu,
                &pipeline,
                &pending.snapshot,
                &extreme,
                pending.appearance.palette()
            )
        );
    }
    layer.prepare_with_dirty(gpu, &pending.snapshot, &[], &new_view);
    assert_eq!(
        raster(&device, &queue, |p| layer.draw(p)),
        rebuilt_pixels(
            gpu,
            &pipeline,
            &pending.snapshot,
            &new_view,
            pending.appearance.palette()
        )
    );
    eprintln!(
        "PASS: box GPU edit/attribute/erase/conceal/reveal/palette/full equivalence, shrink/grow/equal-count stride, viewport, skipped frame recovery and upload bounds"
    );
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn gpu_box_scalar_suppression_suffix_preedit_and_font_resource_replacement() {
    use crate::{TerminalRenderPipeline, Text};
    use harbor_text::TextMetrics;
    let (device, queue) = test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let fonts = harbor_text::load_system_fonts(&harbor_config::FontSettings {
        family: Some("Consolas".into()),
        size: 16.0,
    })
    .unwrap();
    let metrics = TextMetrics::from_font_metrics(fonts.font_metrics());
    let view = RenderViewport {
        cell_width: metrics.cell_width,
        line_height: metrics.line_height,
        ..viewport()
    };
    let snap = snapshot("─╬╳╭", 2, 8);
    let mut text = Text::new(gpu, fonts, metrics, &snap, &view, Palette::default()).unwrap();
    // Ensure boxes really exist in the scalar atlas via the unchanged preedit
    // path, then remove preedit. Base suppression must not depend on atlas misses.
    let preedit = crate::Preedit::new("─", None);
    text.prepare_with_dirty(gpu, &snap, &[], &view, Some(&preedit));
    text.prepare_preedit(gpu, Some(&preedit), &snap, &view);
    assert!(
        raster(&device, &queue, |p| text.draw(p))
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[3] != 0),
        "preedit retains font path"
    );
    text.prepare_preedit(gpu, None, &snap, &view);
    assert!(
        raster(&device, &queue, |p| text.draw(p))
            .iter()
            .all(|p| *p == 0),
        "no duplicate scalar box paint even when cached"
    );
    let suffix = snapshot("─\u{301}", 2, 8);
    text.prepare_with_dirty(gpu, &suffix, &suffix.dirty_ranges, &view, None);
    text.prepare_preedit(gpu, None, &suffix, &view);
    assert!(
        raster(&device, &queue, |p| text.draw(p))
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[3] != 0),
        "combining suffix still paints"
    );
    let ordinary = snapshot("A", 2, 8);
    text.prepare_with_dirty(gpu, &ordinary, &ordinary.dirty_ranges, &view, None);
    text.prepare_preedit(gpu, None, &ordinary, &view);
    assert!(
        raster(&device, &queue, |p| text.draw(p))
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[3] != 0),
        "ordinary glyph still paints"
    );
    let mut screen = Screen::new(2, 8);
    TerminalParser::default().put_bytes(&mut screen, "─\u{301}╬".as_bytes());
    let before = screen.terminal_snapshot();
    assert_eq!(before.cursor_x, 2);
    let copy = screen.selected_text(crate::SelectionBounds {
        start_row: 0,
        start_col: 0,
        end_row: 0,
        end_col: 1,
    });
    assert_eq!(copy, "─\u{301}╬");

    // Actual renderer seam: changed native font metrics rebuild box projection
    // without new engine text damage. Only box pixels are isolated here; the
    // combined native-font/decoration desktop acceptance still belongs to T0003.
    let mut terminal = Terminal::new_headless(4, 8);
    terminal.put_str("─\u{301}╔═╗");
    let now = Instant::now();
    let update = terminal.read_update(now);
    let fonts = harbor_text::load_system_fonts(&harbor_config::FontSettings {
        family: Some("Consolas".into()),
        size: 16.0,
    })
    .unwrap();
    let metrics = TextMetrics::from_font_metrics(fonts.font_metrics());
    let mut renderer =
        TerminalRenderPipeline::new(gpu, (256, 256), fonts, metrics, &update).unwrap();
    renderer.prepare(gpu, &update, false);
    let old = raster(&device, &queue, |p| renderer.boxes_for_test().draw(p));
    assert!(terminal.acknowledge_update(&update));
    let current = terminal.read_update(now);
    assert_eq!(current.damage, crate::model::UpdateDamage::Ranges(vec![]));
    let fonts = harbor_text::load_system_fonts(&harbor_config::FontSettings {
        family: Some("Consolas".into()),
        size: 23.0,
    })
    .unwrap();
    renderer.replace_fonts(gpu, fonts, &current).unwrap();
    assert_ne!(renderer.metrics().cell_width, metrics.cell_width);
    let after = raster(&device, &queue, |p| renderer.boxes_for_test().draw(p));
    assert_ne!(
        old, after,
        "changed font-resource metrics reproject box pixels"
    );
    let pipe = Arc::new(crate::render::gpu::create_colored_quad_pipeline(
        &device,
        gpu.format(),
        "font box rebuild",
    ));
    assert_eq!(
        after,
        rebuilt_pixels(
            gpu,
            &pipe,
            &current.snapshot,
            &renderer.viewport(),
            current.appearance.palette()
        )
    );
    assert_eq!(
        current.snapshot.cells,
        terminal.read_update(now).snapshot.cells
    );
    for scale in [1.25, 1.5, 2.0, 1.0] {
        renderer.sync_raster_scale(gpu, scale, &current).unwrap();
        assert_eq!(
            after,
            raster(&device, &queue, |p| renderer.boxes_for_test().draw(p)),
            "DPI-only keeps physical box metrics"
        );
    }
    eprintln!(
        "PASS: scalar suppression, ordinary/suffix/preedit paths, source/copy/width/cursor, actual changed-metrics font-resource box reprojection"
    );
}

#[test]
#[ignore = "gpu: demand-bounded box capacity and dense-reference readback"]
fn gpu_demand_bounded_boxes_reclaim_content_and_resize_without_stale_paint() {
    let (device, queue) = test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let pipeline = Arc::new(crate::render::gpu::create_colored_quad_pipeline(
        &device,
        gpu.format(),
        "demand bounded boxes",
    ));
    let view = viewport();
    let palette = Palette::default();
    let mut layer = BoxDrawing::new(gpu, pipeline.clone(), palette);
    let compare_dense = |layer: &BoxDrawing, snap: &TerminalSnapshot| {
        let vertices: Vec<_> = (0..snap.rows)
            .flat_map(|row| {
                (0..snap.cols).flat_map(move |col| {
                    cell_vertices(snap.cell(row, col), &view, &palette, row, col)
                })
            })
            .collect();
        let buffer = crate::render::gpu::create_colored_vertex_buffer(&device, &vertices);
        assert_eq!(
            raster(&device, &queue, |p| layer.draw(p)),
            raster(&device, &queue, |p| {
                p.set_pipeline(&pipeline);
                p.set_vertex_buffer(0, buffer.slice(..));
                p.draw(0..vertices.len() as u32, 0..1);
            }),
        );
    };
    let mut sparse_bytes = None;
    for (rows, cols) in [(4, 8), (32, 64)] {
        let empty = snapshot("", rows, cols);
        layer.prepare_with_dirty(gpu, &empty, &[], &view);
        assert_eq!(layer.geometry.storage_bytes(), (0, 0, 0));
        let sparse = snapshot("─", rows, cols);
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
        let bytes = layer.geometry.storage_bytes();
        assert_eq!(
            *sparse_bytes.get_or_insert(bytes),
            bytes,
            "grid-independent geometry"
        );
        compare_dense(&layer, &sparse);
    }

    for _ in 0..3 {
        let mut dense = snapshot("", 8, 16);
        for c in dense.cells.iter_mut() {
            *c = cell('╬');
        }
        layer.prepare_with_dirty(gpu, &dense, &dense.dirty_ranges, &view);
        compare_dense(&layer, &dense);
        let (old_d, old_g, old_c) = layer.geometry.storage_bytes();
        assert!(old_d > 0 && old_g >= old_d && old_c >= old_d);
        let ranges: Vec<_> = (0..8)
            .map(|row| DirtyRange {
                row,
                start_col: 0,
                end_col: 16,
            })
            .collect();
        let mut sparse = snapshot("─", 8, 16);
        layer.prepare_with_dirty(gpu, &sparse, &ranges, &view);
        let (d, g, c) = layer.geometry.storage_bytes();
        assert!(old_g > 4 * d && old_c > 4 * d);
        assert!(g <= 2 * d && c <= 2 * d);
        eprintln!("boxes dense_bytes=({old_d},{old_g},{old_c}) sparse_bytes=({d},{g},{c})");
        compare_dense(&layer, &sparse);
        let warm = layer.geometry.buffers();
        assert_eq!(
            crate::render::observations::buffer_write_bytes(|| {
                layer.prepare_with_dirty(gpu, &sparse, &[], &view);
            }),
            0
        );
        assert_eq!(layer.geometry.buffers(), warm);
        sparse.cells[16] = cell('╳');
        let unchanged = layer.geometry.buffers()[0].clone();
        let writes = crate::render::observations::buffer_write_bytes(|| {
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
        assert!(writes > 0 && writes < old_d);
        assert_eq!(layer.geometry.buffers()[0], unchanged);
        compare_dense(&layer, &sparse);
        for (rows, cols) in [(16, 8), (2, 4), (8, 16)] {
            let resized = snapshot("╭─", rows, cols);
            layer.prepare_with_dirty(gpu, &resized, &[], &view);
            let (d, g, c) = layer.geometry.storage_bytes();
            assert!(g <= 4 * d && c <= 4 * d);
            compare_dense(&layer, &resized);
        }
        let empty = snapshot("", 8, 16);
        layer.prepare_with_dirty(gpu, &empty, &ranges, &view);
        assert_eq!(layer.geometry.storage_bytes(), (0, 0, 0));
        compare_dense(&layer, &empty);
    }
}
