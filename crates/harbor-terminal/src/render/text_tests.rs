//! Integration tests over real native fonts, coherent engine updates and GPU readback.
#[path = "text_tests/gpu.rs"]
mod gpu;
#[path = "text_tests/native.rs"]
mod native;

use super::*;
use crate::{Terminal, TerminalRenderPipeline};
use std::time::Instant;

pub(crate) use crate::test_gpu::device as test_gpu;

pub(crate) fn read_draw(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    draw: impl FnOnce(&mut wgpu::RenderPass),
) -> Vec<u8> {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("emoji readback target"),
        size: wgpu::Extent3d {
            width: 256,
            height: 128,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("emoji readback"),
        size: 256 * 128 * 4,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let view = texture.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("emoji test pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.25,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        draw(&mut pass);
    }
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(1024),
                rows_per_image: Some(128),
            },
        },
        wgpu::Extent3d {
            width: 256,
            height: 128,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let pixels = buffer.slice(..).get_mapped_range().unwrap().to_vec();
    buffer.unmap();
    pixels
}

fn fonts() -> FontBook {
    harbor_text::load_system_fonts(&harbor_config::FontSettings {
        family: Some("Consolas".into()),
        size: 16.0,
    })
    .unwrap()
}
fn text(gpu: TerminalGpuAccess<'_>, snap: &TerminalSnapshot) -> (Text, RenderViewport) {
    let f = fonts();
    let metrics = TextMetrics::from_font_metrics(f.font_metrics());
    let viewport = RenderViewport::with_surface(
        metrics.cell_width,
        metrics.line_height,
        (256, 128),
        (256, 128),
    );
    (
        Text::new(gpu, f, metrics, snap, &viewport, Palette::default()).unwrap(),
        viewport,
    )
}

// Keep byte-exact GPU comparisons, but report bounded diagnostics instead of
// dumping two 128 KiB images into the Actions log.
fn assert_pixels_equal(actual: &[u8], expected: &[u8], scale: f32) {
    assert_eq!(actual.len(), expected.len());
    let differences: Vec<_> = actual
        .as_chunks::<4>()
        .0
        .iter()
        .zip(expected.as_chunks::<4>().0.iter())
        .enumerate()
        .filter(|(_, (actual, expected))| actual != expected)
        .collect();
    let max_delta = actual
        .iter()
        .zip(expected)
        .map(|(actual, expected)| actual.abs_diff(*expected))
        .max()
        .unwrap_or(0);
    let samples: Vec<_> = differences
        .iter()
        .take(8)
        .map(|(index, (actual, expected))| (index % 256, index / 256, actual, expected))
        .collect();
    assert!(
        differences.is_empty(),
        "pixels changed at scale {scale}: {} pixels, max channel delta {max_delta}, \
         first (x, y, actual, expected): {samples:?}",
        differences.len()
    );
}

fn metric_values(metrics: &TextMetrics) -> [f32; 7] {
    [
        metrics.cell_width,
        metrics.line_height,
        metrics.ascent,
        metrics.underline_position,
        metrics.underline_thickness,
        metrics.strikethrough_position,
        metrics.strikethrough_thickness,
    ]
}
