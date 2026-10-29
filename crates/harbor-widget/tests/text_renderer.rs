//! Integration tests for TextRenderer — construction with a wgpu Device.
//!
//! Explicit GPU cases require a usable device; missing resources are failures.

#[path = "text_renderer/gpu.rs"]
mod gpu;

use harbor_text::{FaceId, FontSize, FontStyle, GlyphId, GlyphKey};
use harbor_widget::layout::{Point, Rect, Size};
use harbor_widget::renderer::Viewport;
use harbor_widget::renderer::text_renderer::TextRenderer;
use harbor_widget::runtime::Runtime;
use harbor_widget::scene::clip::RoundedClip;
use harbor_widget::scene::primitive::{Color, Primitive};
use harbor_widget::scene::{SceneGraph, SceneItem};
use harbor_widget::text::{AtlasGlyph, AtlasUv, TextMetrics, TextRunCache};
use harbor_widget::widgets::text_label::TextLabel;
use harbor_widget::{BorderRadius, BoxDecoration, ClipBehavior, DecoratedBox};
use std::sync::Arc;
use std::time::Instant;

// ── GPU helpers ─────────────────────────────────────────────────────────────

#[path = "../../../tests/support/gpu.rs"]
mod test_gpu;
use test_gpu::device as gpu_device;

/// Creates a tiny 64×64 R8 atlas texture for TextRenderer construction.
fn create_atlas_texture(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("test-atlas"),
        size: wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

/// Creates a linear atlas sampler.
fn create_atlas_sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("test-sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    })
}

fn create_bind_group(
    device: &wgpu::Device,
    atlas_texture: &wgpu::Texture,
    atlas_sampler: &wgpu::Sampler,
) -> (wgpu::BindGroupLayout, wgpu::BindGroup) {
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("test-text-bind-layout"),
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

    let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("test-text-bind-group"),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&atlas_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(atlas_sampler),
            },
        ],
    });

    (layout, bind_group)
}

// ── Tests ───────────────────────────────────────────────────────────────────

fn solid_glyph() -> AtlasGlyph {
    AtlasGlyph {
        key: GlyphKey::new(
            FaceId::PRIMARY,
            GlyphId::new(0),
            FontSize::new(1.0).expect("valid test font size"),
            FontStyle::REGULAR,
        ),
        uv: AtlasUv {
            left: 0.0,
            top: 0.0,
            right: 1.0,
            bottom: 1.0,
        },
        width: 32,
        height: 32,
        bearing_x: 0,
        bearing_y: 0,
        advance_width: 32.0,
        atlas_x: 0,
        atlas_y: 0,
    }
}

fn glyph_metrics() -> TextMetrics {
    TextMetrics {
        cell_width: 32.0,
        line_height: 32.0,
        ascent: 32.0,
        underline_position: 0.0,
        underline_thickness: 1.0,
        strikethrough_position: 0.0,
        strikethrough_thickness: 1.0,
    }
}

fn text_item(clips: Vec<RoundedClip>) -> SceneItem {
    SceneItem {
        id: 1,
        primitive: Primitive::Text {
            text: Arc::from("X"),
            origin: Point::ZERO,
            color: Color::RED,
        },
        clips,
        paint_order: 0,
    }
}

fn bgra(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let offset = (y * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT + x * 4) as usize;
    [
        pixels[offset],
        pixels[offset + 1],
        pixels[offset + 2],
        pixels[offset + 3],
    ]
}

fn render_text_item(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    item: SceneItem,
) -> Option<Vec<u8>> {
    let atlas = create_atlas_texture(device);
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &atlas,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &[255u8; 64 * 64],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(64),
            rows_per_image: Some(64),
        },
        wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
    );
    let sampler = create_atlas_sampler(device);
    let (layout, bind_group) = create_bind_group(device, &atlas, &sampler);
    let mut renderer = TextRenderer::new(
        device,
        wgpu::TextureFormat::Bgra8Unorm,
        &layout,
        &bind_group,
    );
    let mut cache = TextRunCache::new();
    cache.upsert(item.id, "X", &glyph_metrics(), &|_| Some(solid_glyph()));
    let mut graph = SceneGraph::new();
    graph.diff(vec![item]);
    let viewport = Viewport::new(32, 32, 1.0);
    renderer.update(queue, &graph, &cache, &viewport);

    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("text clip render target"),
        size: wgpu::Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("text clip render pass"),
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
        renderer.encode_item(&mut pass, 1);
    }
    queue.submit(Some(encoder.finish()));

    const BYTES_PER_ROW: u32 = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("text clip readback"),
        size: (32 * BYTES_PER_ROW) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
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
                bytes_per_row: Some(BYTES_PER_ROW),
                rows_per_image: Some(32),
            },
        },
        wgpu::Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));
    let slice = buffer.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        sender.send(result).unwrap();
    });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    receiver.recv().unwrap().unwrap();
    let bytes = slice
        .get_mapped_range()
        .expect("readback buffer should be mapped")
        .to_vec();
    buffer.unmap();
    Some(bytes)
}

fn paint_text(clip_behavior: ClipBehavior) -> harbor_widget::scene::SceneDelta {
    let mut runtime = Runtime::new();
    runtime.set_root(
        DecoratedBox::new(BoxDecoration::new().border_radius(BorderRadius::all(8.0).unwrap()))
            .clip_behavior(clip_behavior)
            .child(
                TextLabel::new("Hi")
                    .foreground(Color::RED)
                    .expect("valid color"),
            ),
    );
    runtime.update(Instant::now());
    runtime
        .pending_delta()
        .cloned()
        .expect("first update has a delta")
}

#[test]
fn should_attach_clip_to_text_items_when_decorated_box_clips_descendants() {
    // Arrange / Act
    let delta = paint_text(ClipBehavior::HardEdge);

    // Assert
    let text = delta
        .added
        .iter()
        .find(|item| matches!(item.primitive, Primitive::Text { .. }))
        .expect("text primitive");
    assert_eq!(text.clips.len(), 1);
    assert_eq!(text.clips[0].behavior(), ClipBehavior::HardEdge);
}

#[test]
fn should_leave_text_items_unclipped_when_policy_is_none() {
    // Arrange / Act
    let delta = paint_text(ClipBehavior::None);

    // Assert
    let text = delta
        .added
        .iter()
        .find(|item| matches!(item.primitive, Primitive::Text { .. }))
        .expect("text primitive");
    assert!(text.clips.is_empty());
}
