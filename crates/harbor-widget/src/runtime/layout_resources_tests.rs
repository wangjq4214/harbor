//! GPU-backed evidence for layout-only resize: compare actual retained handles.
use crate::layout::{Alignment, Rect};
use crate::renderer::Viewport;
use crate::runtime::Runtime;
use crate::scene::primitive::{Color, ExternalDrawFn, Primitive};
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
        rt.encode(queue, &mut pass, viewport, true);
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
    let handler: Arc<ExternalDrawFn<'static>> = Arc::new(move |_, context, _, _| {
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
                Primitive::External { draw: 99, rect } => Some(rect),
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
