use crate::input::event::{PointerPhase, UiEvent};
use crate::input::event_ctx::{EventCtx, EventHandled};
use crate::layout::{BoxConstraints, Point, Rect, Size};
use crate::scene::primitive::{ExternalDrawFn, ExternalDrawId, Primitive};
use crate::text::TextMetrics;
use crate::view::{AnyView, BuildCx, Component, Key as ViewKey, View};
use std::sync::Arc;

/// A focusable widget that delegates painting to an externally-owned
/// renderer identified by [`ExternalDrawId`].
///
/// During the paint pass, produces a single [`Primitive::External`].
/// During event handling, queues input events for deferred delivery to
/// the external provider after the event walk completes.
#[derive(Clone)]
pub struct CustomPaint {
    pub draw_id: ExternalDrawId,
    handler: Option<Arc<ExternalDrawFn<'static>>>,
    children: Vec<View>,
}

impl CustomPaint {
    pub fn new(draw_id: ExternalDrawId) -> Self {
        CustomPaint {
            draw_id,
            handler: None,
            children: vec![],
        }
    }

    /// Sets the renderer invoked for this widget's external primitive.
    pub fn handler(mut self, handler: Arc<ExternalDrawFn<'static>>) -> Self {
        self.handler = Some(handler);
        self
    }

    pub fn child(mut self, child: impl Component + 'static) -> Self {
        self.children.push(View::deferred(child));
        self
    }
}

impl Component for CustomPaint {
    fn build(&self, cx: &mut BuildCx) -> View {
        if let Some(handler) = &self.handler {
            cx.register_external_draw(self.draw_id, Arc::clone(handler));
        }
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

    fn intrinsic_size(&self, constraints: BoxConstraints, _metrics: &TextMetrics) -> Size {
        // Fill available space so the terminal occupies the full viewport.
        constraints.max
    }

    fn layout_children(
        &self,
        constraints: BoxConstraints,
        child_sizes: &[Size],
        _metrics: &TextMetrics,
    ) -> (Size, Vec<Point>) {
        let own = constraints.max;
        let positions = vec![Point::ZERO; child_sizes.len()];
        (own, positions)
    }

    fn paint_primitives(&self, rect: Rect, _metrics: &TextMetrics) -> Vec<Primitive> {
        vec![Primitive::External {
            draw: self.draw_id,
            rect,
        }]
    }

    fn is_focusable(&self) -> bool {
        true
    }

    fn handle_event(&self, event: &UiEvent, ctx: &mut EventCtx, _rect: Rect) -> EventHandled {
        if matches!(event, UiEvent::Pointer(pointer) if pointer.phase == PointerPhase::Down)
            && let Some(fiber) = ctx.current_fiber()
        {
            ctx.request_focus(fiber);
        }

        // Queue for deferred delivery to the App via Runtime.
        crate::runtime::queue_external_input(self.draw_id, event.clone());
        EventHandled::Handled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::event::{Key, KeyboardEvent, Modifiers};
    use std::sync::Arc;

    #[test]
    fn custom_paint_is_focusable() {
        let cp = CustomPaint::new(1);
        assert!(cp.is_focusable());
    }

    #[test]
    fn custom_paint_paint_primitives_contains_external() {
        let cp = CustomPaint::new(42);
        let rect = Rect::from_min_size(Point::new(10.0, 20.0), Size::new(800.0, 600.0));
        let prims = cp.paint_primitives(rect, &crate::runtime::DEFAULT_TEXT_METRICS);
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
        let size = cp.intrinsic_size(constraints, &crate::runtime::DEFAULT_TEXT_METRICS);
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

    #[test]
    fn should_register_handler_when_custom_paint_is_built_with_one() {
        // Arrange: a CustomPaint with a stable handler and real build context.
        let handler: Arc<ExternalDrawFn<'static>> = Arc::new(|_, _, _| {});
        let custom_paint = CustomPaint::new(42).handler(Arc::clone(&handler));
        let mut cx = BuildCx::stub();

        // Act: build the component.
        custom_paint.build(&mut cx);

        // Assert: the handler is registered for the component's draw ID.
        assert_eq!(cx.external_draws.len(), 1);
        assert_eq!(cx.external_draws[0].0, 42);
        assert!(Arc::ptr_eq(&cx.external_draws[0].1, &handler));
    }

    #[test]
    fn should_not_register_handler_when_custom_paint_has_none() {
        // Arrange: a CustomPaint using its default, handler-free configuration.
        let custom_paint = CustomPaint::new(42);
        let mut cx = BuildCx::stub();

        // Act: build the component.
        custom_paint.build(&mut cx);

        // Assert: no external draw registration is produced.
        assert!(cx.external_draws.is_empty());
    }
}
