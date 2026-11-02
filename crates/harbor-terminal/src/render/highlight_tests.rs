#[path = "highlight_tests/gpu.rs"]
mod gpu;

use super::*;
use crate::{
    Terminal,
    literal_search::{LiteralSearch, SearchStatus},
    read_model::{ReadBudget, ReadScope},
};

fn raster(device: &wgpu::Device, queue: &wgpu::Queue, layer: &Highlights) -> Vec<u8> {
    let texture = crate::test_gpu::readback::target(
        device,
        "highlight readback",
        64,
        32,
        wgpu::TextureFormat::Rgba8Unorm,
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
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
    crate::test_gpu::readback::read(
        device,
        queue,
        encoder,
        &texture,
        crate::test_gpu::readback::Rows::Tight,
    )
}
