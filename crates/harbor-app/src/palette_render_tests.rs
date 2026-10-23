//! GPU coverage of real glass overlay composition, not just scene descriptors.
use super::*;
use harbor_widget::{
    layout::{BoxConstraints, Size},
    renderer::FrameTarget,
    scene::primitive::ExternalDrawGpu,
    view::PrimitiveLeaf,
};

struct TestBackdrop;
impl PrimitiveLeaf for TestBackdrop {
    fn intrinsic_size(
        &self,
        constraints: BoxConstraints,
        _: &harbor_widget::text::TextMetrics,
    ) -> Size {
        constraints.constrain(Size::new(1000.0, 600.0))
    }
    fn paint_primitives(&self, rect: Rect, _: &harbor_widget::text::TextMetrics) -> Vec<Primitive> {
        let mut items = vec![Primitive::Quad {
            rect,
            color: Color {
                r: 0.25,
                g: 0.28,
                b: 0.36,
                a: 1.0,
            },
            corner_radius: 0.0,
        }];
        for x in (0..1000).step_by(50) {
            items.push(Primitive::Quad {
                rect: Rect::from_min_size(Point::new(x as f32, 0.0), Size::new(6.0, 600.0)),
                color: Color {
                    r: 0.75,
                    g: 0.35,
                    b: 0.18,
                    a: 1.0,
                },
                corner_radius: 0.0,
            });
        }
        items
    }
}

fn render(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    runtime: &mut Runtime,
    viewport: Viewport,
    format: wgpu::TextureFormat,
) -> Vec<u8> {
    let (width, height) = viewport.physical_size;
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("palette rounded glass regression"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let stride = (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("palette pixels"),
        size: u64::from(stride * height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let view = target.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    runtime.encode_frame(
        ExternalDrawGpu::new(device, queue, format),
        &mut encoder,
        FrameTarget {
            view: &view,
            clear_color: wgpu::Color::TRANSPARENT,
        },
        viewport,
        false,
    );
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));
    let (send, receive) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            send.send(result).unwrap()
        });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    receive.recv().unwrap().unwrap();
    let mapped = buffer.slice(..).get_mapped_range().unwrap();
    let pixels = mapped
        .chunks_exact(stride as usize)
        .flat_map(|row| row[..(width * 4) as usize].iter().copied())
        .collect();
    drop(mapped);
    buffer.unmap();
    pixels
}

#[test]
#[ignore = "GPU pixel acceptance; run explicitly with --ignored --nocapture"]
fn palette_selection_pixels_have_visible_rounded_cutouts_and_optical_edges_at_multiple_dpi() {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    if std::env::var("HARBOR_TEST_BACKEND").as_deref() == Ok("dx12") {
        descriptor.backends = wgpu::Backends::DX12;
    }
    let instance = wgpu::Instance::new(descriptor);
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .expect("a GPU adapter is required for palette pixel acceptance");
    eprintln!("palette pixel adapter: {:?}", adapter.get_info());
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    for scale in [1.0, 1.5, 2.0] {
        for format in [
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        ] {
            let (_, controller, mut runtime, _) = fixture();
            let viewport = Viewport::new((1000.0 * scale) as u32, (600.0 * scale) as u32, scale);
            runtime.set_viewport(viewport.clone());
            runtime.init_renderer(&device, format);
            controller.open_palette();
            let root = controller.clone();
            runtime.set_root(move |cx: &mut BuildCx| {
                let state = root.store.watch(cx).clone();
                let palette = state.popups.get::<palette::PaletteState>().unwrap();
                let panel = crate::popups::Popup::build(
                    palette,
                    cx,
                    &root,
                    &validate_keybindings(&harbor_config::RawKeybindings::default()).unwrap(),
                    0,
                );
                Stack::new()
                    .push_child(View::leaf(TestBackdrop))
                    .push_child(panel)
                    .build(cx)
            });
            runtime.update(Instant::now());
            let row = row_surface(&runtime, 0.10).unwrap();
            let selected = render(&device, &queue, &mut runtime, viewport.clone(), format);
            let mut state = controller.store.state().read().clone();
            state
                .popups
                .update::<palette::PaletteState>(|palette| palette.selected = 1);
            controller.store.set_state(state);
            runtime.update(Instant::now());
            assert!(
                row_surface(&runtime, 0.10).unwrap().min.y > row.min.y,
                "reference frame deselects the first row"
            );
            let baseline = render(&device, &queue, &mut runtime, viewport.clone(), format);
            if let Ok(directory) = std::env::var("HARBOR_PALETTE_EVIDENCE_DIR") {
                std::fs::create_dir_all(&directory).unwrap();
                std::fs::write(
                    std::path::Path::new(&directory)
                        .join(format!("selected-{scale}-{format:?}.bgra")),
                    &selected,
                )
                .unwrap();
            }
            let pixel = |pixels: &[u8], x: f32, y: f32| {
                let offset = (((y * scale) as u32 * viewport.physical_size.0 + (x * scale) as u32)
                    * 4) as usize;
                pixels[offset..offset + 4].to_vec()
            };
            assert_ne!(
                pixel(&selected, row.min.x + 1.0, (row.min.y + row.max.y) / 2.0),
                pixel(&baseline, row.min.x + 1.0, (row.min.y + row.max.y) / 2.0),
                "ancestor clip packing must not slice off the straight optical side"
            );
            for (x, y) in [
                (row.min.x + 2.0, row.min.y + 2.0),
                (row.max.x - 3.0, row.min.y + 2.0),
                (row.min.x + 2.0, row.max.y - 3.0),
                (row.max.x - 3.0, row.max.y - 3.0),
            ] {
                let actual = pixel(&selected, x, y);
                let original = pixel(&baseline, x, y);
                assert!(
                    actual
                        .iter()
                        .zip(&original)
                        .all(|(a, b)| a.abs_diff(*b) <= 1),
                    "round corners must expose the original backdrop at scale {scale}, ({x}, {y}): {actual:?} vs {original:?}"
                );
            }
            let x = (row.min.x + row.max.x) / 2.0;
            assert_ne!(
                pixel(&selected, x, row.min.y + 1.0),
                pixel(&baseline, x, row.min.y + 1.0),
                "optical rim must be visible"
            );
            assert_ne!(
                pixel(&selected, x, row.min.y + 15.0),
                pixel(&baseline, x, row.min.y + 15.0),
                "glass remains distinguishable from the panel"
            );
        }
    }
}

