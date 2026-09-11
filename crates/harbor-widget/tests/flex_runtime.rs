use harbor_widget::fiber::FiberId;
use harbor_widget::input::event::{PointerButton, PointerEvent, PointerPhase, UiEvent};
use harbor_widget::layout::{Alignment, Point, Rect, Size};
use harbor_widget::renderer::Viewport;
use harbor_widget::runtime::{DEFAULT_TEXT_METRICS, Runtime};
use harbor_widget::scene::primitive::{
    Color, ExternalScheduleDemand, ExternalScheduleFn, Primitive,
};
use harbor_widget::signal::Signal;
use harbor_widget::text::TextMetrics;
use harbor_widget::view::{BuildCx, Component, View};
use harbor_widget::widgets::custom_paint::CustomPaint;
use harbor_widget::widgets::padding::Padding;
use harbor_widget::widgets::text_label::TextLabel;
use harbor_widget::{
    Axis, BorderRadius, BoxDecoration, ClipBehavior, ConstrainedBox, DecoratedBox, Expanded, Flex,
    Flexible, Row, Separator,
};
use std::cell::{Cell, RefCell};
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

fn update(runtime: &mut Runtime) -> bool {
    runtime.update(Instant::now()).request_redraw
}

fn pointer(runtime: &mut Runtime, x: f32, y: f32) -> Vec<u64> {
    runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
        Point::new(x, y),
        PointerPhase::Move,
        PointerButton::Left,
        1,
    )));
    runtime
        .drain_external_input()
        .into_iter()
        .filter_map(|(id, event)| matches!(event, UiEvent::Pointer(_)).then_some(id))
        .collect()
}

fn fiber_at(runtime: &Runtime, path: &[usize]) -> FiberId {
    path.iter().fold(runtime.root_id().unwrap(), |id, &index| {
        runtime.arena().get(id).unwrap().children()[index]
    })
}

fn rect_at(runtime: &Runtime, path: &[usize]) -> Rect {
    runtime
        .arena()
        .get(fiber_at(runtime, path))
        .unwrap()
        .layout_rect()
        .unwrap()
}

fn tree_ids(runtime: &Runtime) -> Vec<FiberId> {
    let mut ids = Vec::new();
    let mut pending = vec![runtime.root_id().unwrap()];
    while let Some(id) = pending.pop() {
        ids.push(id);
        pending.extend_from_slice(runtime.arena().get(id).unwrap().children());
    }
    ids
}

fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.001, "{actual} != {expected}");
}

// These integration tests leave CPU deltas unencoded. Consumed-delta assertions
// live in runtime's unit tests, where the private consumption seam is available.
fn external_rect(runtime: &Runtime, draw_id: u64) -> Rect {
    let delta = runtime.pending_delta().unwrap();
    delta
        .modified
        .iter()
        .chain(&delta.added)
        .find_map(|item| match item.primitive {
            Primitive::External { draw, rect } if draw == draw_id => Some(rect),
            _ => None,
        })
        .unwrap()
}

fn bounded_external(width: f32, draw_id: u64) -> ConstrainedBox {
    ConstrainedBox::new()
        .min_width(width)
        .max_width(width)
        .child(CustomPaint::new(draw_id))
}

fn rail_root(rail: f32, content: impl Component + 'static) -> Row {
    Row::new()
        .cross_axis_alignment(Alignment::Stretch)
        .child(bounded_external(rail, 1))
        .child(Separator::vertical().color(Color::WHITE))
        .child(Expanded::new().child(content))
}

struct Counted<C> {
    inner: C,
    builds: Rc<Cell<usize>>,
    initializations: Rc<Cell<usize>>,
}

impl<C: Component> Component for Counted<C> {
    fn build(&self, cx: &mut BuildCx) -> View {
        self.builds.set(self.builds.get() + 1);
        let state = cx.use_state(|| {
            self.initializations.set(self.initializations.get() + 1);
            73_u32
        });
        assert_eq!(*state.read(), 73);
        self.inner.build(cx)
    }
}

