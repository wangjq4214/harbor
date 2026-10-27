//! T0001 geometry and real GPU raster/readback evidence.
use super::*;
use crate::{Screen, TerminalParser};

fn metrics() -> TextMetrics {
    TextMetrics {
        cell_width: 16.0,
        line_height: 24.0,
        ascent: 18.0,
        underline_position: 19.0,
        underline_thickness: 1.0,
        strikethrough_position: 12.0,
        strikethrough_thickness: 1.0,
    }
}

fn viewport() -> RenderViewport {
    RenderViewport {
        padding: 0.0,
        ..RenderViewport::with_surface(16.0, 24.0, (256, 192), (256, 192))
    }
}

fn snapshot(stream: &str, rows: usize, cols: usize) -> TerminalSnapshot {
    let mut screen = Screen::new(rows, cols);
    TerminalParser::default().put_bytes(&mut screen, stream.as_bytes());
    screen.terminal_snapshot()
}

fn rects(vertices: &[ColoredVertex]) -> Vec<(f32, f32, f32, f32)> {
    vertices
        .as_chunks::<6>()
        .0
        .iter()
        .filter(|v| v[0].color[3] != 0.0)
        .map(|v| {
            let xs: Vec<_> = v.iter().map(|v| (v.position[0] + 1.0) * 128.0).collect();
            let ys: Vec<_> = v.iter().map(|v| (1.0 - v.position[1]) * 96.0).collect();
            (
                xs.iter().copied().fold(f32::INFINITY, f32::min),
                ys.iter().copied().fold(f32::INFINITY, f32::min),
                xs.iter().copied().fold(f32::NEG_INFINITY, f32::max),
                ys.iter().copied().fold(f32::NEG_INFINITY, f32::max),
            )
        })
        .collect()
}

#[test]
fn styles_spaces_wide_units_and_color_are_distinct_and_bounded() {
    let mut shapes = Vec::new();
    for style in 1..=5 {
        let snap = snapshot(&format!("\x1b[4:{style};58;2;255;0;0mA 界"), 1, 4);
        let vertices =
            build_underline_vertices(&metrics(), &snap, &viewport(), &Palette::default());
        assert_eq!(vertices.len(), 4 * UNDERLINE_VERTICES_PER_CELL);
        for col in 0..4 {
            let slot = &vertices
                [col * UNDERLINE_VERTICES_PER_CELL..(col + 1) * UNDERLINE_VERTICES_PER_CELL];
            let shape = rects(slot);
            assert!(!shape.is_empty(), "style {style}, cell {col}");
            for &(x0, y0, x1, y1) in &shape {
                assert!(x0 >= col as f32 * 16.0 && x1 <= (col + 1) as f32 * 16.0);
                assert!(y0 >= 0.0 && y1 <= 24.0);
            }
            for v in slot.iter().filter(|v| v.color[3] != 0.0) {
                assert_eq!(v.color, [1.0, 0.0, 0.0, 1.0]);
            }
            let incremental = underline_cell_vertices(
                &metrics(),
                &snap,
                &viewport(),
                &Palette::default(),
                0,
                col,
            );
            assert_eq!(
                bytemuck::cast_slice::<_, u8>(slot),
                bytemuck::cast_slice::<_, u8>(&incremental),
                "full and dirty-range geometry must agree"
            );
        }
        shapes.push(rects(&vertices[..UNDERLINE_VERTICES_PER_CELL]));
    }
    for a in 0..5 {
        for b in a + 1..5 {
            assert_ne!(shapes[a], shapes[b]);
        }
    }
    assert_eq!(shapes[0].len(), 1);
    assert_eq!(shapes[1].len(), 2);
    assert_eq!(shapes[2].len(), 16);
}

