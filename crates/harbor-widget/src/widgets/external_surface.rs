use crate::input::event::{PointerPhase, UiEvent};
use crate::input::event_ctx::{EventCtx, EventHandled};
use crate::layout::{BoxConstraints, Point, Rect, Size};
use crate::scene::primitive::{ExternalDrawFn, ExternalDrawId, ExternalScheduleFn, Primitive};
use crate::text::TextMetrics;
use crate::view::{AnyView, BuildCx, Component, FocusMetadata, View};
use std::sync::Arc;

/// Optional in-tree input adapter for a focusable external-paint leaf.
///
/// When set on [`ExternalSurface`], routed events are delivered here instead of
/// being queued for [`crate::runtime::Runtime::drain_external_input`].
/// Return `Handled` to consume this event or `Ignored` to let it continue.
/// Requested EventCtx effects commit with either result; this callback does not
/// also trigger automatic external-queue delivery.
///
/// ```
/// use std::sync::Arc;
/// use harbor_widget::input::event::{PointerPhase, UiEvent};
/// use harbor_widget::input::event_ctx::EventHandled;
/// use harbor_widget::widgets::external_surface::ExternalSurface;
///
/// // An input adapter owns wheel input, but leaves other events available.
/// let consuming = ExternalSurface::new(1).on_input(Arc::new(|event, ctx| {
///     if matches!(event, UiEvent::Pointer(p) if matches!(p.phase,
///         PointerPhase::WheelLine { .. } | PointerPhase::WheelPixel { .. }))
///     {
///         ctx.invalidate_paint();
///         EventHandled::Handled
///     } else {
///         EventHandled::Ignored
///     }
/// }));
/// // Observing/requesting an effect need not consume the input.
/// let observing = ExternalSurface::new(2).on_input(Arc::new(|_, ctx| {
///     ctx.invalidate_paint();
///     EventHandled::Ignored
/// }));
/// ```
pub type ExternalInputFn = dyn Fn(&UiEvent, &mut EventCtx) -> EventHandled;

/// Callback invoked when an external-paint leaf leaves the mounted tree.
pub type ExternalUnmountFn = dyn Fn();

/// A focusable widget that delegates painting to an externally-owned
/// renderer identified by [`ExternalDrawId`].
///
/// During the paint pass, produces a single [`Primitive::External`].
/// During event handling, either invokes an optional [`ExternalInputFn`] or
/// queues input for deferred delivery after the event walk completes.
#[derive(Clone)]
pub struct ExternalSurface {
    draw_id: ExternalDrawId,
    handler: Option<Arc<ExternalDrawFn<'static>>>,
    schedule: Option<Arc<ExternalScheduleFn>>,
    ime: Option<Arc<crate::scene::primitive::ExternalImeFn>>,
    on_input: Option<Arc<ExternalInputFn>>,
    on_unmount: Option<Arc<ExternalUnmountFn>>,
    child: Option<View>,
}

impl ExternalSurface {
    pub fn new(draw_id: impl Into<ExternalDrawId>) -> Self {
        ExternalSurface {
            draw_id: draw_id.into(),
            ime: None,
            handler: None,
            schedule: None,
            on_input: None,
            on_unmount: None,
            child: None,
        }
    }

    /// Returns the stable identifier used by this external paint widget.
    pub const fn draw_id(&self) -> ExternalDrawId {
        self.draw_id
    }

    /// Sets the renderer invoked for this widget's external primitive.
    pub fn on_draw(mut self, handler: Arc<ExternalDrawFn<'static>>) -> Self {
        self.handler = Some(handler);
        self
    }

    /// Sets the schedule provider consulted before idle wait selection.
    pub fn schedule_with(mut self, schedule: Arc<ExternalScheduleFn>) -> Self {
        self.schedule = Some(schedule);
        self
    }

    /// Sets a callback invoked when this retained external leaf is unmounted.
    pub fn on_unmount(mut self, on_unmount: Arc<ExternalUnmountFn>) -> Self {
        self.on_unmount = Some(on_unmount);
        self
    }

    /// Sets the IME effect provider consulted while this external leaf owns focus.
    pub fn ime_with(mut self, ime: Arc<crate::scene::primitive::ExternalImeFn>) -> Self {
        self.ime = Some(ime);
        self
    }

