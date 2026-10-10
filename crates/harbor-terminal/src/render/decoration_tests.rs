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
        .chunks_exact(6)
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
    for (col, slot) in overline.chunks_exact(6).enumerate() {
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
            retained.chunks_exact(4).any(|p| p == [0, 255, 0, 255]),
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
        pixels.chunks_exact(4).any(|p| p[3] != 0),
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