#[test]
fn continuous_lines_and_curves_have_no_cell_boundary_holes() {
    for style in [1, 2, 3] {
        let snap = snapshot(&format!("\x1b[4:{style}mA  界"), 1, 5);
        let vertices =
            build_underline_vertices(&metrics(), &snap, &viewport(), &Palette::default());
        for col in 0..4 {
            let a = rects(
                &vertices
                    [col * UNDERLINE_VERTICES_PER_CELL..(col + 1) * UNDERLINE_VERTICES_PER_CELL],
            );
            let b = rects(
                &vertices[(col + 1) * UNDERLINE_VERTICES_PER_CELL
                    ..(col + 2) * UNDERLINE_VERTICES_PER_CELL],
            );
            let boundary = (col + 1) as f32 * 16.0;
            assert!(
                a.iter()
                    .any(|ra| b.iter().any(|rb| (ra.2 - boundary).abs() < 0.001
                        && (rb.0 - boundary).abs() < 0.001
                        && ra.1.max(rb.1) < ra.3.min(rb.3))),
                "style {style} boundary {col}"
            );
        }
    }
}

#[test]
fn hyperlink_fallback_precedence_and_effective_palette_following() {
    let snap = snapshot(
        "\x1b]8;;https://example.test\x1b\\A 界\x1b[4:3m X\x1b[24mY",
        1,
        8,
    );
    assert_eq!(
        effective_underline(snap.cell(0, 0), &snap, 0, 0),
        UnderlineStyle::Single
    );
    assert_eq!(
        effective_underline(snap.cell(0, 1), &snap, 0, 1),
        UnderlineStyle::Off
    );
    assert_eq!(
        effective_underline(snap.cell(0, 2), &snap, 0, 2),
        UnderlineStyle::Single
    );
    assert_eq!(
        effective_underline(snap.cell(0, 3), &snap, 0, 3),
        UnderlineStyle::Single
    );
    assert_eq!(
        effective_underline(snap.cell(0, 4), &snap, 0, 4),
        UnderlineStyle::Curly
    );
    assert_eq!(
        effective_underline(snap.cell(0, 6), &snap, 0, 6),
        UnderlineStyle::Single
    );

    let mut palette = Palette::default();
    palette.normal[1] = harbor_config::Rgba::new(0.2, 0.4, 0.6, 1.0);
    for (stream, expected) in [
        ("\x1b[4:3;31mX", palette.resolve(Color::Named(1))),
        ("\x1b[4:3;31;44;7mX", palette.resolve(Color::Named(4))),
        (
            "\x1b[4:3;31;44;7;58;5;1mX",
            palette.resolve(Color::Indexed(1)),
        ),
        ("\x1b[4:3;31;44;7;58;2;255;0;0mX", [1.0, 0.0, 0.0, 1.0]),
        (
            "\x1b[4:3;31;44;7;58;5;1;59mX",
            palette.resolve(Color::Named(4)),
        ),
    ] {
        let snap = snapshot(stream, 1, 1);
        assert_eq!(
            build_underline_vertices(&metrics(), &snap, &viewport(), &palette)[0].color,
            expected
        );
    }
}

#[test]
fn metrics_origin_and_dpi_project_inside_each_row() {
    let snap = snapshot("\x1b[4:3m    ", 2, 2);
    for font_metrics in [
        metrics(),
        TextMetrics {
            underline_position: 2.0,
            underline_thickness: 3.0,
            ..metrics()
        },
    ] {
        for scale in [1.0, 1.5, 2.0] {
            let mut view = viewport();
            view.cell_width *= scale;
            view.line_height *= scale;
            view.allocation_origin = (7.0, 11.0);
            let vertices =
                build_underline_vertices(&font_metrics, &snap, &view, &Palette::default());
            for row in 0..2 {
                for col in 0..2 {
                    let slot = (row * 2 + col) * UNDERLINE_VERTICES_PER_CELL;
                    let (left, top, right, bottom) = view.cell_bounds(row, col);
                    let shape = rects(&vertices[slot..slot + UNDERLINE_VERTICES_PER_CELL]);
                    assert!(!shape.is_empty());
                    for (x0, y0, x1, y1) in shape {
                        assert!(x0 >= left - 0.001 && x1 <= right + 0.001);
                        assert!(y0 >= top && y1 <= bottom);
                    }
                }
            }
        }
    }
}