#[test]
fn viewport_is_committed_before_input_without_rebuild_or_schedule_poll() {
    let builds = Rc::new(Cell::new(0));
    let initializations = Rc::new(Cell::new(0));
    let polls = Arc::new(AtomicUsize::new(0));
    let observed_polls = polls.clone();
    let schedule: Arc<ExternalScheduleFn> = Arc::new(move |_, _| {
        observed_polls.fetch_add(1, Ordering::SeqCst);
        ExternalScheduleDemand::empty()
    });
    let content = Counted {
        inner: CustomPaint::new(2).schedule(schedule),
        builds: builds.clone(),
        initializations: initializations.clone(),
    };
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(400, 80, 1.0));
    runtime.set_root(Counted {
        inner: rail_root(200.0, content),
        builds: builds.clone(),
        initializations: initializations.clone(),
    });
    assert!(update(&mut runtime));
    let ids = tree_ids(&runtime);
    let initial_builds = builds.get();
    let initial_polls = polls.load(Ordering::SeqCst);
    assert_eq!(rect_at(&runtime, &[2]).size().width, 199.0);

    runtime.set_viewport(Viewport::new(1000, 80, 1.0));
    // No update between viewport invalidation and the pointer event.
    assert_eq!(pointer(&mut runtime, 900.0, 40.0), [2]);
    let content_rect = rect_at(&runtime, &[2, 0]);
    assert_eq!(
        content_rect,
        Rect::from_min_size(Point::new(201.0, 0.0), Size::new(799.0, 80.0))
    );
    assert_eq!(external_rect(&runtime, 2), content_rect);
    assert_eq!(tree_ids(&runtime), ids);
    assert_eq!(builds.get(), initial_builds);
    assert_eq!(initializations.get(), initial_builds);
    assert_eq!(polls.load(Ordering::SeqCst), initial_polls);
    assert!(update(&mut runtime));
    assert_eq!(polls.load(Ordering::SeqCst), initial_polls + 1);
    assert!(!update(&mut runtime));
}

#[test]
fn resize_metrics_and_scale_changes_preserve_deferred_components_and_ids() {
    let builds = Rc::new(Cell::new(0));
    let initializations = Rc::new(Cell::new(0));
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(1000, 600, 1.0));
    runtime.set_root(Counted {
        inner: Row::new()
            .cross_axis_alignment(Alignment::Stretch)
            .child(Counted {
                inner: TextLabel::new("aa"),
                builds: builds.clone(),
                initializations: initializations.clone(),
            })
            .child(Expanded::new().child(CustomPaint::new(2))),
        builds: builds.clone(),
        initializations: initializations.clone(),
    });
    update(&mut runtime);
    let ids = tree_ids(&runtime);
    let initial_builds = builds.get();
    assert_eq!(rect_at(&runtime, &[1]).min.x, 24.0);
    assert_eq!(pointer(&mut runtime, 30.0, 10.0), [2]);

    let metrics = TextMetrics {
        cell_width: 20.0,
        ..DEFAULT_TEXT_METRICS
    };
    runtime.set_text_metrics(metrics);
    assert!(pointer(&mut runtime, 30.0, 10.0).is_empty());
    assert_eq!(rect_at(&runtime, &[1]).min.x, 44.0);
    assert!(update(&mut runtime));
    runtime.set_text_metrics(metrics);
    assert!(!update(&mut runtime));

    for scale in [1.0, 1.25, 1.5, 2.0] {
        let viewport = Viewport::new(1001, 603, scale);
        assert!(runtime.set_viewport(viewport.clone()));
        assert!(update(&mut runtime));
        let content = rect_at(&runtime, &[1]);
        assert_close(content.min.x, 44.0);
        assert_close(content.size().width, viewport.logical_size.width - 44.0);
        assert_close(content.max.x, viewport.logical_size.width);
        assert_close(content.size().height, viewport.logical_size.height);
        assert_eq!(tree_ids(&runtime), ids);
        assert_eq!(builds.get(), initial_builds);
        assert_eq!(initializations.get(), initial_builds);
        assert!(!runtime.set_viewport(viewport));
        assert!(!update(&mut runtime));
    }
    // Physical/scale-only transitions still refresh rendering state.
    let logical = runtime.current_viewport().unwrap().logical_size;
    assert!(runtime.set_viewport(Viewport {
        logical_size: logical,
        physical_size: (2002, 1206),
        scale_factor: 4.0,
    }));
    assert!(update(&mut runtime));
    assert_close(rect_at(&runtime, &[1]).max.x, logical.width);
    assert_eq!(builds.get(), initial_builds);
}

struct SignalFactor {
    exposed: Rc<RefCell<Option<Signal<u32>>>>,
    initializations: Rc<Cell<usize>>,
}

impl Component for SignalFactor {
    fn build(&self, cx: &mut BuildCx) -> View {
        let factor = cx.use_state(|| {
            self.initializations.set(self.initializations.get() + 1);
            1_u32
        });
        *self.exposed.borrow_mut() = Some(factor.clone());
        let value = *factor.read();
        Expanded::new()
            .factor(NonZeroU32::new(value).unwrap())
            .child(CustomPaint::new(1))
            .build(cx)
    }
}

