use super::*;

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn lucide_icons_render_at_multiple_dpi_with_tint_and_single_coverage() {
    let (device, queue) = gpu_device();
    eprintln!("GPU available: executing Lucide pixel assertions");
    use harbor_widget::LucideIcon;
    for scale in [1.0, 1.5, 2.0] {
        let target_size = (32.0 * scale) as u32;
        for (icon, sample) in [
            (LucideIcon::Terminal, (16.0, 19.0)),
            (LucideIcon::Plus, (12.0, 12.0)),
            (LucideIcon::X, (12.0, 12.0)),
            (LucideIcon::Dot, (12.0, 12.0)),
            (LucideIcon::ChevronUp, (12.0, 9.0)),
            (LucideIcon::ChevronDown, (12.0, 15.0)),
        ] {
            let item = make_rounded_item(
                1,
                Primitive::Icon {
                    icon,
                    rect: Rect::from_min_size(Point::new(4.0, 4.0), Size::new(24.0, 24.0)),
                    color: Color {
                        a: 0.5,
                        ..Color::RED
                    },
                },
            );
            let pixels = render_quad_item(
                &device,
                &queue,
                item,
                Viewport::new(target_size, target_size, scale),
                target_size,
            );
            let sample = bgra(
                &pixels,
                ((sample.0 + 4.0) * scale) as u32,
                ((sample.1 + 4.0) * scale) as u32,
            );
            assert!(
                sample[2] > 80 && (90..=130).contains(&sample[3]),
                "{icon:?} at {scale}x: {sample:?}"
            );
            assert_eq!(
                bgra(&pixels, (5.0 * scale) as u32, (5.0 * scale) as u32),
                [0; 4]
            );
        }
    }
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn lucide_icon_uses_runtime_frame_encoding_and_ancestor_clip() {
    let (device, queue) = gpu_device();
    let pixels = encode_root_to_pixels(
        &device,
        &queue,
        harbor_widget::Icon::new(harbor_widget::LucideIcon::Plus)
            .size(24.0)
            .expect("valid icon size")
            .foreground(Color::GREEN)
            .expect("valid color"),
        Viewport::new(24, 24, 1.0),
        24,
    );
    assert!(bgra(&pixels, 12, 12)[1] > 200);
    assert_eq!(bgra(&pixels, 2, 2), [0; 4]);

    let mut item = make_rounded_item(
        1,
        Primitive::Icon {
            icon: harbor_widget::LucideIcon::Plus,
            rect: Rect::from_min_size(Point::ZERO, Size::new(24.0, 24.0)),
            color: Color::GREEN,
        },
    );
    item.clips.push(
        RoundedClip::new(
            Rect::from_min_size(Point::new(12.0, 0.0), Size::new(12.0, 24.0)),
            BorderRadius::default(),
            ClipBehavior::HardEdge,
        )
        .unwrap(),
    );
    let pixels = render_quad_item(&device, &queue, item, Viewport::new(32, 32, 1.0), 32);
    assert_eq!(bgra(&pixels, 8, 12), [0; 4]);
    assert!(bgra(&pixels, 16, 12)[1] > 200);
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn lucide_axis_strokes_match_pixel_coverage_without_a_blur_halo() {
    let (device, queue) = gpu_device();
    for side in [24.0, 20.0, 16.0] {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for phase in [0.0, 0.25, 0.5] {
                let origin = 4.0 + phase;
                let target_size = (32.0 * scale) as u32;
                let item = make_rounded_item(
                    1,
                    Primitive::Icon {
                        icon: harbor_widget::LucideIcon::Plus,
                        rect: Rect::from_min_size(
                            Point::new(origin, origin),
                            Size::new(side, side),
                        ),
                        color: Color::WHITE,
                    },
                );
                let pixels = render_quad_item(
                    &device,
                    &queue,
                    item,
                    Viewport::new(target_size, target_size, scale),
                    target_size,
                );
                // Sample away from caps and the crossing. A 2/24-wide SVG stroke
                // must cover exactly its geometric intersection with each pixel.
                let x = ((origin + side * 2.0 / 3.0) * scale) as u32;
                let center = (origin + side / 2.0) * scale;
                let half_width = side * scale / 24.0;
                let lo = center - half_width;
                let hi = center + half_width;
                for y in (lo.floor() as u32 - 1)..=(hi.ceil() as u32 + 1) {
                    let expected = ((hi.min(y as f32 + 1.0) - lo.max(y as f32)).clamp(0.0, 1.0)
                        * 255.0)
                        .round() as i32;
                    let actual = i32::from(bgra(&pixels, x, y)[3]);
                    assert!(
                        (actual - expected).abs() <= 4,
                        "side={side}, scale={scale}, phase={phase}, pixel=({x},{y}): alpha={actual}, expected={expected}"
                    );
                }
            }
        }
    }
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn transparent_icon_button_has_no_zero_width_border_fill() {
    let (device, queue) = gpu_device();
    let mut style = harbor_widget::Theme::default().icon_button;
    style.min_height = 32.0;
    style.border_width = 0.0;
    for colors in [
        &mut style.normal,
        &mut style.hovered,
        &mut style.pressed,
        &mut style.disabled,
        &mut style.selected,
    ] {
        colors.background = Color::TRANSPARENT;
    }
    let mut runtime = Runtime::new();
    runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    runtime.set_root(
        Stack::new()
            .push_child(
                SizedBox::new(Size::new(32.0, 32.0))
                    .unwrap()
                    .background(Color::GREEN)
                    .expect("valid color"),
            )
            .push_child(
                harbor_widget::IconButton::new(harbor_widget::LucideIcon::Plus, "transparent")
                    .style(style)
                    .expect("valid style"),
            ),
    );
    let viewport = Viewport::new(32, 32, 1.0);
    runtime.set_viewport(viewport.clone());
    runtime.update(Instant::now());
    for event in [
        None,
        Some(harbor_widget::input::event::PointerPhase::Move),
        Some(harbor_widget::input::event::PointerPhase::Down),
        Some(harbor_widget::input::event::PointerPhase::Up),
    ] {
        if let Some(phase) = event {
            runtime.dispatch(harbor_widget::input::event::UiEvent::Pointer(
                harbor_widget::input::event::PointerEvent::new(
                    Point::new(16.0, 16.0),
                    phase,
                    harbor_widget::input::event::PointerButton::Left,
                    7,
                ),
            ));
            runtime.update(Instant::now());
        }
        let pixels = encode_filter_frame(&device, &queue, &mut runtime, viewport.clone());
        assert_eq!(
            bgra(&pixels, 8, 8),
            [0, 255, 0, 255],
            "transparent idle/hover/pressed button is not a fill"
        );
    }
}