fn raster(device: &wgpu::Device, queue: &wgpu::Queue, decoration: &Decoration) -> Vec<u8> {
    raster_draw(device, queue, |pass| decoration.draw(pass))
}

fn raster_draw(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    draw: impl FnOnce(&mut wgpu::RenderPass),
) -> Vec<u8> {
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("T0001 readback"),
        size: wgpu::Extent3d {
            width: 256,
            height: 192,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("T0001 decoration pass"),
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
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_scissor_rect(0, 0, 128, 160);
        draw(&mut pass);
    }
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("T0001 pixels"),
        size: 256 * 192 * 4,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &target,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(1024),
                rows_per_image: Some(192),
            },
        },
        wgpu::Extent3d {
            width: 256,
            height: 192,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));
    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let pixels = slice.get_mapped_range().expect("mapped pixels").to_vec();
    buffer.unmap();
    pixels
}

#[test]
fn strikethrough_ranges_match_full_slots_for_spaces_wide_conceal_and_inverse() {
    let snap = snapshot("\x1b[9;31mA 界\x1b[8mB\x1b[28;7;44mC", 2, 8);
    let mut view = viewport();
    view.allocation_origin = (7.0, 11.0);
    let palette = Palette::default();
    let full = build_strikethrough_vertices(&metrics(), &snap, &view, &palette);
    for row in 0..snap.rows {
        for (start_col, end_col) in [(0, 8), (1, 4), (4, 6), (6, 6)] {
            let mut range = Vec::new();
            append_strikethrough_range(
                &metrics(),
                &snap,
                &view,
                &palette,
                &DirtyRange {
                    row,
                    start_col,
                    end_col,
                },
                &mut range,
            );
            let start = (row * snap.cols + start_col) * 6;
            let end = (row * snap.cols + end_col) * 6;
            assert_eq!(
                bytemuck::cast_slice::<_, u8>(&range),
                bytemuck::cast_slice::<_, u8>(&full[start..end])
            );
        }
    }
    for col in [1, 3, 4] {
        assert!(rects(&full[col * 6..(col + 1) * 6]).is_empty());
    }
    assert_eq!(full[5 * 6].color, palette.resolve(Color::Named(4)));
}

