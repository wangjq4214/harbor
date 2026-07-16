use crate::{Key, Rect};
use harbor_gpu::GpuContext;

pub struct PaintContext<'a> {
    pub gpu: &'a GpuContext,
    pub bounds: Rect,
}

/// A custom GPU painter. The UI runtime owns surface acquisition and presentation.
pub trait CustomPainter {
    fn paint<'pass>(&mut self, context: PaintContext<'_>, pass: &mut wgpu::RenderPass<'pass>);
}

pub struct CustomPaint<P> {
    pub painter: P,
    pub key: Option<Key>,
}

impl<P> CustomPaint<P> {
    pub fn new(painter: P) -> Self {
        Self { painter, key: None }
    }

    pub fn key(mut self, key: Key) -> Self {
        self.key = Some(key);
        self
    }
}
