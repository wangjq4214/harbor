use crate::input::event::UiEvent;
use crate::input::event_ctx::{EventCtx, EventHandled};
use crate::layout::{BoxConstraints, Point, Rect, Size};
use crate::scene::primitive::{ExternalDrawId, Primitive};
use crate::view::{AnyView, BuildCx, Component, Key as ViewKey, View};

/// A focusable widget that delegates painting to an externally-owned
/// renderer identified by [`ExternalDrawId`].
///
/// During the paint pass, produces a single [`Primitive::External`].
/// During event handling, queues input events for deferred delivery to
/// the external provider after the event walk completes.
#[derive(Clone)]
pub struct CustomPaint {
    pub draw_id: ExternalDrawId,
    children: Vec<View>,
}

impl CustomPaint {
    pub fn new(draw_id: ExternalDrawId) -> Self {
        CustomPaint {
            draw_id,
            children: vec![],
        }
    }

    pub fn child(mut self, child: impl Component + 'static) -> Self {
        let mut cx = BuildCx::stub();
        self.children.push(child.build(&mut cx));
        self
    }
}

impl Component for CustomPaint {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::new(self.clone(), self.children.clone(), None)
    }
}

impl AnyView for CustomPaint {
    fn key(&self) -> Option<&ViewKey> {
        None
    }

    fn widget_type(&self) -> std::any::TypeId {
        std::any::TypeId::of::<Self>()
    }

    fn build(self: Box<Self>, _cx: &mut BuildCx) -> View {
        View::new(*self, vec![], None)
    }

    fn intrinsic_size(&self, constraints: BoxConstraints) -> Size {
        // Fill available space so the terminal occupies the full viewport.
        constraints.max
    }

    fn layout_children(
        &self,
        constraints: BoxConstraints,
        child_sizes: &[Size],
    ) -> (Size, Vec<Point>) {
        let own = constraints.max;
        let positions = vec![Point::ZERO; child_sizes.len()];
        (own, positions)
    }

    fn paint_primitives(&self, rect: Rect) -> Vec<Primitive> {
        vec![Primitive::External {
            draw: self.draw_id,
            rect,
        }]
    }

    fn is_focusable(&self) -> bool {
        true
    }

    fn handle_event(&self, event: &UiEvent, _ctx: &mut EventCtx, _rect: Rect) -> EventHandled {
        // Queue for deferred delivery to the App via Runtime.
        crate::runtime::queue_external_input(self.draw_id, event.clone());
        EventHandled::Handled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::event::{Key, KeyboardEvent, Modifiers};

    #[test]
    fn custom_paint_is_focusable() {
        let cp = CustomPaint::new(1);
        assert!(cp.is_focusable());
    }

    #[test]
    fn custom_paint_paint_primitives_contains_external() {
        let cp = CustomPaint::new(42);
        let rect = Rect::from_min_size(Point::new(10.0, 20.0), Size::new(800.0, 600.0));
        let prims = cp.paint_primitives(rect);
        assert_eq!(prims.len(), 1);
        match &prims[0] {
            Primitive::External { draw, rect: r } => {
                assert_eq!(*draw, 42);
                assert_eq!(r.min, Point::new(10.0, 20.0));
            }
            _ => panic!("expected External primitive"),
        }
    }

    #[test]
    fn custom_paint_fills_max_constraints() {
        let cp = CustomPaint::new(1);
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let size = cp.intrinsic_size(constraints);
        assert_eq!(size, Size::new(800.0, 600.0));
    }

    #[test]
    fn custom_paint_handle_event_returns_handled() {
        let cp = CustomPaint::new(1);
        let rect = Rect::from_min_size(Point::ZERO, Size::new(100.0, 100.0));
        let event = UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Enter,
            modifiers: Modifiers::default(),
        });
        let mut ctx = EventCtx::new();
        let result = cp.handle_event(&event, &mut ctx, rect);
        assert_eq!(result, EventHandled::Handled);
    }

    #[test]
    fn custom_paint_build_produces_view() {
        let cp = CustomPaint::new(7);
        let mut cx = BuildCx::stub();
        let view = cp.build(&mut cx);
        let (_inner, children, _key) = view.decompose();
        assert!(children.is_empty());
    }
}