    /// Sets an in-tree input adapter. When present, events are not queued for
    /// deferred external drain.
    pub fn on_input(mut self, on_input: Arc<ExternalInputFn>) -> Self {
        self.on_input = Some(on_input);
        self
    }

    /// Replaces the optional child slot.
    pub fn child(mut self, child: impl crate::IntoChildView) -> Self {
        self.child = Some(child.into_child_view());
        self
    }

    /// Returns whether this external surface widget can receive focus.
    pub fn is_focusable(&self) -> bool {
        true
    }
}

impl Component for ExternalSurface {
    fn build(&self, cx: &mut BuildCx) -> View {
        if let Some(handler) = &self.handler {
            cx.register_external_draw(self.draw_id, Arc::clone(handler));
        }
        if let Some(schedule) = &self.schedule {
            cx.register_external_schedule(self.draw_id, Arc::clone(schedule));
        }
        if let Some(ime) = &self.ime {
            cx.register_external_ime(self.draw_id, Arc::clone(ime));
        }
        View::new(self.clone(), self.child.iter().cloned().collect(), None)
    }
}

impl crate::WithChildren for ExternalSurface {
    type Capability = crate::SingleChild;
    fn with_children(
        mut self,
        children: crate::Children,
    ) -> Result<Self, crate::ChildConstructionError> {
        children.into_single("ExternalSurface", &mut self.child)?;
        Ok(self)
    }
}

impl AnyView for ExternalSurface {
    fn intrinsic_size(&self, constraints: BoxConstraints, _metrics: &TextMetrics) -> Size {
        // Fill bounded axes; an unbounded axis has no viewport to fill.
        constraints.fill_bounded(Size::ZERO)
    }

    fn layout_children(
        &self,
        constraints: BoxConstraints,
        child_sizes: &[Size],
        _metrics: &TextMetrics,
    ) -> (Size, Vec<Point>) {
        // The optional child starts at the origin. On unbounded axes preserve its
        // natural extent rather than publishing infinite external draw geometry.
        let natural = child_sizes.iter().fold(Size::ZERO, |size, child| {
            Size::new(size.width.max(child.width), size.height.max(child.height))
        });
        let own = constraints.fill_bounded(natural);
        let positions = vec![Point::ZERO; child_sizes.len()];
        (own, positions)
    }

    fn paint_primitives(&self, rect: Rect, _metrics: &TextMetrics) -> Vec<Primitive> {
        vec![Primitive::External {
            draw: self.draw_id,
            rect,
        }]
    }

    fn focus_metadata(&self) -> Option<FocusMetadata> {
        Some(FocusMetadata {
            enabled: true,
            participates_in_tab_traversal: false,
            order: 0,
            handle: None,
        })
    }
    fn external_draw_id(&self) -> Option<ExternalDrawId> {
        Some(self.draw_id)
    }

    fn handle_event(&self, event: &UiEvent, ctx: &mut EventCtx, _rect: Rect) -> EventHandled {
        if matches!(event, UiEvent::Pointer(pointer) if pointer.phase == PointerPhase::Down)
            && let Some(fiber) = ctx.current_fiber()
        {
            ctx.request_focus(fiber);
        }

        if let Some(on_input) = &self.on_input {
            return on_input(event, ctx);
        }

        // Queue for deferred delivery to the App via Runtime.
        ctx.queue_external_input(self.draw_id, event.clone());
        EventHandled::Handled
    }

