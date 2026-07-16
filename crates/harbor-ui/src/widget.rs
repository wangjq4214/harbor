use crate::{BoxConstraints, Key, PaintContext, Rect};

/// Result of routing one window event through a widget tree.
#[derive(Debug, PartialEq, Eq)]
pub enum EventResult<A> {
    Ignored,
    Handled,
    Intent(A),
}

impl<A> EventResult<A> {
    pub fn map<B>(self, map: impl FnOnce(A) -> B) -> EventResult<B> {
        match self {
            Self::Ignored => EventResult::Ignored,
            Self::Handled => EventResult::Handled,
            Self::Intent(intent) => EventResult::Intent(map(intent)),
        }
    }
}

/// Retained state belonging to a declarative widget configuration.
///
/// A widget configuration is immutable. Its runtime state is created separately
/// and is retained by the parent runtime while the configuration is rebuilt.
pub trait Widget<A> {
    type State;

    fn create_state(&self) -> Self::State;

    /// An explicit identity for a dynamic child. Fixed children use their
    /// structural path in the parent tree.
    fn key(&self) -> Option<Key> {
        None
    }

    /// Whether an enclosing linear layout should allocate remaining main-axis
    /// space to this widget.
    fn expands(&self) -> bool {
        false
    }

    /// Computes this widget's assigned bounds inside the offered constraints.
    fn layout(&self, state: &mut Self::State, constraints: BoxConstraints) -> Rect;

    /// Routes a window event inside `bounds` and may emit one typed intent.
    fn event(
        &self,
        _state: &mut Self::State,
        _event: &winit::event::WindowEvent,
        _bounds: Rect,
    ) -> EventResult<A> {
        EventResult::Ignored
    }

    /// Draws inside the bounds returned by `layout`.
    fn paint<'pass>(
        &self,
        _state: &mut Self::State,
        _context: PaintContext<'_>,
        _pass: &mut wgpu::RenderPass<'pass>,
    ) {
    }
}

/// Retains state for a statically composed widget tree.
pub struct WidgetRuntime<W, A>
where
    W: Widget<A>,
{
    state: W::State,
    _widget: std::marker::PhantomData<fn() -> (W, A)>,
}

impl<W, A> WidgetRuntime<W, A>
where
    W: Widget<A>,
{
    pub fn new(widget: &W) -> Self {
        Self {
            state: widget.create_state(),
            _widget: std::marker::PhantomData,
        }
    }

    pub fn layout(&mut self, widget: &W, constraints: BoxConstraints) -> Rect {
        widget.layout(&mut self.state, constraints)
    }

    pub fn event(
        &mut self,
        widget: &W,
        event: &winit::event::WindowEvent,
        bounds: Rect,
    ) -> EventResult<A> {
        widget.event(&mut self.state, event, bounds)
    }

    pub fn paint<'pass>(
        &mut self,
        widget: &W,
        context: PaintContext<'_>,
        pass: &mut wgpu::RenderPass<'pass>,
    ) {
        widget.paint(&mut self.state, context, pass);
    }
}

/// Maps a child's local intent into its parent's typed intent.
pub struct Map<W, F, B> {
    pub child: W,
    pub map: F,
    _intent: std::marker::PhantomData<fn(B)>,
}

impl<W, F, B> Map<W, F, B> {
    pub fn new(child: W, map: F) -> Self {
        Self {
            child,
            map,
            _intent: std::marker::PhantomData,
        }
    }
}

impl<A, B, W, F> Widget<A> for Map<W, F, B>
where
    W: Widget<B>,
    F: Fn(B) -> A,
{
    type State = W::State;

    fn create_state(&self) -> Self::State {
        self.child.create_state()
    }

    fn key(&self) -> Option<Key> {
        self.child.key()
    }

    fn layout(&self, state: &mut Self::State, constraints: BoxConstraints) -> Rect {
        self.child.layout(state, constraints)
    }

    fn event(
        &self,
        state: &mut Self::State,
        event: &winit::event::WindowEvent,
        bounds: Rect,
    ) -> EventResult<A> {
        self.child.event(state, event, bounds).map(&self.map)
    }

    fn paint<'pass>(
        &self,
        state: &mut Self::State,
        context: PaintContext<'_>,
        pass: &mut wgpu::RenderPass<'pass>,
    ) {
        self.child.paint(state, context, pass);
    }
}
