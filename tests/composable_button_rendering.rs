//! Explicit renderer/font acceptance samples for spec 0037, not a native/HMR substitute.
//! Run: cargo test --test composable_button_rendering -- --ignored --nocapture
//! Writes target/f4-buttons/samples.png (columns: 320, 150, 40dp; see test row order).
use harbor_widget::{
    Button, ConstrainedBox, LucideIcon, Theme,
    input::event::{PointerButton, PointerEvent, PointerPhase, UiEvent},
    layout::{Point, Size},
    renderer::Viewport,
    runtime::Runtime,
    scene::primitive::{Color, ExternalDrawGpu, Primitive},
    view::Component,
    widgets::text_label::TextLabel,
};
use std::time::Instant;

fn scene(rt: &Runtime) -> Vec<Primitive> {
    let delta = rt.pending_delta().expect("painted scene");
    delta
        .added
        .iter()
        .chain(&delta.modified)
        .map(|item| item.primitive.clone())
        .collect()
}
fn root_size(rt: &Runtime) -> Size {
    rt.arena()
        .get(rt.root_id().unwrap())
        .unwrap()
        .layout_rect()
        .unwrap()
        .size()
}
fn text(rt: &Runtime) -> String {
    scene(rt)
        .iter()
        .find_map(|p| match p {
            Primitive::Text { text, .. } => Some(text.to_string()),
            _ => None,
        })
        .unwrap_or_default()
}
fn pointer(rt: &mut Runtime, phase: PointerPhase) {
    rt.dispatch(UiEvent::Pointer(PointerEvent::new(
        Point::new(1.0, 1.0),
        phase,
        PointerButton::Left,
        0,
    )));
    rt.update(Instant::now());
}

fn capture(
    rt: &mut Runtime,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    width: u32,
) -> image::RgbaImage {
    const HEIGHT: u32 = 44;
    let viewport = Viewport::new(width, HEIGHT, 1.0);
    rt.prepare_text(queue);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("F4 samples"),
        size: wgpu::Extent3d {
            width,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("F4 render"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.08,
                        g: 0.08,
                        b: 0.08,
                        a: 1.0,
                    }),
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
    let stride = (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("F4 pixels"),
        size: u64::from(stride * HEIGHT),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
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
                bytes_per_row: Some(stride),
                rows_per_image: Some(HEIGHT),
            },
        },
        wgpu::Extent3d {
            width,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let (send, recv) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            send.send(result).unwrap()
        });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    recv.recv().unwrap().unwrap();
    let data = buffer.slice(..).get_mapped_range().unwrap();
    let pixels = image::RgbaImage::from_fn(width, HEIGHT, |x, y| {
        let offset = (y * stride + x * 4) as usize;
        image::Rgba([
            data[offset + 2],
            data[offset + 1],
            data[offset],
            data[offset + 3],
        ])
    });
    drop(data);
    buffer.unmap();
    assert!(
        pixels.pixels().any(|p| p.0[0] > 30),
        "actual control pixels rendered"
    );
    pixels
}

#[test]
#[ignore = "requires a GPU and system UI fonts; explicitly generates visual evidence"]
fn measured_button_geometry_and_rendered_samples() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .expect("F4 requires GPU evidence");
    eprintln!("F4 adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
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
                    _ => Button::new().content(move |cx, state| {
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
