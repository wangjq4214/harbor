use super::*;
use crate::{
    Terminal,
    literal_search::{LiteralSearch, SearchStatus},
    read_model::{ReadBudget, ReadScope},
    render::gpu,
};

fn raster(device: &wgpu::Device, queue: &wgpu::Queue, layer: &Highlights) -> Vec<u8> {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("highlight readback"),
        size: wgpu::Extent3d {
            width: 64,
            height: 32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 256 * 32,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
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
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });
        layer.draw(&mut pass);
    }
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(32),
            },
        },
        wgpu::Extent3d {
            width: 64,
            height: 32,
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

#[test]
fn gpu_highlights_use_current_geometry_cache_and_remove_stale_text_without_cell_mutation() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("GPU highlight test adapter");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let pipeline = Arc::new(gpu::create_colored_quad_pipeline(
        &device,
        gpu.format(),
        "highlight test",
    ));
    let mut layer = Highlights::new(gpu, pipeline);
    let mut t = Terminal::new_headless(2, 8);
    t.put_str("ab界");
    let version = t.content_version();
    let mut gate = LiteralSearch::default();
    let mut job = gate
        .start(
            &t,
            "界",
            ReadScope::Retained {
                start_row: 0,
                row_count: 2,
            },
            ReadBudget::default(),
        )
        .unwrap();
    while job.step(&t) == SearchStatus::Running {}
    let results = gate.accept(&t, job.into_results()).unwrap();
    let spans = t.project_content_ranges(results.ranges());
    let mut viewport = RenderViewport::with_surface(8.0, 8.0, (64, 32), (64, 32));
    viewport.padding = 0.0;
    layer.prepare(gpu, &spans, viewport);
    assert_eq!(layer.quads.count(), 6);
    let pixels = raster(&device, &queue, &layer);
    let pixel = |p: &[u8], x: usize, y: usize| p[(y * 64 + x) * 4..(y * 64 + x + 1) * 4].to_vec();
    assert!(pixel(&pixels, 17, 3)[0] > 0);
    assert_eq!(pixel(&pixels, 15, 3), [0, 0, 0, 0]);
    assert_eq!(pixel(&pixels, 32, 3), [0, 0, 0, 0]);
    layer.prepare(gpu, &spans, viewport);
    assert_eq!(
        raster(&device, &queue, &layer),
        pixels,
        "cached frame is identical"
    );
    viewport.allocation_origin = (8.0, 8.0);
    layer.prepare(gpu, &spans, viewport);
    let moved = raster(&device, &queue, &layer);
    assert_eq!(pixel(&moved, 17, 3), [0, 0, 0, 0]);
    assert!(pixel(&moved, 25, 11)[0] > 0);
    assert_eq!(
        t.content_version(),
        version,
        "painting doesn't mutate terminal cells"
    );
    t.put_str("\rX");
    layer.prepare(gpu, &t.project_content_ranges(results.ranges()), viewport);
    assert_eq!(layer.quads.count(), 0);
    assert!(raster(&device, &queue, &layer).iter().all(|b| *b == 0));
}
