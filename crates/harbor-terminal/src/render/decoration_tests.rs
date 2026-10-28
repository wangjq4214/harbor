//! T0001 geometry and real GPU raster/readback evidence.
#[path = "decoration_tests/gpu.rs"]
mod gpu;

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
