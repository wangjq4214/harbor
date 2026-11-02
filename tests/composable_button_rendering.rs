//! Explicit renderer/font acceptance samples for spec 0037, not a native/HMR substitute.
//! Run: cargo test --test composable_button_rendering -- --ignored --nocapture
//! Writes target/f4-buttons/samples.png (columns: 320, 150, 40dp; see test row order).
#[path = "composable_button_rendering/gpu.rs"]
mod gpu;

use harbor_widget::{
    Button, ConstrainedBox, LucideIcon, Theme,
    input::event::{PointerButton, PointerEvent, PointerPhase, UiEvent},
    layout::{Point, Size},
    renderer::Viewport,
    runtime::Runtime,
    scene::primitive::{Color, ExternalDrawGpu, Primitive},
    view::Component,
    widgets::text_label::TextLabel,
};
use std::time::Instant;
#[path = "support/gpu.rs"]
mod test_gpu;

fn scene(rt: &Runtime) -> Vec<Primitive> {
    let delta = rt.pending_delta().expect("painted scene");
    delta
        .added
        .iter()
        .chain(&delta.modified)
        .map(|item| item.primitive.clone())
        .collect()
}
fn root_size(rt: &Runtime) -> Size {
    rt.arena()
        .get(rt.root_id().unwrap())
        .unwrap()
        .layout_rect()
        .unwrap()
        .size()
}
fn text(rt: &Runtime) -> String {
    scene(rt)
        .iter()
        .find_map(|p| match p {
            Primitive::Text { text, .. } => Some(text.to_string()),
            _ => None,
        })
        .unwrap_or_default()
}
fn pointer(rt: &mut Runtime, phase: PointerPhase) {
    rt.dispatch(UiEvent::Pointer(PointerEvent::new(
        Point::new(1.0, 1.0),
        phase,
        PointerButton::Left,
        0,
    )));
    rt.update(Instant::now());
}

fn capture(
    rt: &mut Runtime,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    width: u32,
) -> image::RgbaImage {
    const HEIGHT: u32 = 44;
    let viewport = Viewport::new(width, HEIGHT, 1.0);
    rt.prepare_text(queue);
    let target = test_gpu::readback::target(
        device,
        "F4 samples",
        width,
        HEIGHT,
        wgpu::TextureFormat::Bgra8Unorm,
    );
    let view = target.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("F4 render"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.08,
                        g: 0.08,
                        b: 0.08,
                        a: 1.0,
                    }),
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
    let data = test_gpu::readback::read(
        device,
        queue,
        encoder,
        &target,
        test_gpu::readback::Rows::Tight,
    );
    // Readback is tightly packed BGRA; image::RgbaImage needs an explicit swizzle.
    let pixels = image::RgbaImage::from_fn(width, HEIGHT, |x, y| {
        let offset = ((y * width + x) * 4) as usize;
        image::Rgba([
            data[offset + 2],
            data[offset + 1],
            data[offset],
            data[offset + 3],
        ])
    });
    assert!(
        pixels.pixels().any(|p| p.0[0] > 30),
        "actual control pixels rendered"
    );
    pixels
}
