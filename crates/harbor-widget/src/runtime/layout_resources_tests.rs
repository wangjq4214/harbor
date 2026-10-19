//! GPU-backed evidence for layout-only resize: compare actual retained handles.
use crate::layout::{Alignment, Rect};
use crate::renderer::Viewport;
use crate::runtime::Runtime;
use crate::scene::primitive::{Color, ExternalDrawFn, ExternalDrawGpu, Primitive};
use crate::widgets::custom_paint::CustomPaint;
use crate::widgets::text_label::TextLabel;
use crate::{
    BorderRadius, BoxDecoration, ClipBehavior, ConstrainedBox, DecoratedBox, Expanded, Row,
    Separator,
};
use std::sync::{Arc, Mutex};
use std::time::Instant;

fn glyph(_: char) -> Option<crate::text::AtlasGlyph> {
    use harbor_text::{FaceId, FontSize, FontStyle, GlyphId, GlyphKey};
    Some(crate::text::AtlasGlyph {
        key: GlyphKey::new(
            FaceId::PRIMARY,
            GlyphId::new(0),
            FontSize::new(8.0).unwrap(),
            FontStyle::REGULAR,
        ),
        uv: crate::text::AtlasUv {
            left: 0.0,
            top: 0.0,
            right: 1.0,
            bottom: 1.0,
        },
        width: 8,
        height: 8,
        bearing_x: 0,
        bearing_y: 8,
        advance_width: 8.0,
        atlas_x: 0,
        atlas_y: 0,
    })
}

fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .ok()?;
    eprintln!("layout resource adapter: {:?}", adapter.get_info());
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
}

fn atlas(device: &wgpu::Device) -> (wgpu::Texture, wgpu::BindGroupLayout, wgpu::BindGroup) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("layout-test-atlas"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("layout-test-atlas-layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("layout-test-atlas-group"),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    (texture, layout, group)
}

fn encode(rt: &mut Runtime, device: &wgpu::Device, queue: &wgpu::Queue, viewport: Viewport) {
    // Offscreen target is test infrastructure, not a Runtime allocation.
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("layout-test-target"),
        size: wgpu::Extent3d {
            width: viewport.physical_size.0.max(1),
            height: viewport.physical_size.1.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("layout-test-pass"),
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
        rt.encode(
            ExternalDrawGpu::new(device, queue, wgpu::TextureFormat::Bgra8Unorm),
            &mut pass,
            viewport,
            true,
        );
    }
    queue.submit([encoder.finish()]);
}

#[test]
fn should_retain_gpu_handles_and_external_allocations_across_flex_resize_and_dpi() {
    let Some((device, queue)) = device() else {
        assert!(
            std::env::var_os("HARBOR_REQUIRE_LAYOUT_GPU").is_none(),
            "required layout GPU evidence unavailable"
        );
        eprintln!("SKIP: layout resource evidence requires a GPU adapter");
        return;
    };
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
            .cross_axis_alignment(Alignment::Stretch)
            .child(
                ConstrainedBox::new()
                    .min_width(200.0)
                    .max_width(200.0)
                    .child(TextLabel::new("Rail")),
            )
            .child(Separator::vertical().color(Color::WHITE))
            .child(
                Expanded::new().child(
                    DecoratedBox::new(
                        BoxDecoration::new().border_radius(BorderRadius::all(4.0).unwrap()),
                    )
                    .clip_behavior(ClipBehavior::HardEdge)
                    .child(CustomPaint::new(99).handler(Arc::clone(&handler))),
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
fn default_label_layout_matches_rendered_ui_advances_across_dpi() {
    use crate::decoration::Border;
    use crate::widgets::padding::Padding;

    let (device, queue) = device().expect("GPU adapter required for real UI-font regression");
    let mut rt = Runtime::new();
    rt.init_text_renderer(&device, &queue, wgpu::TextureFormat::Bgra8Unorm)
        .unwrap();
    let mut row = Row::new().gap(4.0);
    for key in ["Ctrl", "Shift", "Tab", "R"] {
        row = row.child(
            DecoratedBox::new(BoxDecoration::new().border(Border::all(Color::WHITE, 1.0).unwrap()))
                .child(Padding::new(1.0, 2.0, 1.0, 2.0).child(TextLabel::new(key))),
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
fn default_label_ellipsis_fits_rendered_ui_text_and_restores_after_resize() {
    use unicode_segmentation::UnicodeSegmentation;

    let (device, queue) = device().expect("GPU adapter required for real UI-font regression");
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
fn notification_wrapping_fills_real_ui_width_and_caps_details_across_dpi() {
    use crate::widgets::notification::NotificationPlacement;
    use crate::{LucideIcon, Notification};
    use unicode_segmentation::UnicodeSegmentation;

    let (device, queue) =
        device().expect("GPU adapter required for notification UI-font regression");
    let mut rt = Runtime::new();
    rt.init_text_renderer(&device, &queue, wgpu::TextureFormat::Bgra8Unorm)
        .unwrap();
    let source = "Wi thin and wide glyphs 项目 e\u{0301} 👩\u{200d}💻 ".repeat(100);
    rt.set_root(
        Notification::new(source.clone(), || {})
            .title("Configuration reload")
            .detail_color(Color {
                a: 0.82,
                ..Color::WHITE
            })
            .max_lines(4)
            .close_alignment(Alignment::Start)
            .placement(NotificationPlacement {
                horizontal: Alignment::End,
                vertical: Alignment::Start,
                inset: 12.0,
            }),
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
