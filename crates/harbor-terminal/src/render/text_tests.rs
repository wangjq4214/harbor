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
    let texture = crate::test_gpu::readback::target(
        device,
        "emoji readback target",
        256,
        128,
        wgpu::TextureFormat::Rgba8Unorm,
    );
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
    crate::test_gpu::readback::read(
        device,
        queue,
        encoder,
        &texture,
        crate::test_gpu::readback::Rows::Tight,
    )
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
