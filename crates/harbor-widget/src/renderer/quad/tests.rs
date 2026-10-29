//! Internal storage/retention evidence, separate from public pixel tests.
#[path = "tests/gpu.rs"]
mod gpu;

use super::*;
use crate::layout::{Point, Size};
use crate::scene::primitive::Color;
use std::sync::Arc;

use crate::test_gpu::device as gpu_device;

/// Creates a headless RenderPass backed by a throwaway 256×256 texture.
fn create_render_pass<'a>(
    device: &wgpu::Device,
    encoder: &'a mut wgpu::CommandEncoder,
) -> wgpu::RenderPass<'a> {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("test-render-target"),
        size: wgpu::Extent3d {
            width: 256,
            height: 256,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("test-pass"),
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
    })
}

fn make_viewport() -> Viewport {
    Viewport::new(800, 600, 1.0)
}

fn make_quad_item(id: u64, order: u32, x: f32, y: f32, w: f32, h: f32) -> SceneItem {
    SceneItem {
        id,
        primitive: Primitive::Quad {
            rect: Rect::from_min_size(Point::new(x, y), Size::new(w, h)),
            color: Color::RED,
            corner_radius: 0.0,
        },
        clips: Vec::new(),
        paint_order: order,
    }
}

fn make_rounded_item(id: u64, primitive: Primitive) -> SceneItem {
    SceneItem {
        id,
        primitive,
        clips: Vec::new(),
        paint_order: 0,
    }
}
