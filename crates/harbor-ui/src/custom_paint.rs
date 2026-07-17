use crate::{BoxConstraints, Key, Rect, Widget};
pub use harbor_render::PaintContext;
use harbor_render::RenderEnvironment;

/// A renderer-command painter supplied by a UI-owned [`CustomPaint`] Widget.
pub trait CustomPainter {
    fn paint<'a>(&'a self, context: &mut PaintContext<'a>);
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

    fn layout(
        &self,
        _state: &mut Self::State,
        _environment: RenderEnvironment,
        constraints: BoxConstraints,
    ) -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            width: constraints.max_width,
            height: constraints.max_height,
        }
    }

    fn paint<'a>(&'a self, _state: &'a mut Self::State, context: &mut PaintContext<'a>) {
        self.painter.paint(context);
    }
}
