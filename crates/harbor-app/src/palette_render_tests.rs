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
    let target = crate::test_gpu::readback::target(
        device,
        "palette rounded glass regression",
        width,
        height,
        format,
    );
    let view = target.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    runtime.prepare_text(queue);
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
    crate::test_gpu::readback::read(
        device,
        queue,
        encoder,
        &target,
        crate::test_gpu::readback::Rows::Tight,
    )
}