#[test]
fn gpu_modern_underline_styles_readback_and_incremental_reprojection() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    })) {
        Ok(adapter) => adapter,
        Err(error) => {
            eprintln!("BLOCKED: T0001 GPU adapter: {error}");
            return;
        }
    };
    eprintln!("T0001 GPU adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("T0001 GPU"),
        ..Default::default()
    }))
    .expect("T0001 GPU device");
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let pipeline = Arc::new(gpu::create_colored_quad_pipeline(
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
fn conceal_suppresses_all_decoration_and_overline_uses_effective_foreground() {
    for prefix in ["", "\x1b]8;;https://example.test\x1b\\"] {
        for attrs in [
            "8;4;9;53",
            "8;4;9;53;7;31;44",
            "8;53",
            "8;4:1;58;5;123;53",
            "8;4:2;58;5;123;53",
            "8;4:3;58;2;255;0;0;53;7;31;44",
            "8;4:4;58;5;123;53",
            "8;4:5;58;5;123;53",
        ] {
            let snap = snapshot(&format!("{prefix}\x1b[{attrs}mAe\u{301}界  "), 1, 6);
            for vertices in [
                build_underline_vertices(&metrics(), &snap, &viewport(), &Palette::default()),
                build_strikethrough_vertices(&metrics(), &snap, &viewport(), &Palette::default()),
                build_overline_vertices(&metrics(), &snap, &viewport(), &Palette::default()),
            ] {
                assert!(
                    rects(&vertices).is_empty(),
                    "{attrs}: no foreground decoration"
                );
            }
        }
    }
    let snap = snapshot("\x1b[53;4;9;31;44;7mA 界", 1, 4);
    let mut palette = Palette::default();
    palette.normal[4] = harbor_config::Rgba::new(0.3, 0.6, 0.9, 1.0);
    let overline = build_overline_vertices(&metrics(), &snap, &viewport(), &palette);
    assert_eq!(
        rects(&overline).len(),
        4,
        "spaces and wide continuation each have one slot"
    );
    for (col, slot) in overline.as_chunks::<6>().0.iter().enumerate() {
        let shape = rects(slot);
        let (left, top, right, bottom) = shape[0];
        assert!((left - col as f32 * 16.0).abs() < 0.001);
        assert!((right - (col + 1) as f32 * 16.0).abs() < 0.001);
        assert!(top.abs() < 0.001 && (bottom - 1.0).abs() < 0.001);
        assert_eq!(slot[0].color, palette.resolve(Color::Named(4)));
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(slot),
            bytemuck::cast_slice::<_, u8>(&overline_cell_vertices(
                &metrics(),
                &snap,
                &viewport(),
                &palette,
                0,
                col
            ))
        );
    }
    assert!(
        !rects(&build_underline_vertices(
            &metrics(),
            &snap,
            &viewport(),
            &palette
        ))
        .is_empty()
    );
    assert!(
        !rects(&build_strikethrough_vertices(
            &metrics(),
            &snap,
            &viewport(),
            &palette
        ))
        .is_empty()
    );
}

#[test]
fn overline_font_origin_dpi_geometry_stays_inside_rows() {
    let snap = snapshot("\x1b[53m    ", 2, 2);
    for thickness in [0.2, 1.0, 3.0, 100.0] {
        let font = TextMetrics {
            underline_thickness: thickness,
            ..metrics()
        };
        for scale in [1.0, 1.5, 2.0] {
            let mut view = viewport();
            view.cell_width *= scale;
            view.line_height *= scale;
            view.allocation_origin = (7.0, 11.0);
            let vertices = build_overline_vertices(&font, &snap, &view, &Palette::default());
            for row in 0..2 {
                for col in 0..2 {
                    let start = (row * 2 + col) * 6;
                    let shape = rects(&vertices[start..start + 6]);
                    assert_eq!(shape.len(), 1);
                    let (x0, y0, x1, y1) = shape[0];
                    let (left, top, right, bottom) = view.cell_bounds(row, col);
                    assert!((x0 - left).abs() < 0.001 && (x1 - right).abs() < 0.001);
                    assert!((y0 - top).abs() < 0.001 && y1 <= bottom + 0.001);
                    assert!(
                        (y1 - y0 - (thickness * scale).max(1.0).min(view.line_height / 4.0)).abs()
                            < 0.001
                    );
                }
            }
        }
    }
}

#[test]
fn gpu_conceal_overline_readback_dirty_removal_and_reprojection() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .expect("T0002 requires a GPU adapter; missing adapter is a prerequisite failure");
    eprintln!("T0002 GPU adapter: {:?}", adapter.get_info());
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let pipeline = Arc::new(gpu::create_colored_quad_pipeline(
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
    let mut text_layer =
        super::super::text::Text::new(gpu, fonts, font_metrics, &hidden, &view, Palette::default())
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
fn gpu_osc_palette_retained_layers_recolor_and_replay() {
    use crate::{Terminal, TerminalAppearance};
    use std::time::Instant;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: None,
        ..Default::default()
    })) {
        Ok(adapter) => adapter,
        Err(error) => {
            eprintln!("BLOCKED: OSC palette GPU adapter: {error}");
            return;
        }
    };
    eprintln!("OSC palette GPU adapter: {:?}", adapter.get_info());
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
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
fn t0002_all_decoration_metrics_project_and_clamp_inside_fractional_rows() {
    let palette = Palette::default();
    for style in 1..=5 {
        let snap = snapshot(&format!("\x1b[4:{style};9;53mXXX\r\nXXX"), 2, 3);
        for height in [12.0, 24.0, 37.5] {
            for offset in [-100.0, 0.0, 8.25, 1000.0] {
                let m = TextMetrics {
                    line_height: 32.0,
                    ascent: 25.0,
                    underline_position: offset,
                    strikethrough_position: offset,
                    underline_thickness: 2.5,
                    strikethrough_thickness: 3.0,
                    ..metrics()
                };
                let mut view = viewport();
                view.cell_width = 13.75;
                view.line_height = height;
                view.padding = 0.25;
                view.allocation_origin = (3.35, 4.45);
                let u = build_underline_vertices(&m, &snap, &view, &palette);
                let s = build_strikethrough_vertices(&m, &snap, &view, &palette);
                let o = build_overline_vertices(&m, &snap, &view, &palette);
                assert_eq!(u.len(), 6 * 96, "underline layout contract");
                let scale = height / m.line_height;
                let ut = (m.underline_thickness * scale).max(1.0).min(height / 4.0);
                let st = (m.strikethrough_thickness * scale)
                    .max(1.0)
                    .min(height / 4.0);
                for row in 0..2 {
                    for col in 0..3 {
                        let index = row * 3 + col;
                        let (left, top, right, bottom) = view.cell_bounds(row, col);
                        for (vertices, count) in [(&u, 96), (&s, 6), (&o, 6)] {
                            let shape = rects(&vertices[index * count..(index + 1) * count]);
                            assert!(!shape.is_empty());
                            for (x0, y0, x1, y1) in shape {
                                assert!(x0 >= left - 0.0001 && x1 <= right + 0.0001);
                                assert!(y0 >= top - 0.0001 && y1 <= bottom + 0.0001);
                            }
                        }
                        let strike = rects(&s[index * 6..(index + 1) * 6])[0];
                        let expected_top =
                            (top + offset * scale - st / 2.0).clamp(top, bottom - st);
                        assert!((strike.1 - expected_top).abs() < 0.0001);
                        assert!((strike.3 - strike.1 - st).abs() < 0.0001);
                        let overline = rects(&o[index * 6..(index + 1) * 6])[0];
                        assert!((overline.1 - top).abs() < 0.0001);
                        assert!((overline.3 - top - ut).abs() < 0.0001);
                        if style == 1 {
                            let underline = rects(&u[index * 96..(index + 1) * 96])[0];
                            let expected_top = (top + offset * scale).clamp(top, bottom - 3.0 * ut);
                            assert!((underline.1 - expected_top).abs() < 0.0001);
                            assert!((underline.3 - underline.1 - ut).abs() < 0.0001);
                        }
                        let mut range = Vec::new();
                        append_strikethrough_range(
                            &m,
                            &snap,
                            &view,
                            &palette,
                            &DirtyRange {
                                row,
                                start_col: col,
                                end_col: col + 1,
                            },
                            &mut range,
                        );
                        assert_eq!(
                            bytemuck::cast_slice::<_, u8>(&range),
                            bytemuck::cast_slice::<_, u8>(&s[index * 6..(index + 1) * 6])
                        );
                    }
                }
            }
        }
    }
}

fn assert_finite_slots(vertices: &[ColoredVertex]) {
    for vertex in vertices {
        assert!(
            vertex
                .position
                .into_iter()
                .all(|v| v.is_finite() && v.abs() <= 1.0)
        );
        assert!(vertex.color.into_iter().all(f32::is_finite));
    }
}

#[test]
fn t0002_degenerate_extreme_projection_and_capacity_are_bounded() {
    let palette = Palette::default();
    for style in 1..=5 {
        let snap = snapshot(&format!("\x1b[4:{style};9;53mXX\r\nXX"), 2, 2);
        for field in 0..7 {
            for value in [
                0.0,
                -1.0,
                f32::NAN,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::from_bits(1),
                f32::MAX,
            ] {
                let mut m = metrics();
                *match field {
                    0 => &mut m.cell_width,
                    1 => &mut m.line_height,
                    2 => &mut m.ascent,
                    3 => &mut m.underline_position,
                    4 => &mut m.underline_thickness,
                    5 => &mut m.strikethrough_position,
                    _ => &mut m.strikethrough_thickness,
                } = value;
                for vertices in [
                    build_underline_vertices(&m, &snap, &viewport(), &palette),
                    build_strikethrough_vertices(&m, &snap, &viewport(), &palette),
                    build_overline_vertices(&m, &snap, &viewport(), &palette),
                ] {
                    assert_finite_slots(&vertices);
                    if !valid_metrics(&m) {
                        assert!(vertices.iter().all(|v| v.color[3] == 0.0));
                    }
                }
            }
        }
        for value in [
            0.0,
            -1.0,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::from_bits(1),
            f32::MAX,
        ] {
            for field in 0..5 {
                let mut view = viewport();
                *match field {
                    0 => &mut view.cell_width,
                    1 => &mut view.line_height,
                    2 => &mut view.padding,
                    3 => &mut view.allocation_origin.0,
                    _ => &mut view.allocation_origin.1,
                } = value;
                for vertices in [
                    build_underline_vertices(&metrics(), &snap, &view, &palette),
                    build_strikethrough_vertices(&metrics(), &snap, &view, &palette),
                    build_overline_vertices(&metrics(), &snap, &view, &palette),
                ] {
                    assert_finite_slots(&vertices);
                }
                if let Some(projection) =
                    DecorationCell::new(&metrics(), &view, usize::MAX, usize::MAX)
                {
                    assert_finite_slots(&projection.rect(projection.bounds, [1.0; 4]));
                }
            }
        }
        for (allocation_size, surface_size) in [
            ((0, 192), (256, 192)),
            ((256, 0), (256, 192)),
            ((256, 192), (0, 192)),
            ((256, 192), (256, 0)),
        ] {
            let view = RenderViewport {
                allocation_size,
                surface_size,
                ..viewport()
            };
            for vertices in [
                build_underline_vertices(&metrics(), &snap, &view, &palette),
                build_strikethrough_vertices(&metrics(), &snap, &view, &palette),
                build_overline_vertices(&metrics(), &snap, &view, &palette),
            ] {
                assert_finite_slots(&vertices);
                assert!(vertices.iter().all(|v| v.color[3] == 0.0));
            }
        }
    }
    let mut snap = snapshot("X", 2, 2);
    let bytes = (4 * 96 * std::mem::size_of::<ColoredVertex>()) as u64;
    assert_eq!(slot_count(&snap, bytes), Some(4));
    assert_eq!(slot_count(&snap, bytes - 1), None);
    for (rows, cols) in [
        (0, 2),
        (2, 0),
        (usize::MAX, 2),
        (2, usize::MAX),
        (1, usize::MAX / 96),
    ] {
        snap.rows = rows;
        snap.cols = cols;
        assert_eq!(slot_count(&snap, u64::MAX), None);
        assert!(
            build_underline_vertices(&metrics(), &snap, &viewport(), &Palette::default())
                .is_empty()
        );
        assert!(
            build_strikethrough_vertices(&metrics(), &snap, &viewport(), &Palette::default())
                .is_empty()
        );
        assert!(
            build_overline_vertices(&metrics(), &snap, &viewport(), &Palette::default()).is_empty()
        );
    }
}

#[test]
fn t0002_gpu_retained_edits_stride_projection_and_invalid_recovery_match_full() {
    let (device, queue) = super::super::text::sequence_tests::test_gpu()
        .expect("BLOCKED: T0002 requires an actual GPU adapter");
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let pipeline = Arc::new(gpu::create_colored_quad_pipeline(
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
