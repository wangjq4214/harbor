//! GPU-backed evidence for layout-only resize: compare actual retained handles.
#[path = "layout_resources_tests/gpu.rs"]
mod gpu;

use crate::layout::{CrossAxisAlignment, Rect};
use crate::renderer::Viewport;
use crate::runtime::Runtime;
use crate::scene::primitive::{Color, ExternalDrawFn, ExternalDrawGpu, Primitive};
use crate::widgets::external_surface::ExternalSurface;
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

use crate::test_gpu::device as gpu_device;

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
