//! GPU coverage of real glass overlay composition, not just scene descriptors.
#[path = "palette_render_tests/gpu.rs"]
mod gpu;

use super::palette_support::*;
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