#[test]
#[ignore = "GPU pixel acceptance; run explicitly with --ignored --nocapture"]
fn search_button_hover_pixels_are_visible_local_and_revert_at_multiple_dpi() {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    if std::env::var("HARBOR_TEST_BACKEND").as_deref() == Ok("dx12") {
        descriptor.backends = wgpu::Backends::DX12;
    }
    let instance = wgpu::Instance::new(descriptor);
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .expect("a GPU adapter is required for search button pixel acceptance");
    eprintln!("search button pixel adapter: {:?}", adapter.get_info());
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    for scale in [1.0, 1.5, 2.0] {
        for format in [
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        ] {
            let (_, controller, mut runtime, _) = fixture();
            let viewport = Viewport::new((1000.0 * scale) as u32, (600.0 * scale) as u32, scale);
            runtime.set_viewport(viewport.clone());
            runtime.init_renderer(&device, format);
            controller.open_search(TabId(1));
            let root = controller.clone();
            runtime.set_root(move |cx: &mut BuildCx| {
                let state = root.store.watch(cx).clone();
                let search = state.popups.get::<SearchUiState>().unwrap();
                let panel = search_bar(cx, &root, search, state.popups.generation().unwrap());
                Stack::new()
                    .push_child(View::leaf(TestBackdrop))
                    .push_child(panel)
                    .build(cx)
            });
            runtime.update(Instant::now());
            let delta = runtime.pending_delta().unwrap();
            let mut buttons: Vec<_> = delta
                .added
                .iter()
                .filter_map(|item| match item.primitive {
                    Primitive::Quad { rect, .. } if rect.size() == Size::new(32.0, 32.0) => {
                        Some(rect)
                    }
                    _ => None,
                })
                .collect();
            buttons.sort_by(|a, b| a.min.x.total_cmp(&b.min.x));
            assert_eq!(buttons.len(), 3);
            let baseline = render(&device, &queue, &mut runtime, viewport.clone(), format);
            for rect in buttons {
                runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
                    Point::new(rect.min.x + 1.0, rect.min.y + 16.0),
                    PointerPhase::Move,
                    PointerButton::Left,
                    93,
                )));
                runtime.update(Instant::now());
                let hovered = render(&device, &queue, &mut runtime, viewport.clone(), format);
                let mut changed = 0;
                for (index, (before, after)) in baseline
                    .chunks_exact(4)
                    .zip(hovered.chunks_exact(4))
                    .enumerate()
                {
                    let x = (index as u32 % viewport.physical_size.0) as f32 / scale;
                    let y = (index as u32 / viewport.physical_size.0) as f32 / scale;
                    // Ignore the one-physical-pixel antialiasing boundary for isolation.
                    let edge = 1.0 / scale;
                    if x < rect.min.x - edge
                        || x > rect.max.x + edge
                        || y < rect.min.y - edge
                        || y > rect.max.y + edge
                    {
                        assert_eq!(
                            before, after,
                            "hover must not change pixels outside the button"
                        );
                    } else if before != after {
                        changed += 1;
                    }
                }
                assert!(
                    changed > (100.0 * scale * scale) as usize,
                    "glass must visibly cover the control, not only change the icon"
                );
                let offset = ((((rect.min.y + 16.0) * scale) as u32 * viewport.physical_size.0
                    + ((rect.min.x + 1.0) * scale) as u32)
                    * 4) as usize;
                assert_ne!(
                    &baseline[offset..offset + 4],
                    &hovered[offset..offset + 4],
                    "the optical side must survive ancestor clipping"
                );
                runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
                    Point::new(0.0, 0.0),
                    PointerPhase::Move,
                    PointerButton::Left,
                    93,
                )));
                runtime.update(Instant::now());
                let restored = render(&device, &queue, &mut runtime, viewport.clone(), format);
                assert_eq!(
                    baseline, restored,
                    "leaving restores the original frame exactly"
                );
                assert!(controller.drain_actions().is_empty());
            }
        }
    }
}