#[test]
fn signal_factor_reallocates_siblings_before_the_next_pointer_event() {
    let exposed = Rc::new(RefCell::new(None));
    let initializations = Rc::new(Cell::new(0));
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(400, 40, 1.0));
    runtime.set_root(
        Row::new()
            .cross_axis_alignment(Alignment::Stretch)
            .child(SignalFactor {
                exposed: exposed.clone(),
                initializations: initializations.clone(),
            })
            .child(Expanded::new().child(CustomPaint::new(2))),
    );
    update(&mut runtime);
    let ids = tree_ids(&runtime);
    assert_eq!(pointer(&mut runtime, 250.0, 20.0), [2]);
    let signal = exposed.borrow().as_ref().unwrap().clone();
    signal.set(3);
    assert_eq!(pointer(&mut runtime, 250.0, 20.0), [1]);
    assert_eq!(rect_at(&runtime, &[0]).size().width, 300.0);
    assert_eq!(
        rect_at(&runtime, &[1]),
        Rect::from_min_size(Point::new(300.0, 0.0), Size::new(100.0, 40.0))
    );
    assert_eq!(tree_ids(&runtime), ids);
    assert_eq!(initializations.get(), 1);
    assert!(update(&mut runtime));
    assert!(!update(&mut runtime));
    signal.set(1);
    assert!(update(&mut runtime));
    assert_eq!(rect_at(&runtime, &[1]).size().width, 200.0);
    assert_eq!(initializations.get(), 1);
}

struct SignalContent(Rc<RefCell<Option<Signal<String>>>>);

impl Component for SignalContent {
    fn build(&self, cx: &mut BuildCx) -> View {
        let text = cx.use_state(|| "a".to_owned());
        *self.0.borrow_mut() = Some(text.clone());
        TextLabel::new(text.read().clone()).build(cx)
    }
}

#[test]
fn signal_content_changes_relayout_the_full_root_including_siblings() {
    let exposed = Rc::new(RefCell::new(None));
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(200, 40, 1.0));
    runtime.set_root(
        Row::new()
            .child(SignalContent(exposed.clone()))
            .child(Expanded::new().child(CustomPaint::new(2))),
    );
    update(&mut runtime);
    let ids = tree_ids(&runtime);
    assert_eq!(rect_at(&runtime, &[1]).min.x, 14.0);
    exposed.borrow().as_ref().unwrap().set("longer".to_owned());
    assert!(update(&mut runtime));
    assert_eq!(rect_at(&runtime, &[1]).min.x, 64.0);
    assert_eq!(rect_at(&runtime, &[1]).size().width, 136.0);
    assert_eq!(tree_ids(&runtime), ids);
    assert!(!update(&mut runtime));
}

struct ExternalWidth {
    width: Rc<Cell<f32>>,
    builds: Rc<Cell<usize>>,
}

impl Component for ExternalWidth {
    fn build(&self, cx: &mut BuildCx) -> View {
        self.builds.set(self.builds.get() + 1);
        bounded_external(self.width.get(), 1).build(cx)
    }
}

#[test]
fn external_build_invalidation_is_not_mistaken_for_layout_only_work() {
    let width = Rc::new(Cell::new(50.0));
    let builds = Rc::new(Cell::new(0));
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(200, 40, 1.0));
    runtime.set_root(
        Row::new()
            .child(ExternalWidth {
                width: width.clone(),
                builds: builds.clone(),
            })
            .child(Expanded::new().child(CustomPaint::new(2))),
    );
    update(&mut runtime);
    let ids = tree_ids(&runtime);
    assert_eq!(pointer(&mut runtime, 75.0, 20.0), [2]);
    width.set(100.0);
    assert!(
        runtime
            .invalidate_external(harbor_widget::ExternalInvalidation::new())
            .request_redraw
    );
    assert_eq!(pointer(&mut runtime, 75.0, 20.0), [1]);
    assert_eq!(rect_at(&runtime, &[1]).min.x, 100.0);
    assert_eq!(rect_at(&runtime, &[1]).size().width, 100.0);
    assert_eq!(builds.get(), 2);
    assert_eq!(tree_ids(&runtime), ids);
    assert!(update(&mut runtime));
    assert!(!update(&mut runtime));
}

fn overflowing_row() -> Row {
    Row::new()
        .cross_axis_alignment(Alignment::Stretch)
        .child(bounded_external(80.0, 1))
        .child(bounded_external(80.0, 2))
        .child(Expanded::new().child(CustomPaint::new(3)))
}

