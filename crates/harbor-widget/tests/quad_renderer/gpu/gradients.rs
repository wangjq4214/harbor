use super::*;

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn gradient_eight_stops_respect_nested_clips_and_translucent_edge_coverage() {
    let (device, queue) = gpu_device();
    let stops: Vec<_> = (0..8)
        .map(|i| {
            (
                i as f32 / 7.0,
                Color {
                    a: 0.5,
                    ..if i < 4 { Color::RED } else { Color::BLUE }
                },
            )
        })
        .collect();
    let paint = test_gradient(Point::ZERO, Point::new(1.0, 0.0), &stops);
    for scale in [1.0, 1.5, 2.0] {
        let mut item = make_rounded_item(
            1,
            Primitive::PaintedQuad {
                rect: Rect::from_min_size(Point::ZERO, Size::new(32.0, 32.0)),
                paint,
                corner_radii: [4.0; 4],
            },
        );
        for (origin, side, radius) in [(2.0, 28.0, 4.0), (4.0, 24.0, 6.0)] {
            item.clips.push(
                RoundedClip::new(
                    Rect::from_min_size(Point::new(origin, origin), Size::new(side, side)),
                    BorderRadius::all(radius).unwrap(),
                    ClipBehavior::AntiAlias,
                )
                .unwrap(),
            );
        }
        let side = (32.0 * scale) as u32;
        let pixels = render_quad_item(
            &device,
            &queue,
            item,
            Viewport::new(side, side, scale),
            side,
        );
        let at = |x: f32, y: f32| bgra(&pixels, (x * scale) as u32, (y * scale) as u32);
        assert_eq!(at(6.0, 16.0), [0, 0, 128, 128], "first half of stop array");
        assert_eq!(
            at(25.0, 16.0),
            [128, 0, 0, 128],
            "second half of stop array"
        );
        assert_eq!(
            at(4.0, 4.0),
            [0; 4],
            "ancestor rounded corner clips gradient"
        );
        assert_eq!(at(1.0, 16.0), [0; 4], "outside ancestors stays empty");
        let edge = at(4.0, 16.0);
        assert!(
            edge[3] > 0 && edge[3] < 128,
            "antialias scales paint alpha: {edge:?}"
        );
    }
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn gradient_fill_and_stroke_share_stops_direction_clamping_and_rounded_coverage() {
    let (device, queue) = gpu_device();
    for (start, end, reversed) in [
        (Point::ZERO, Point::new(1.0, 0.0), false),
        (Point::new(1.0, 0.0), Point::ZERO, true),
    ] {
        let paint = test_gradient(
            start,
            end,
            &[(0.25, Color::RED), (0.5, Color::GREEN), (0.75, Color::BLUE)],
        );
        let rect = Rect::from_min_size(Point::ZERO, Size::new(32.0, 32.0));
        for scale in [1.0, 1.5, 2.0] {
            let side = (32.0 * scale) as u32;
            let viewport = Viewport::new(side, side, scale);
            for border in [false, true] {
                let primitive = if border {
                    Primitive::PaintedBorder {
                        rect,
                        width: 3.0,
                        paint,
                        corner_radii: [6.0; 4],
                    }
                } else {
                    Primitive::PaintedQuad {
                        rect,
                        paint,
                        corner_radii: [6.0; 4],
                    }
                };
                let pixels = render_quad_item(
                    &device,
                    &queue,
                    make_rounded_item(1, primitive),
                    viewport.clone(),
                    side,
                );
                let y = if border { 1.0 } else { 16.0 };
                let left = bgra(&pixels, (6.0 * scale) as u32, (y * scale) as u32);
                let right = bgra(&pixels, (26.0 * scale) as u32, (y * scale) as u32);
                assert_eq!(
                    left,
                    if reversed {
                        [255, 0, 0, 255]
                    } else {
                        [0, 0, 255, 255]
                    }
                );
                assert_eq!(
                    right,
                    if reversed {
                        [0, 0, 255, 255]
                    } else {
                        [255, 0, 0, 255]
                    }
                );
                let middle = bgra(&pixels, (16.0 * scale) as u32, (y * scale) as u32);
                assert!(
                    middle[1] > 230 && middle[3] == 255,
                    "middle stop: {middle:?}"
                );
                assert_eq!(bgra(&pixels, 0, 0)[3], 0, "rounded corner remains empty");
                if border {
                    assert_eq!(
                        bgra(&pixels, side / 2, side / 2)[3],
                        0,
                        "stroke has no center fill"
                    );
                }
            }
        }
    }
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn gradient_interpolates_premultiplied_colors_without_transparent_rgb_fringes() {
    let (device, queue) = gpu_device();
    let rect = Rect::from_min_size(Point::ZERO, Size::new(32.0, 32.0));
    let paint = test_gradient(
        Point::ZERO,
        Point::new(1.0, 0.0),
        &[
            (
                0.0,
                Color {
                    a: 0.0,
                    ..Color::RED
                },
            ),
            (
                1.0,
                Color {
                    a: 0.8,
                    ..Color::BLUE
                },
            ),
        ],
    );
    let ramp = make_rounded_item(
        2,
        Primitive::PaintedQuad {
            rect,
            paint,
            corner_radii: [0.0; 4],
        },
    );
    let viewport = Viewport::new(32, 32, 1.0);
    let pixels = render_quad_item(&device, &queue, ramp.clone(), viewport.clone(), 32);
    let pixel = bgra(&pixels, 16, 16);
    assert!(
        (i32::from(pixel[3]) - 105).abs() <= 1,
        "alpha ramp: {pixel:?}"
    );
    assert_eq!(pixel[2], 0, "transparent red contributes no fringe");
    assert_eq!(pixel[0], pixel[3], "blue is premultiplied exactly once");
    let green = make_rounded_item(
        1,
        Primitive::Quad {
            rect,
            color: Color::GREEN,
            corner_radius: 0.0,
        },
    );
    let pixels = render_quad_items(&device, &queue, vec![green, ramp], viewport, 32, None);
    let pixel = bgra(&pixels, 16, 16);
    assert!((i32::from(pixel[0]) - 105).abs() <= 1 && (i32::from(pixel[1]) - 150).abs() <= 1);
    assert_eq!(
        pixel[2..],
        [0, 255],
        "background remains visible through ramp"
    );
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn gradient_table_survives_growth_retained_edits_slot_reuse_and_viewport_refresh() {
    let (device, queue) = gpu_device();
    let rect = Rect::from_min_size(Point::ZERO, Size::new(32.0, 32.0));
    let make = |id, colors: [Color; 2]| {
        make_rounded_item(
            id,
            Primitive::PaintedQuad {
                rect,
                paint: test_gradient(
                    Point::ZERO,
                    Point::new(0.0, 1.0),
                    &[(0.25, colors[0]), (0.75, colors[1])],
                ),
                corner_radii: [0.0; 4],
            },
        )
    };
    let viewport = Viewport::new(32, 32, 1.0);
    let mut renderer = QuadRenderer::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let mut items: Vec<_> = (1..=300)
        .map(|id| {
            make(
                id,
                if id == 1 {
                    [Color::RED, Color::GREEN]
                } else {
                    [Color::BLUE, Color::WHITE]
                },
            )
        })
        .collect();
    renderer.update(
        &queue,
        &SceneDelta {
            added: items.clone(),
            removed: vec![],
            modified: vec![],
        },
        &viewport,
    );
    assert_eq!(
        bgra(
            &render_retained_quads(&device, &queue, &renderer, 32, &[1]),
            16,
            2
        ),
        [0, 0, 255, 255]
    );
    assert_eq!(
        bgra(
            &render_retained_quads(&device, &queue, &renderer, 32, &[300]),
            16,
            2
        ),
        [255, 0, 0, 255]
    );
    items[0] = make(1, [Color::BLACK, Color::WHITE]);
    renderer.update(
        &queue,
        &SceneDelta {
            removed: vec![300],
            modified: vec![items[0].clone()],
            added: vec![],
        },
        &viewport,
    );
    let solid = make_rounded_item(
        400,
        Primitive::PaintedQuad {
            rect,
            paint: Color::GREEN.into(),
            corner_radii: [0.0; 4],
        },
    );
    renderer.update(
        &queue,
        &SceneDelta {
            added: vec![solid],
            removed: vec![],
            modified: vec![],
        },
        &viewport,
    );
    assert_eq!(
        bgra(
            &render_retained_quads(&device, &queue, &renderer, 32, &[400]),
            16,
            16
        ),
        [0, 255, 0, 255]
    );
    assert_eq!(
        bgra(
            &render_retained_quads(&device, &queue, &renderer, 32, &[1]),
            16,
            2
        ),
        [0, 0, 0, 255]
    );
    renderer.refresh_viewport(&queue, &items[..299], &Viewport::new(64, 64, 2.0));
    let pixels = render_retained_quads(&device, &queue, &renderer, 64, &[1]);
    assert_eq!(bgra(&pixels, 32, 60), [255; 4]);
    assert_eq!(bgra(&pixels, 32, 4), [0, 0, 0, 255]);
}
