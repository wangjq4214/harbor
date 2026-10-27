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

fn test_gpu() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .expect("BLOCKED: box GPU tests require a real adapter; unavailable is NOT a pass");
    eprintln!("BOX GPU adapter: {:?}", adapter.get_info());
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
        .expect("BOX GPU device")
}

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

#[test]
fn gpu_repertoire_sharp_pixels_and_fractional_horizontal_vertical_junction_joins() {
    let (device, queue) = test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let pipeline = Arc::new(gpu::create_colored_quad_pipeline(
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
fn gpu_retained_updates_resize_stride_palette_and_skipped_projection_match_rebuilds() {
    let (device, queue) = test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let pipeline = Arc::new(gpu::create_colored_quad_pipeline(
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
    let snap = terminal.read_update(now).snapshot;
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
        assert_eq!(
            layer.vertex_count as usize,
            rows * cols * BOX_VERTICES_PER_CELL
        );
        assert!(
            layer.buffer.size()
                >= (layer.vertex_count as u64) * std::mem::size_of::<ColoredVertex>() as u64
        );
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
    assert_eq!(layer.vertex_count, 0);
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
    let valid_size = layer.buffer.size();
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
        assert_eq!(layer.vertex_count, 0);
        assert_eq!(
            layer.buffer.size(),
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
fn gpu_box_scalar_suppression_suffix_preedit_and_font_resource_replacement() {
    use super::super::{TerminalRenderPipeline, Text};
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
    let old = raster(&device, &queue, |p| renderer.boxes.draw(p));
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
    let after = raster(&device, &queue, |p| renderer.boxes.draw(p));
    assert_ne!(
        old, after,
        "changed font-resource metrics reproject box pixels"
    );
    let pipe = Arc::new(gpu::create_colored_quad_pipeline(
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
            raster(&device, &queue, |p| renderer.boxes.draw(p)),
            "DPI-only keeps physical box metrics"
        );
    }
    eprintln!(
        "PASS: scalar suppression, ordinary/suffix/preedit paths, source/copy/width/cursor, actual changed-metrics font-resource box reprojection"
    );
}
