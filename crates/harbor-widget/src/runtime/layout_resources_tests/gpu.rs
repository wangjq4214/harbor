use super::*;

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_retain_gpu_handles_and_external_allocations_across_flex_resize_and_dpi() {
    let (device, queue) = gpu_device();
    let (_texture, layout, group) = atlas(&device);
    let draws = Arc::new(Mutex::new(Vec::<Rect>::new()));
    let recorded = Arc::clone(&draws);
    let handler: Arc<ExternalDrawFn<'static>> = Arc::new(move |_, context, _, _, _| {
        recorded.lock().unwrap().push(context.logical_rect);
    });
    let mut rt = Runtime::new();
    rt.set_viewport(Viewport::new(1000, 600, 1.0));
    rt.set_root(
        Row::new()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .push_child(
                ConstrainedBox::new()
                    .min_width(200.0)
                    .max_width(200.0)
                    .child(TextLabel::new("Rail")),
            )
            .push_child(
                Separator::vertical()
                    .color(Color::WHITE)
                    .expect("valid color"),
            )
            .push_child(
                Expanded::new().child(
                    DecoratedBox::new(
                        BoxDecoration::new().border_radius(BorderRadius::all(4.0).unwrap()),
                    )
                    .clip_behavior(ClipBehavior::HardEdge)
                    .child(ExternalSurface::new(99).on_draw(Arc::clone(&handler))),
                ),
            ),
    );
    rt.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    rt.init_text_renderer_with_bind_group(
        &device,
        wgpu::TextureFormat::Bgra8Unorm,
        &layout,
        &group,
    );
    rt.update(Instant::now());
    rt.prepare_text_runs(&glyph);
    encode(&mut rt, &device, &queue, Viewport::new(1000, 600, 1.0));
    assert!(
        rt.pending_delta.is_none(),
        "initial delta must actually be consumed"
    );
    let quad = rt.encoder.renderer.as_ref().unwrap().resource_handles();
    let text = rt
        .encoder
        .text_renderer
        .as_ref()
        .unwrap()
        .resource_handles();
    let ids: Vec<_> = rt.scene_graph.items().iter().map(|item| item.id).collect();
    let rail_text_id = rt
        .scene_graph
        .items()
        .iter()
        .find(|item| matches!(item.primitive, Primitive::Text { .. }))
        .unwrap()
        .id;
    for viewport in [
        Viewport::new(1250, 750, 1.25), // same logical allocation, physical-only transition
        Viewport::new(1001, 601, 1.25),
        Viewport::new(1001, 601, 1.5),
        Viewport::new(1001, 601, 2.0),
        Viewport::new(0, 0, 1.0),
        Viewport::new(1000, 600, 1.0),
    ] {
        assert!(rt.set_viewport(viewport.clone()));
        rt.update(Instant::now());
        assert_eq!(
            rt.scene_graph
                .items()
                .iter()
                .map(|item| item.id)
                .collect::<Vec<_>>(),
            ids
        );
        let delta = rt.pending_delta.as_ref().unwrap();
        assert!(delta.added.is_empty() && delta.removed.is_empty());
        assert!(
            delta.modified.iter().all(|item| item.id != rail_text_id),
            "unmoved rail text must remain unchanged"
        );
        if viewport.physical_size == (1250, 750) {
            assert!(
                delta.modified.is_empty(),
                "scale-only transition has identical logical scene"
            );
        }
        assert!(Arc::ptr_eq(rt.external_draws.get(&99).unwrap(), &handler));
        let allocation = rt
            .scene_graph
            .items()
            .iter()
            .find_map(|item| match item.primitive {
                Primitive::External { draw, rect } if draw == 99 => Some(rect),
                _ => None,
            })
            .unwrap();
        let prior_draws = draws.lock().unwrap().len();
        rt.prepare_text_runs(&glyph);
        encode(&mut rt, &device, &queue, viewport.clone());
        assert_eq!(
            rt.encoder.renderer.as_ref().unwrap().resource_handles(),
            quad
        );
        assert_eq!(
            rt.encoder
                .text_renderer
                .as_ref()
                .unwrap()
                .resource_handles(),
            text
        );
        assert_eq!(rt.encoder.encoded_viewport.as_ref(), Some(&viewport));
        if viewport.physical_size == (0, 0) {
            assert_eq!(draws.lock().unwrap().len(), prior_draws);
        } else {
            assert_eq!(draws.lock().unwrap().last(), Some(&allocation));
        }
        assert!(!rt.set_viewport(viewport));
        assert!(!rt.update(Instant::now()).request_redraw);
    }
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    eprintln!(
        "PASS: retained quad/text pipelines, buffers, atlas binding and external handler through resize/DPI/zero/restore"
    );
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn default_label_layout_matches_rendered_ui_advances_across_dpi() {
    use crate::decoration::Border;
    use crate::widgets::padding::Padding;

    let (device, queue) = gpu_device();
    let mut rt = Runtime::new();
    rt.init_text_renderer(&device, &queue, wgpu::TextureFormat::Bgra8Unorm)
        .unwrap();
    let mut row = Row::new().gap(4.0).unwrap();
    for key in ["Ctrl", "Shift", "Tab", "R"] {
        row = row.push_child(
            DecoratedBox::new(BoxDecoration::new().border(Border::all(Color::WHITE, 1.0).unwrap()))
                .child(
                    Padding::new(crate::Insets {
                        top: 1.0,
                        right: 2.0,
                        bottom: 1.0,
                        left: 2.0,
                    })
                    .unwrap()
                    .child(TextLabel::new(key)),
                ),
        );
    }
    rt.set_root(row);
    for scale in [1.0, 1.5, 2.0, 1.0] {
        rt.set_viewport(Viewport::new(
            (600.0 * scale) as u32,
            (100.0 * scale) as u32,
            scale,
        ));
        rt.update(Instant::now());
        rt.prepare_text(&queue);
        let items = rt.scene_graph.items().to_vec();
        let mut checked = 0;
        for item in &items {
            let Primitive::RoundedBorder { rect, .. } = item.primitive else {
                continue;
            };
            let text = items.iter().find(|item| matches!(&item.primitive,
                Primitive::Text { origin, .. } if *origin == crate::layout::Point::new(rect.min.x + 2.0, rect.min.y + 1.0)
            )).expect("text inside keycap");
            let advance = rt.text_run_cache().get(text.id).unwrap().advance_width;
            assert!(
                (rect.size().width - (advance + 4.0)).abs() < 0.01,
                "at scale {scale}, keycap width {} must match rendered text advance {advance} plus 4dp; {:?}",
                rect.size().width,
                text.primitive
            );
            checked += 1;
        }
        assert_eq!(checked, 4);
    }
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn default_label_ellipsis_fits_rendered_ui_text_and_restores_after_resize() {
    use unicode_segmentation::UnicodeSegmentation;

    let (device, queue) = gpu_device();
    let mut rt = Runtime::new();
    rt.init_text_renderer(&device, &queue, wgpu::TextureFormat::Bgra8Unorm)
        .unwrap();
    for source in [
        "iiiiWWWWiiii",
        "项目标签 WiWiWi",
        "e\u{0301}WiWiWi",
        "👩\u{200d}💻WiWiWi",
    ] {
        rt.set_root(TextLabel::new(source).ellipsis());
        for scale in [1.0, 1.5, 2.0] {
            for width in [9, 23, 55, 500, 23] {
                let viewport = Viewport::new((width as f32 * scale) as u32, 100, scale);
                let available = viewport.logical_size.width;
                rt.set_viewport(viewport);
                rt.update(Instant::now());
                rt.prepare_text(&queue);
                let text = rt
                    .scene_graph
                    .items()
                    .iter()
                    .find_map(|item| match &item.primitive {
                        Primitive::Text { text, .. } => Some((item.id, text.to_string())),
                        _ => None,
                    });
                let measure = |rt: &Runtime, text: &str| {
                    text.chars()
                        .map(|ch| {
                            rt.encoder
                                .advance_width(ch, scale, rt.text_metrics.cell_width)
                        })
                        .sum::<f32>()
                };
                let full_width = measure(&rt, source);
                let ellipsis_width = measure(&rt, "…");
                if let Some((id, text)) = text {
                    let actual = rt.text_run_cache().get(id).unwrap().advance_width;
                    assert!(
                        actual <= available + 0.001,
                        "{text:?} renders past {available} at scale {scale}"
                    );
                    if full_width <= available {
                        assert_eq!(text, source, "widening restores full source");
                    } else {
                        let prefix = text
                            .strip_suffix('…')
                            .expect("overflow uses measured ellipsis");
                        assert!(source.starts_with(prefix));
                        assert!(
                            source
                                .grapheme_indices(true)
                                .any(|(end, _)| end == prefix.len())
                        );
                        let next = source[prefix.len()..].graphemes(true).next().unwrap();
                        let longer = format!("{prefix}{next}…");
                        assert!(
                            measure(&rt, &longer) > available,
                            "must retain the longest fitting grapheme prefix"
                        );
                    }
                } else {
                    assert!(
                        ellipsis_width > available,
                        "only omit text when even ellipsis cannot fit"
                    );
                }
            }
        }
    }
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn notification_wrapping_fills_real_ui_width_and_caps_details_across_dpi() {
    use crate::widgets::notification_card::NotificationCardPlacement;
    use crate::{LucideIcon, NotificationCard};
    use unicode_segmentation::UnicodeSegmentation;

    let (device, queue) = gpu_device();
    let mut rt = Runtime::new();
    rt.init_text_renderer(&device, &queue, wgpu::TextureFormat::Bgra8Unorm)
        .unwrap();
    let source = "Wi thin and wide glyphs 项目 e\u{0301} 👩\u{200d}💻 ".repeat(100);
    rt.set_root(
        NotificationCard::new(source.clone(), || {})
            .title("Configuration reload")
            .detail_foreground(Color {
                a: 0.82,
                ..Color::WHITE
            })
            .unwrap()
            .max_lines(4)
            .close_alignment(crate::PositionAlignment::Start)
            .placement(NotificationCardPlacement {
                horizontal: crate::PositionAlignment::End,
                vertical: crate::PositionAlignment::Start,
                inset: 12.0,
            })
            .unwrap(),
    );
    for scale in [1.0, 1.5, 2.0, 1.0] {
        for width in [600.0, 300.0, 600.0] {
            rt.set_viewport(Viewport::new(
                (width * scale) as u32,
                (300.0 * scale) as u32,
                scale,
            ));
            rt.update(Instant::now());
            rt.prepare_text(&queue);
            let items = rt.scene_graph.items().to_vec();
            let card = items
                .iter()
                .find_map(|item| match item.primitive {
                    Primitive::Quad {
                        rect,
                        corner_radius: 6.0,
                        ..
                    } => Some(rect),
                    _ => None,
                })
                .unwrap();
            let available = card.size().width - 24.0 - 36.0;
            let text: Vec<_> = items
                .iter()
                .filter_map(|item| match &item.primitive {
                    Primitive::Text {
                        text,
                        origin,
                        color,
                    } => Some((item.id, text.as_ref(), *origin, *color)),
                    _ => None,
                })
                .collect();
            assert_eq!(text.len(), 5, "one title and four detail lines");
            assert_eq!(text[0].1, "Configuration reload");
            assert!(text[0].3.a > text[1].3.a, "title is visually distinct");
            let close = items
                .iter()
                .find_map(|item| match item.primitive {
                    Primitive::Icon {
                        icon: LucideIcon::X,
                        rect,
                        ..
                    } => Some(rect),
                    _ => None,
                })
                .unwrap();
            assert!(
                (close.min.y - card.min.y - (32.0 - close.size().height) / 2.0).abs() < 0.01,
                "close glyph centered within the top header row"
            );
            let measure = |rt: &Runtime, text: &str| {
                text.chars()
                    .map(|ch| {
                        rt.encoder
                            .advance_width(ch, scale, rt.text_metrics.cell_width)
                    })
                    .sum::<f32>()
            };
            let mut consumed = 0;
            for (i, &(id, line, origin, _)) in text.iter().enumerate().skip(1) {
                let advance = rt.text_run_cache().get(id).unwrap().advance_width;
                assert!(
                    advance <= available + 0.01,
                    "detail must not invade close lane"
                );
                assert!(origin.y + rt.text_metrics.line_height <= card.max.y + 0.01);
                if i < 4 {
                    assert!(source[consumed..].starts_with(line));
                    consumed += line.len();
                    let next = source[consumed..].graphemes(true).next().unwrap();
                    assert!(
                        advance + measure(&rt, next) > available - 0.01,
                        "each non-final line uses the full rendered width, not a cell estimate"
                    );
                } else {
                    assert!(line.ends_with('…'), "capped detail signals truncation");
                }
            }
            assert!(card.size().height <= 32.0 + 12.0 + 4.0 * rt.text_metrics.line_height);
        }
    }
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn uncapped_label_wrap_uses_rendered_advances_and_preserves_unicode_across_resize() {
    use crate::view::{BuildCx, Component};
    use std::cell::Cell;
    use std::rc::Rc;
    use unicode_segmentation::UnicodeSegmentation;

    let (device, queue) = gpu_device();
    let mut rt = Runtime::new();
    rt.init_text_renderer(&device, &queue, wgpu::TextureFormat::Bgra8Unorm)
        .unwrap();
    let source = "iiiiWWWWiiii 项目 e\u{0301} 👩\u{200d}💻\nFull final warning without truncation.";
    let builds = Rc::new(Cell::new(0));
    let observed = builds.clone();
    rt.set_root(move |cx: &mut BuildCx| {
        observed.set(observed.get() + 1);
        TextLabel::new(source)
            .wrap()
            .foreground(Color::GREEN)
            .unwrap()
            .build(cx)
    });
    let root = rt.root_id().unwrap();
    for scale in [1.0, 1.5, 2.0] {
        let measure = |rt: &Runtime, text: &str| {
            text.chars()
                .map(|ch| {
                    rt.encoder
                        .advance_width(ch, scale, rt.text_metrics.cell_width)
                })
                .sum::<f32>()
        };
        assert!(
            measure(&rt, "W") > measure(&rt, "i"),
            "real proportional UI font"
        );
        for width in [4, 23, 70, 500, 23] {
            let viewport = Viewport::new((width as f32 * scale) as u32, 4000, scale);
            let available = viewport.logical_size.width;
            rt.set_viewport(viewport);
            rt.update(Instant::now());
            rt.prepare_text(&queue);
            let lines: Vec<_> = rt
                .scene_graph
                .items()
                .iter()
                .filter_map(|item| match &item.primitive {
                    Primitive::Text {
                        text,
                        origin,
                        color,
                    } => {
                        assert_eq!(*color, Color::GREEN);
                        Some((item.id, text.to_string(), *origin))
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(
                lines
                    .iter()
                    .map(|(_, text, _)| text.as_str())
                    .collect::<String>(),
                source.replace('\n', "")
            );
            assert_eq!(rt.root_id(), Some(root));
            assert_eq!(
                builds.get(),
                1,
                "resize must re-layout retained source, not rebuild it"
            );
            assert!(rt.arena.get(root).unwrap().layout_error().is_none());
            let rect = rt.arena.get(root).unwrap().layout_rect().unwrap();
            assert!(
                (rect.size().height - lines.len() as f32 * rt.text_metrics.line_height).abs()
                    < 0.01
            );
            for (index, (id, text, origin)) in lines.iter().enumerate() {
                assert!(!text.contains('…'));
                let rendered = rt.text_run_cache().get(*id).unwrap().advance_width;
                assert!((rendered - measure(&rt, text)).abs() < 0.01);
                assert!(
                    rendered <= available + 0.01 || text.graphemes(true).count() == 1,
                    "only an indivisible oversized grapheme may overflow: {text:?}"
                );
                assert_eq!(origin.x, rect.min.x);
                assert!(
                    (origin.y - rect.min.y - index as f32 * rt.text_metrics.line_height).abs()
                        < 0.01
                );
            }
            if width == 500 {
                assert_eq!(
                    lines
                        .iter()
                        .map(|(_, text, _)| text.as_str())
                        .collect::<Vec<_>>(),
                    source.split('\n').collect::<Vec<_>>()
                );
            }
        }
    }
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn mixed_local_text_scales_use_matching_raster_metrics_and_encode_across_dpi() {
    let (device, queue) = gpu_device();
    let mut rt = Runtime::new();
    rt.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    rt.init_text_renderer(&device, &queue, wgpu::TextureFormat::Bgra8Unorm)
        .unwrap();
    rt.set_root(
        crate::Column::new()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .push_child(
                TextLabel::new("WiWi 项目 heading")
                    .wrap()
                    .scale(1.6)
                    .unwrap(),
            )
            .push_child(
                TextLabel::new("WiWi 项目 section")
                    .wrap()
                    .scale(1.25)
                    .unwrap(),
            )
            .push_child(TextLabel::new("WiWi 项目 body").wrap()),
    );
    for dpi in [1.0, 1.25, 1.5, 2.0, 1.0] {
        for width in [120.0, 500.0, 160.0] {
            let viewport = Viewport::new((width * dpi) as u32, (1000.0 * dpi) as u32, dpi);
            rt.set_viewport(viewport.clone());
            rt.update(Instant::now());
            rt.prepare_text(&queue);
            let items = rt.scene_graph.items().to_vec();
            for text_scale in [1.0, 1.25, 1.6] {
                let runs: Vec<_> = items
                    .iter()
                    .filter_map(|item| {
                        let (text, _, _, scale) = item.primitive.text_run()?;
                        (scale == text_scale).then_some((item.id, text))
                    })
                    .collect();
                assert!(!runs.is_empty());
                for (id, text) in runs {
                    let measured: f32 = text
                        .chars()
                        .map(|ch| {
                            rt.encoder.advance_width(
                                ch,
                                dpi * text_scale,
                                rt.text_metrics.cell_width,
                            ) * text_scale
                        })
                        .sum();
                    let run = rt.text_run_cache().get(id).unwrap();
                    assert!(
                        (run.advance_width - measured).abs() < 0.01,
                        "layout and raster advances diverged at DPI {dpi}, local scale {text_scale}"
                    );
                    assert!(run.advance_width <= width + 0.01);
                    assert!(!run.glyphs.is_empty());
                }
                let atlas = rt.encoder.text_atlas.as_ref().unwrap().borrow();
                let glyph = atlas
                    .glyph('W', dpi * text_scale)
                    .expect("every local size is retained");
                let body = atlas.glyph('W', dpi).unwrap();
                if text_scale > 1.0 {
                    assert!(
                        glyph.height > body.height,
                        "rasterize a larger glyph instead of stretching the body bitmap"
                    );
                }
            }
            encode(&mut rt, &device, &queue, viewport);
            assert!(rt.pending_delta.is_none());
        }
    }
}