    fn unmount(&self) {
        if let Some(on_unmount) = &self.on_unmount {
            on_unmount();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::event::{Key, KeyboardEvent, Modifiers};
    use std::sync::Arc;

    #[test]
    fn external_surface_is_focusable() {
        let cp = ExternalSurface::new(1);
        assert!(cp.is_focusable());
    }

    #[test]
    fn external_surface_paint_primitives_contains_external() {
        let cp = ExternalSurface::new(42);
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

    #[derive(Clone)]
    struct ConstraintParent {
        constraints: BoxConstraints,
        child: View,
    }

    impl Component for ConstraintParent {
        fn build(&self, _cx: &mut BuildCx) -> View {
            View::new(self.clone(), vec![self.child.clone()], None)
        }
    }

    impl AnyView for ConstraintParent {
        fn child_constraints(&self, _constraints: BoxConstraints) -> BoxConstraints {
            self.constraints
        }
        fn layout_children(
            &self,
            constraints: BoxConstraints,
            sizes: &[Size],
            _metrics: &TextMetrics,
        ) -> (Size, Vec<Point>) {
            (constraints.constrain(sizes[0]), vec![Point::ZERO])
        }
    }

    fn unbounded_runtime(
        constraints: BoxConstraints,
        child: impl Component + 'static,
    ) -> crate::runtime::Runtime {
        let mut runtime = crate::runtime::Runtime::new();
        runtime.set_root(ConstraintParent {
            constraints,
            child: View::deferred(child),
        });
        runtime.update(std::time::Instant::now());
        runtime
    }

    #[test]
    fn external_surface_has_finite_per_axis_unbounded_fallback_in_real_layout() {
        use crate::runtime::DEFAULT_TEXT_METRICS;
        use crate::widgets::sized_box::SizedBox;
        for (max, expected) in [
            (Size::new(f32::INFINITY, 90.0), Size::new(40.0, 90.0)),
            (Size::new(80.0, f32::INFINITY), Size::new(80.0, 30.0)),
            (
                Size::new(f32::INFINITY, f32::INFINITY),
                Size::new(40.0, 30.0),
            ),
        ] {
            let constraints = BoxConstraints {
                min: Size::new(7.0, 11.0),
                max,
            };
            let runtime = unbounded_runtime(
                constraints,
                ExternalSurface::new(1).child(SizedBox::new(Size::new(40.0, 30.0)).unwrap()),
            );
            let root = runtime.arena().get(runtime.root_id().unwrap()).unwrap();
            let child = runtime.arena().get(root.children()[0]).unwrap();
            assert_eq!(child.layout_rect().unwrap().size(), expected);
            assert!(
                matches!(runtime.pending_delta().unwrap().added[0].primitive,
                Primitive::External { rect, draw } if draw == 1 && rect.size() == expected)
            );
            let natural =
                ExternalSurface::new(1).intrinsic_size(constraints, &DEFAULT_TEXT_METRICS);
            assert_eq!(
                natural.width,
                if max.width.is_finite() {
                    max.width
                } else {
                    7.0
                }
            );
            assert_eq!(
                natural.height,
                if max.height.is_finite() {
                    max.height
                } else {
                    11.0
                }
            );
        }
        let runtime = unbounded_runtime(
            BoxConstraints {
                min: Size::new(7.0, 11.0),
                max: Size::new(f32::INFINITY, f32::INFINITY),
            },
            ExternalSurface::new(1),
        );
        assert!(
            matches!(runtime.pending_delta().unwrap().added[0].primitive,
            Primitive::External { rect, draw } if draw == 1 && rect.size() == Size::new(7.0, 11.0))
        );
    }

    #[test]
    fn unbounded_expanded_external_surface_falls_back_to_finite_content() {
        use crate::widgets::sized_box::SizedBox;
        use crate::{Expanded, Row};
        let runtime = unbounded_runtime(
            BoxConstraints::loose(Size::new(f32::INFINITY, f32::INFINITY)),
            Row::new().push_child(Expanded::new().child(
                ExternalSurface::new(1).child(SizedBox::new(Size::new(33.0, 21.0)).unwrap()),
            )),
        );
        let root = runtime.arena().get(runtime.root_id().unwrap()).unwrap();
        assert_eq!(root.layout_rect().unwrap().size(), Size::new(33.0, 21.0));
        assert!(
            matches!(runtime.pending_delta().unwrap().added[0].primitive,
            Primitive::External { rect, draw } if draw == 1 && rect.size() == Size::new(33.0, 21.0))
        );
    }

    #[test]
    fn external_surface_fills_max_constraints() {
        let cp = ExternalSurface::new(1);
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let size = cp.intrinsic_size(constraints, &crate::runtime::DEFAULT_TEXT_METRICS);
        assert_eq!(size, Size::new(800.0, 600.0));
    }

    #[test]
    fn external_surface_handle_event_returns_handled() {
        let cp = ExternalSurface::new(1);
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
    fn external_surface_build_produces_view() {
        let cp = ExternalSurface::new(7);
        let mut cx = BuildCx::stub();
        let view = cp.build(&mut cx);
        let (_inner, children, _key) = view.decompose();
        assert!(children.is_empty());
    }

    #[test]
    fn should_register_handler_when_external_surface_is_built_with_one() {
        // Arrange: a ExternalSurface with a stable handler and real build context.
        let handler: Arc<ExternalDrawFn<'static>> = Arc::new(|_, _, _, _, _| {});
        let external_surface = ExternalSurface::new(42).on_draw(Arc::clone(&handler));
        let mut cx = BuildCx::stub();

        // Act: build the component.
        external_surface.build(&mut cx);

        // Assert: the handler is registered for the component's draw ID.
        assert_eq!(cx.externals.draws.len(), 1);
        assert_eq!(cx.externals.draws[0].0, 42);
        assert!(Arc::ptr_eq(&cx.externals.draws[0].1, &handler));
        assert!(cx.externals.schedules.is_empty());
    }

    #[test]
    fn should_not_register_handler_when_external_surface_has_none() {
        // Arrange: a ExternalSurface using its default, handler-free configuration.
        let external_surface = ExternalSurface::new(42);
        let mut cx = BuildCx::stub();

        // Act: build the component.
        external_surface.build(&mut cx);

        // Assert: no external draw registration is produced.
        assert!(cx.externals.draws.is_empty());
        assert!(cx.externals.schedules.is_empty());
    }

    #[test]
    fn should_register_schedule_when_external_surface_is_built_with_one() {
        use crate::scene::primitive::ExternalScheduleDemand;

        // Arrange
        let schedule: Arc<ExternalScheduleFn> = Arc::new(|_, _| ExternalScheduleDemand::empty());
        let external_surface = ExternalSurface::new(7).schedule_with(Arc::clone(&schedule));
        let mut cx = BuildCx::stub();

        // Act
        external_surface.build(&mut cx);

        // Assert
        assert!(cx.externals.draws.is_empty());
        assert_eq!(cx.externals.schedules.len(), 1);
        assert_eq!(cx.externals.schedules[0].0, 7);
        assert!(Arc::ptr_eq(&cx.externals.schedules[0].1, &schedule));
    }

    #[test]
    fn should_register_handler_and_schedule_when_both_are_configured() {
        use crate::scene::primitive::ExternalScheduleDemand;

        // Arrange
        let handler: Arc<ExternalDrawFn<'static>> = Arc::new(|_, _, _, _, _| {});
        let schedule: Arc<ExternalScheduleFn> = Arc::new(|_, _| ExternalScheduleDemand::empty());
        let external_surface = ExternalSurface::new(11)
            .on_draw(Arc::clone(&handler))
            .schedule_with(Arc::clone(&schedule));
        let mut cx = BuildCx::stub();

        // Act
        external_surface.build(&mut cx);

        // Assert
        assert_eq!(cx.externals.draws.len(), 1);
        assert_eq!(cx.externals.draws[0].0, 11);
        assert!(Arc::ptr_eq(&cx.externals.draws[0].1, &handler));
        assert_eq!(cx.externals.schedules.len(), 1);
        assert_eq!(cx.externals.schedules[0].0, 11);
        assert!(Arc::ptr_eq(&cx.externals.schedules[0].1, &schedule));
    }

    #[test]
    fn should_invoke_on_input_adapter_instead_of_queueing() {
        use std::sync::atomic::{AtomicBool, Ordering};

        // Arrange
        let called = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&called);
        let cp = ExternalSurface::new(1).on_input(Arc::new(move |_event, _ctx| {
            flag.store(true, Ordering::Relaxed);
            EventHandled::Handled
        }));
        let event = UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Enter,
            modifiers: Modifiers::default(),
        });
        let mut ctx = EventCtx::new();

        // Act
        let result = cp.handle_event(
            &event,
            &mut ctx,
            Rect::from_min_size(Point::ZERO, Size::new(100.0, 100.0)),
        );

        // Assert
        assert_eq!(result, EventHandled::Handled);
        assert!(called.load(Ordering::Relaxed));
    }

    #[test]
    fn should_return_adapter_result_when_on_input_returns_ignored() {
        // Arrange
        let cp = ExternalSurface::new(1).on_input(Arc::new(|_event, _ctx| EventHandled::Ignored));
        let event = UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Tab,
            modifiers: Modifiers::default(),
        });
        let mut ctx = EventCtx::new();

        // Act
        let result = cp.handle_event(
            &event,
            &mut ctx,
            Rect::from_min_size(Point::ZERO, Size::new(100.0, 100.0)),
        );

        // Assert
        assert_eq!(result, EventHandled::Ignored);
    }
}
