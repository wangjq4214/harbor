#[path = "box_drawing_tests/gpu.rs"]
mod gpu;

use super::*;
use crate::{Color, Screen, Terminal, TerminalParser};
use std::time::Instant;

fn viewport() -> RenderViewport {
    RenderViewport {
        padding: 0.0,
        ..RenderViewport::with_surface(16.0, 24.0, (256, 256), (256, 256))
    }
}

fn cell(ch: char) -> Cell {
    Cell {
        ch,
        fg: Color::Rgb(255, 0, 0),
        ..Cell::default()
    }
}

fn snapshot(stream: &str, rows: usize, cols: usize) -> TerminalSnapshot {
    let mut screen = Screen::new(rows, cols);
    TerminalParser::default().put_bytes(&mut screen, stream.as_bytes());
    screen.terminal_snapshot()
}

fn vertices(
    ch: char,
    view: &RenderViewport,
    row: usize,
    col: usize,
) -> [ColoredVertex; BOX_VERTICES_PER_CELL] {
    cell_vertices(&cell(ch), view, &Palette::default(), row, col)
}

fn covers(vertices: &[ColoredVertex], view: &RenderViewport, x: f64, y: f64) -> bool {
    let p = [
        x / f64::from(view.surface_size.0) * 2.0 - 1.0,
        1.0 - y / f64::from(view.surface_size.1) * 2.0,
    ];
    vertices.as_chunks::<3>().0.iter().any(|tri| {
        if tri[0].color[3] == 0.0 {
            return false;
        }
        let points = std::array::from_fn::<Point, 3, _>(|i| tri[i].position.map(f64::from));
        let cross = |a: Point, b: Point, c: Point| {
            (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
        };
        if cross(points[0], points[1], points[2]).abs() < 1e-12 {
            return false;
        }
        let signs = [
            cross(points[0], points[1], p),
            cross(points[1], points[2], p),
            cross(points[2], points[0], p),
        ];
        signs.iter().all(|s| *s >= -1e-9) || signs.iter().all(|s| *s <= 1e-9)
    })
}

#[test]
fn complete_repertoire_has_expected_direction_weight_and_special_forms() {
    // L/R/U/D digit oracle, transcribed in Unicode scalar order. Unlike the
    // implementation it has no constructors/kinds or geometry control flow.
    let groups = [
        "1100 2200 0011 0022 1100 2200 0011 0022 1100 2200 0011 0022 0101 0201 0102 0202",
        "1001 2001 1002 2002 0110 0210 0120 0220 1010 2010 1020 2020 0111 0211 0121 0112",
        "0122 0221 0212 0222 1011 2011 1021 1012 1022 2021 2012 2022 1101 2101 1201 2201",
        "1102 2102 1202 2202 1110 2110 1210 2210 1120 2120 1220 2220 1111 2111 1211 2211",
        "1121 1112 1122 2121 1221 2112 1212 2221 2212 2122 1222 2222 1100 2200 0011 0022",
        "3300 0033 0301 0103 0303 3001 1003 3003 0310 0130 0330 3010 1030 3030 0311 0133",
        "0333 3011 1033 3033 3301 1103 3303 3310 1130 3330 3311 1133 3333 0101 1001 1010",
        "0110 0000 0000 0000 1000 0010 0100 0001 2000 0020 0200 0002 1200 0012 2100 0021",
    ];
    let oracle: Vec<_> = groups.iter().flat_map(|s| s.split_whitespace()).collect();
    assert_eq!(oracle.len(), 128);
    let view = viewport();
    for (i, expected) in oracle.iter().enumerate() {
        let ch = char::from_u32(0x2500 + i as u32).unwrap();
        let descriptor = shape(ch).unwrap();
        assert_eq!(
            descriptor.arms,
            std::array::from_fn(|i| expected.as_bytes()[i] - b'0'),
            "U+{:04X}",
            ch as u32
        );
        let output = vertices(ch, &view, 0, 0);
        assert!(
            output.iter().any(|v| v.color[3] != 0.0),
            "U+{:04X} emitted geometry",
            ch as u32
        );
        assert!(output.iter().all(|v| {
            v.position
                .iter()
                .chain(v.color.iter())
                .all(|x| x.is_finite())
        }));
        // No cell can exceed its independently bounded fixed slot.
        assert_eq!(output.len(), BOX_VERTICES_PER_CELL);
    }
    for (range, kind) in [
        (0x2504..=0x2507, Kind::Dash3),
        (0x2508..=0x250b, Kind::Dash4),
        (0x254c..=0x254f, Kind::Dash2),
        (0x256d..=0x2570, Kind::Round),
    ] {
        for n in range {
            assert_eq!(shape(char::from_u32(n).unwrap()).unwrap().kind, kind);
        }
    }
    for (ch, kind) in [
        ('╱', Kind::Slash),
        ('╲', Kind::Backslash),
        ('╳', Kind::Cross),
    ] {
        assert_eq!(shape(ch).unwrap().kind, kind);
    }
    for ch in ['A', ' ', '▀', '⠿', '\u{24ff}', '\u{2580}'] {
        assert!(shape(ch).is_none());
    }
}

#[test]
fn compatible_cpu_edges_match_exhaustively_at_fractional_origins_and_dimensions() {
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
        for a in 0x2500..=0x257f {
            let ca = char::from_u32(a).unwrap();
            let sa = shape(ca).unwrap();
            if !matches!(sa.kind, Kind::Straight | Kind::Round) {
                continue;
            }
            for b in 0x2500..=0x257f {
                let cb = char::from_u32(b).unwrap();
                let sb = shape(cb).unwrap();
                if !matches!(sb.kind, Kind::Straight | Kind::Round) {
                    continue;
                }
                if sa.arms[1] != 0 && sa.arms[1] == sb.arms[0] {
                    let av = vertices(ca, &view, 1, 1);
                    let bv = vertices(cb, &view, 1, 2);
                    let edge = (f64::from(origin.0) + 2.0 * f64::from(width)).round();
                    for y in 0..64 {
                        let y = y as f64 + 0.5;
                        assert_eq!(
                            covers(&av, &view, edge - 0.01, y),
                            covers(&bv, &view, edge + 0.01, y),
                            "horizontal U+{a:04X}/U+{b:04X}"
                        );
                    }
                }
                if sa.arms[3] != 0 && sa.arms[3] == sb.arms[2] {
                    let av = vertices(ca, &view, 1, 1);
                    let bv = vertices(cb, &view, 2, 1);
                    let edge = (f64::from(origin.1) + 2.0 * f64::from(height)).round();
                    for x in 0..64 {
                        let x = x as f64 + 0.5;
                        assert_eq!(
                            covers(&av, &view, x, edge - 0.01),
                            covers(&bv, &view, x, edge + 0.01),
                            "vertical U+{a:04X}/U+{b:04X}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn stroke_distinctions_junctions_intentional_gaps_and_endpoints_are_preserved() {
    let view = viewport();
    let v = |ch, x, y| covers(&vertices(ch, &view, 0, 0), &view, x, y);
    assert!(v('─', 1.5, 12.5));
    assert!(!v('─', 1.5, 13.5));
    assert!(v('━', 1.5, 13.5));
    assert!(v('═', 1.5, 10.5));
    assert!(v('═', 1.5, 14.5));
    assert!(!v('═', 1.5, 12.5));
    assert!(v('┼', 8.5, 12.5));
    assert!(v('╔', 6.5, 14.5));
    assert!(v('╔', 10.5, 10.5));
    assert!(!v('╔', 8.5, 12.5));
    assert!(!v('╬', 8.5, 12.5));
    assert!(v('╬', 10.5, 14.5));
    assert!(v('╞', 8.5, 12.5));
    assert!(v('╞', 15.5, 10.5));
    for ch in ['╌', '╍', '┄', '┅', '┈', '┉'] {
        let count = (0..16).filter(|&x| v(ch, x as f64 + 0.5, 12.5)).count();
        assert!(count > 0 && count < 16, "intentional dash gaps {ch}");
    }
    assert!(v('╴', 0.5, 12.5));
    assert!(!v('╴', 12.5, 12.5));
    assert!(!v('╶', 1.5, 12.5));
    assert!(v('╶', 15.5, 12.5));
    assert!(!v('╭', 8.1, 12.1));
    assert!(v('╭', 15.5, 12.5));
    assert!(v('╱', 1.5, 21.5));
    assert!(!v('╱', 1.5, 1.5));
    assert!(v('╲', 1.5, 1.5));
    assert!(v('╳', 1.5, 21.5));
}

#[test]
fn invalid_extreme_and_clipped_geometry_is_finite_bounded_and_safe() {
    let base = viewport();
    for value in [
        0.0,
        -1.0,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::MIN_POSITIVE,
        1e-20,
        0.3,
        1e20,
        f32::MAX,
    ] {
        for view in [
            RenderViewport {
                cell_width: value,
                ..base
            },
            RenderViewport {
                line_height: value,
                ..base
            },
            RenderViewport {
                allocation_origin: (value, value),
                ..base
            },
            RenderViewport {
                padding: value,
                ..base
            },
        ] {
            for n in 0x2500..=0x257f {
                for (row, col) in [(0, 0), (3, 7), (usize::MAX, usize::MAX)] {
                    let output = vertices(char::from_u32(n).unwrap(), &view, row, col);
                    assert!(output.iter().all(|v| {
                        v.position
                            .iter()
                            .chain(v.color.iter())
                            .all(|x| x.is_finite())
                    }));
                    for v in output.iter().filter(|v| v.color[3] != 0.0) {
                        assert!(v.position.iter().all(|x| (-1.0..=1.0).contains(x)));
                    }
                }
            }
        }
    }
    for view in [
        RenderViewport {
            surface_size: (0, 1),
            ..base
        },
        RenderViewport {
            allocation_size: (1, 0),
            ..base
        },
    ] {
        assert!(vertices('╳', &view, 0, 0).iter().all(|v| v.color[3] == 0.0));
    }
    let mut snap = snapshot("─", 1, 2);
    assert_eq!(slot_count(&snap, u64::MAX), Some(2 * BOX_VERTICES_PER_CELL));
    assert_eq!(slot_count(&snap, 1), None);
    let empty = snapshot("", 1, 2);
    let mut malformed = empty.clone();
    malformed.rows = usize::MAX;
    malformed.cols = 0;
    assert_eq!(slot_count(&malformed, u64::MAX), None);
    snap.cols = usize::MAX;
    assert_eq!(slot_count(&snap, u64::MAX), None);
    snap.rows = usize::MAX;
    assert_eq!(slot_count(&snap, u64::MAX), None);
    let mut concealed = cell('─');
    concealed.attrs.set(CellAttrs::CONCEAL);
    assert!(
        cell_vertices(&concealed, &base, &Palette::default(), 0, 0)
            .iter()
            .all(|v| v.color[3] == 0.0)
    );
    concealed.attrs = CellAttrs::default();
    concealed.wide_continuation = true;
    assert!(
        cell_vertices(&concealed, &base, &Palette::default(), 0, 0)
            .iter()
            .all(|v| v.color[3] == 0.0)
    );
}

use crate::test_gpu::device as test_gpu;

fn raster(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    draw: impl FnOnce(&mut wgpu::RenderPass),
) -> Vec<u8> {
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("box readback"),
        size: wgpu::Extent3d {
            width: 256,
            height: 256,
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
            label: Some("box pass"),
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
        pass.set_scissor_rect(0, 0, 248, 248);
        draw(&mut pass);
    }
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("box pixels"),
        size: 256 * 256 * 4,
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
                rows_per_image: Some(256),
            },
        },
        wgpu::Extent3d {
            width: 256,
            height: 256,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let pixels = buffer.slice(..).get_mapped_range().unwrap().to_vec();
    buffer.unmap();
    pixels
}

fn pixel(pixels: &[u8], x: usize, y: usize) -> &[u8] {
    &pixels[(y * 256 + x) * 4..(y * 256 + x) * 4 + 4]
}

fn rebuilt_pixels(
    gpu: TerminalGpuAccess<'_>,
    pipeline: &Arc<wgpu::RenderPipeline>,
    snap: &TerminalSnapshot,
    view: &RenderViewport,
    palette: Palette,
) -> Vec<u8> {
    let mut fresh = BoxDrawing::new(gpu, Arc::clone(pipeline), palette);
    fresh.prepare_with_dirty(gpu, snap, &[], view);
    raster(gpu.device(), gpu.queue(), |p| fresh.draw(p))
}

fn components(mut mask: Vec<bool>, width: usize, height: usize) -> usize {
    let mut count = 0;
    for start in 0..mask.len() {
        if !mask[start] {
            continue;
        }
        count += 1;
        mask[start] = false;
        let mut stack = vec![start];
        while let Some(p) = stack.pop() {
            let (x, y) = (p % width, p / width);
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let (nx, ny) = (x as isize + dx, y as isize + dy);
                    if nx >= 0 && ny >= 0 && nx < width as isize && ny < height as isize {
                        let n = ny as usize * width + nx as usize;
                        if mask[n] {
                            mask[n] = false;
                            stack.push(n);
                        }
                    }
                }
            }
        }
    }
    count
}
