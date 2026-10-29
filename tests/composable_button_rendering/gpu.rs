use super::*;

#[test]
#[ignore = "requires a GPU and system UI fonts; explicitly generates visual evidence"]
fn measured_button_geometry_and_rendered_samples() {
    let (device, queue) = test_gpu::device();
    let mut rt = Runtime::new();
    rt.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    rt.init_text_renderer(&device, &queue, wgpu::TextureFormat::Bgra8Unorm)
        .expect("system UI font resources");
    let style = Theme::default().button;
    let mut widths = Vec::new();
    for source in [
        "iiii",
        "WWWW",
        "中文长标题测试",
        "WWii 中文 e\u{301} long button title",
    ] {
        rt.set_viewport(Viewport::new(1000, 100, 1.0));
        rt.set_root(TextLabel::new(source));
        rt.update(Instant::now());
        let measured = root_size(&rt).width;
        widths.push(measured);
        for icon in [false, true] {
            let button = if icon {
                Button::icon(LucideIcon::Plus, source)
            } else {
                Button::text(source)
            };
            rt.set_root(button);
            rt.update(Instant::now());
            let extra = if icon {
                style.icon_size + Theme::default().spacing
            } else {
                0.0
            };
            assert!(
                (root_size(&rt).width - (measured + style.horizontal_padding * 2.0 + extra)).abs()
                    < 0.01
            );
            assert_eq!(text(&rt), source);
            rt.set_viewport(Viewport::new(80, 100, 1.0));
            rt.update(Instant::now());
            if measured + 2.0 * style.horizontal_padding + extra > 80.0 {
                let narrow = text(&rt);
                assert!(narrow.is_empty() || narrow.ends_with('…'));
            }
            rt.set_viewport(Viewport::new(1000, 100, 1.0));
            rt.update(Instant::now());
            assert_eq!(text(&rt), source, "layout-only resize recovers source");
            eprintln!(
                "F4 measured {source:?} icon={icon}: text={measured:.2} control={:.2}",
                root_size(&rt).width
            );
        }
    }
    assert!(
        widths[1] > widths[0],
        "real UI renderer has unequal W/i advances"
    );

    // Four groups: text, icon+text, explicit custom child, state-driven custom content.
    // Within each group: normal, hovered, pressed, selected, disabled, keyboard focus.
    let mut sheet = image::RgbaImage::from_pixel(960, 27 * 48, image::Rgba([20, 20, 20, 255]));
    for (column, width) in [320, 150, 40].into_iter().enumerate() {
        for form in 0..4 {
            for state in 0..6 {
                let source = "WWii 中文 e\u{301} long button title";
                let button = match form {
                    0 => Button::text(source),
                    1 => Button::icon(LucideIcon::Plus, source),
                    2 => Button::new().child(
                        TextLabel::new(source)
                            .ellipsis()
                            .foreground(Color::GREEN)
                            .expect("valid color"),
                    ),
                    _ => Button::new().child_with(move |cx, state| {
                        TextLabel::new(if state.pressed {
                            "Pressed 中文"
                        } else {
                            source
                        })
                        .ellipsis()
                        .foreground(Color::BLUE)
                        .expect("valid color")
                        .build(cx)
                    }),
                }
                .selected(state == 3)
                .disabled(state == 4);
                // Fresh runtime tree for each sample, sharing the real font resources.
                let mut sample = rt.create_child_runtime(&device, wgpu::TextureFormat::Bgra8Unorm);
                sample.set_viewport(Viewport::new(width, 44, 1.0));
                sample.set_root(
                    ConstrainedBox::new()
                        .min_width(width as f32)
                        .min_height(44.0)
                        .child(button),
                );
                sample.update(Instant::now());
                if state == 1 || state == 2 {
                    pointer(&mut sample, PointerPhase::Move);
                }
                if state == 2 {
                    pointer(&mut sample, PointerPhase::Down);
                }
                if state == 5 {
                    sample.dispatch(UiEvent::Keyboard(
                        harbor_widget::input::event::KeyboardEvent::KeyDown {
                            key: harbor_widget::input::event::Key::Tab,
                            modifiers: Default::default(),
                        },
                    ));
                    sample.update(Instant::now());
                    assert!(sample.input().focused().is_some());
                    assert!(
                        scene(&sample)
                            .iter()
                            .any(|p| matches!(p, Primitive::Border { color, .. }
                        if *color == Theme::default().button.focus_ring))
                    );
                }
                let pixels = capture(&mut sample, &device, &queue, width);
                image::imageops::replace(
                    &mut sheet,
                    &pixels,
                    column as i64 * 320,
                    (form * 6 + state) * 48,
                );
            }
        }
        for (row, source) in [
            "A very long text-only title with unequal WWW iii advances",
            "中文长标题测试中文长标题测试",
            "Mixed 中文 e\u{301} 👩‍💻 title",
        ]
        .into_iter()
        .enumerate()
        {
            let mut sample = rt.create_child_runtime(&device, wgpu::TextureFormat::Bgra8Unorm);
            sample.set_viewport(Viewport::new(width, 44, 1.0));
            sample.set_root(Button::text(source));
            sample.update(Instant::now());
            let pixels = capture(&mut sample, &device, &queue, width);
            image::imageops::replace(
                &mut sheet,
                &pixels,
                column as i64 * 320,
                (24 + row) as i64 * 48,
            );
        }
    }
    std::fs::create_dir_all("target/f4-buttons").unwrap();
    sheet.save("target/f4-buttons/samples.png").unwrap();
}