#[test]
fn visible_overflow_is_hit_within_child_allocation_but_zero_allocations_are_not() {
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(100, 40, 1.0));
    runtime.set_root(overflowing_row());
    update(&mut runtime);
    assert_eq!(rect_at(&runtime, &[]).size().width, 100.0);
    assert_eq!(rect_at(&runtime, &[1]).max.x, 160.0);
    assert_eq!(rect_at(&runtime, &[2]).size().width, 0.0);
    assert_eq!(external_rect(&runtime, 2), rect_at(&runtime, &[1, 0]));
    for (x, expected) in [
        (0.0, vec![1]),
        (79.999, vec![1]),
        (80.0, vec![2]),
        (100.0, vec![2]),
        (159.999, vec![2]),
        (160.0, vec![]),
    ] {
        assert_eq!(pointer(&mut runtime, x, 20.0), expected, "x={x}");
    }
    assert!(pointer(&mut runtime, 120.0, 40.0).is_empty());
    assert!(pointer(&mut runtime, 160.0, 0.0).is_empty());
}

#[test]
fn explicit_rounded_ancestor_clip_limits_overflow_hits() {
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(100, 40, 1.0));
    runtime.set_root(
        DecoratedBox::new(BoxDecoration::new().border_radius(BorderRadius::all(12.0).unwrap()))
            .clip_behavior(ClipBehavior::HardEdge)
            .child(overflowing_row()),
    );
    update(&mut runtime);
    assert_eq!(rect_at(&runtime, &[0, 1]).max.x, 160.0);
    assert_eq!(pointer(&mut runtime, 95.0, 20.0), [2]);
    assert!(pointer(&mut runtime, 120.0, 20.0).is_empty());
    assert!(pointer(&mut runtime, 0.0, 0.0).is_empty());
    assert_eq!(pointer(&mut runtime, 12.0, 0.0), [1]);
}

#[test]
fn loose_fit_does_not_make_unused_slot_hit_testable_or_redistribute_it() {
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(300, 40, 1.0));
    runtime.set_root(
        Flex::new(Axis::Horizontal)
            .cross_axis_alignment(Alignment::Stretch)
            .child(
                Flexible::new().child(
                    ConstrainedBox::new()
                        .max_width(20.0)
                        .child(CustomPaint::new(1)),
                ),
            )
            .child(Expanded::new().child(CustomPaint::new(2))),
    );
    update(&mut runtime);
    assert_eq!(rect_at(&runtime, &[0]).size().width, 20.0);
    assert_eq!(rect_at(&runtime, &[1]).min.x, 20.0);
    assert_eq!(rect_at(&runtime, &[1]).size().width, 150.0);
    assert_eq!(pointer(&mut runtime, 19.999, 20.0), [1]);
    assert_eq!(pointer(&mut runtime, 20.0, 20.0), [2]);
    assert_eq!(pointer(&mut runtime, 169.999, 20.0), [2]);
    assert!(pointer(&mut runtime, 170.0, 20.0).is_empty());
    assert!(pointer(&mut runtime, 250.0, 20.0).is_empty());
}

#[test]
fn separator_and_external_allocations_use_half_open_edges() {
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(200, 40, 1.0));
    runtime.set_root(rail_root(56.0, CustomPaint::new(2)));
    update(&mut runtime);
    assert_eq!(pointer(&mut runtime, 55.999, 20.0), [1]);
    assert!(pointer(&mut runtime, 56.0, 20.0).is_empty());
    assert!(pointer(&mut runtime, 56.999, 20.0).is_empty());
    assert_eq!(pointer(&mut runtime, 57.0, 20.0), [2]);
    assert!(pointer(&mut runtime, 200.0, 20.0).is_empty());
    assert!(pointer(&mut runtime, 100.0, 40.0).is_empty());
}

#[test]
fn zero_viewport_with_padding_has_no_hit_and_restores_without_replacing_fibers() {
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(0, 0, 1.0));
    runtime.set_root(
        Padding::new(10.0, 10.0, 10.0, 10.0)
            .child(Row::new().child(Expanded::new().child(CustomPaint::new(1)))),
    );
    update(&mut runtime);
    let ids = tree_ids(&runtime);
    assert_eq!(rect_at(&runtime, &[0, 0, 0]).size(), Size::ZERO);
    assert!(pointer(&mut runtime, 10.0, 10.0).is_empty());
    runtime.set_viewport(Viewport::new(100, 100, 1.0));
    assert_eq!(pointer(&mut runtime, 10.0, 10.0), [1]);
    assert_eq!(
        rect_at(&runtime, &[0, 0, 0]),
        Rect::from_min_size(Point::new(10.0, 10.0), Size::new(80.0, 80.0))
    );
    assert_eq!(tree_ids(&runtime), ids);
}
