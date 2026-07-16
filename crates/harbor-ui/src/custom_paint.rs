use crate::{BoxConstraints, Key, Rect, TextResources, Widget};
use harbor_gpu::GpuContext;

/// Frame-scoped GPU resources shared by every widget in one UI runtime.
pub struct PaintContext<'a> {
    pub gpu: &'a GpuContext,
    pub text: &'a mut TextResources,
    pub bounds: Rect,
}

/// A custom GPU painter. The UI runtime owns surface acquisition and presentation.
pub trait CustomPainter {
    fn paint<'pass>(&self, context: PaintContext<'_>, pass: &mut wgpu::RenderPass<'pass>);
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

impl<A, P> Widget<A> for CustomPaint<P>
where
    P: CustomPainter,
{
    type State = ();

    fn create_state(&self) -> Self::State {}

    fn key(&self) -> Option<Key> {
        self.key
    }

    fn layout(&self, _state: &mut Self::State, constraints: BoxConstraints) -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            width: constraints.max_width,
            height: constraints.max_height,
        }
    }

    fn paint<'pass>(
        &self,
        _state: &mut Self::State,
        context: PaintContext<'_>,
        pass: &mut wgpu::RenderPass<'pass>,
    ) {
        self.painter.paint(context, pass);
    }
}
