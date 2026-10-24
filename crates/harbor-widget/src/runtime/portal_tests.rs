use super::*;
use crate::construction::ComponentExt as _;
use crate::input::event::{Modifiers, PointerButton, PointerEvent};
use crate::scene::primitive::{Color, Primitive};
use crate::view::{AnyView, Component, View};
use crate::widgets::{align::Align, sized_box::SizedBox};
use crate::{
    Alignment, BoxDecoration, ClipBehavior, Column, ConstrainedBox, DecoratedBox, Expanded,
    Notification, Portal,
};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Target;
impl AnyView for Target {
    fn intrinsic_size(&self, constraints: BoxConstraints, _: &TextMetrics) -> Size {
        constraints.constrain(Size::new(40.0, 30.0))
    }
    fn paint_primitives(&self, rect: Rect, _: &TextMetrics) -> Vec<Primitive> {
        vec![Primitive::Quad {
            rect,
            color: Color::RED,
            corner_radius: 0.0,
        }]
    }
    fn hit_test(&self, point: Point, rect: Rect) -> bool {
        rect.contains(point)
    }
}

fn target() -> View {
    View::new(Target, vec![], None)
}
fn red_rect(rt: &Runtime) -> Rect {
    rt.scene_graph
        .items()
        .iter()
        .find_map(|item| match item.primitive {
            Primitive::Quad { rect, color, .. } if color == Color::RED => Some(rect),
            _ => None,
        })
        .expect("escaped arbitrary content painted")
}

#[test]
fn portal_at_root_and_beneath_clipping_parent_uses_root_geometry_and_hit_testing() {
    for nested in [false, true] {
        let mut rt = Runtime::new();
        rt.set_viewport(Viewport::new(300, 200, 1.0));
        let portal =
            Portal::new().child(Align::new(crate::Alignment2D::BOTTOM_RIGHT).child(target()));
        if nested {
            rt.set_root(
                ConstrainedBox::new()
                    .max_width(15.0)
                    .max_height(10.0)
                    .child(
                        DecoratedBox::new(BoxDecoration::new())
                            .clip_behavior(ClipBehavior::HardEdge)
                            .child(portal),
                    ),
            );
        } else {
            rt.set_root(portal);
        }
        rt.update(Instant::now());
        assert_eq!(
            red_rect(&rt),
            Rect::from_min_size(Point::new(260.0, 170.0), Size::new(40.0, 30.0))
        );
        let item = rt
            .scene_graph
            .items()
            .iter()
            .find(|item| {
                matches!(
                    item.primitive,
                    Primitive::Quad {
                        color: Color::RED,
                        ..
                    }
                )
            })
            .unwrap();
        assert_eq!(
            item.clips.len(),
            1,
            "only root clipping, not declaring parent clip"
        );
        assert!(item.clips[0].contains(Point::new(280.0, 180.0)));
        let hit =
            EventRouter::hit_test_walk(&rt.arena, rt.root_id.unwrap(), Point::new(280.0, 180.0))
                .unwrap();
        assert_eq!(
            rt.arena.get(hit).unwrap().widget_type(),
            std::any::TypeId::of::<Target>()
        );
        assert!(
            EventRouter::hit_test_walk(&rt.arena, rt.root_id.unwrap(), Point::new(310.0, 180.0))
                .is_none()
        );
        assert!(
            EventRouter::hit_test_walk(&rt.arena, rt.root_id.unwrap(), Point::new(20.0, 20.0))
                .is_none(),
            "empty overlay has no target"
        );
        rt.set_viewport(Viewport::new(1000, 600, 2.0));
        rt.update(Instant::now());
        assert_eq!(red_rect(&rt).min, Point::new(460.0, 270.0));
        assert!(
            EventRouter::hit_test_walk(&rt.arena, rt.root_id.unwrap(), Point::new(480.0, 280.0))
                .is_some()
        );
        assert!(
            EventRouter::hit_test_walk(&rt.arena, rt.root_id.unwrap(), Point::new(280.0, 180.0))
                .is_none()
        );
    }
}

#[test]
fn portal_lifetime_updates_and_remounts_without_layout_slots_or_stale_hits() {
    let visible = Rc::new(Cell::new(true));
    let width = Rc::new(Cell::new(40.0));
    let mut rt = Runtime::new();
    rt.set_viewport(Viewport::new(300, 200, 1.0));
    rt.set_root({
        let visible = visible.clone();
        let width = width.clone();
        move |cx: &mut BuildCx| {
            let mut column = Column::new()
                .cross_axis_alignment(Alignment::Stretch)
                .gap(17.0)
                .unwrap()
                .push_child(
                    Expanded::new().child(SizedBox::new(Size::ZERO).unwrap().color(Color::BLUE)),
                );
            if visible.get() {
                column = column.push_child(
                    Portal::new().child(
                        Align::new(crate::Alignment2D::BOTTOM_RIGHT).child(
                            SizedBox::new(Size::new(width.get(), 30.0))
                                .unwrap()
                                .color(Color::RED),
                        ),
                    ),
                );
            }
            column.build(cx)
        }
    });
    rt.update(Instant::now());
    let blue = |rt: &Runtime| {
        rt.scene_graph
            .items()
            .iter()
            .find_map(|i| match i.primitive {
                Primitive::Quad {
                    rect,
                    color: Color::BLUE,
                    ..
                } => Some(rect),
                _ => None,
            })
            .unwrap()
    };
    let allocation = blue(&rt);
    assert_eq!(
        allocation.size(),
        Size::new(300.0, 200.0),
        "Portal contributes no flex gap"
    );
    let old_hit =
        EventRouter::hit_test_walk(&rt.arena, rt.root_id.unwrap(), Point::new(280.0, 180.0))
            .unwrap();
    let initial_ids: Vec<_> = rt.scene_graph.items().iter().map(|i| i.id).collect();
    width.set(70.0);
    rt.invalidate_external(ExternalInvalidation::new());
    rt.update(Instant::now());
    assert_eq!(red_rect(&rt).min.x, 230.0);
    assert_eq!(blue(&rt), allocation);
    assert!(
        rt.scene_graph.items().last().unwrap().paint_order > rt.scene_graph.items()[0].paint_order
    );
    visible.set(false);
    rt.invalidate_external(ExternalInvalidation::new());
    rt.update(Instant::now());
    assert_eq!(rt.scene_graph.items().len(), 1);
    assert_eq!(blue(&rt), allocation);
    assert!(
        !rt.arena.contains(old_hit),
        "owner removal unmounts escaped hit identity"
    );
    assert_ne!(
        EventRouter::hit_test_walk(&rt.arena, rt.root_id.unwrap(), Point::new(280.0, 180.0)),
        Some(old_hit)
    );
    assert!(crate::fiber::portal_roots(&rt.arena, rt.root_id.unwrap()).is_empty());
    assert!(
        rt.pending_delta()
            .unwrap()
            .removed
            .iter()
            .any(|id| *id == initial_ids[1])
            || !rt
                .pending_delta()
                .unwrap()
                .added
                .iter()
                .any(|i| i.id == initial_ids[1])
    );
    visible.set(true);
    rt.invalidate_external(ExternalInvalidation::new());
    rt.update(Instant::now());
    assert_eq!(red_rect(&rt).min.x, 230.0);
    assert_eq!(blue(&rt), allocation);
    rt.clear_root();
    assert!(rt.scene_graph.items().is_empty());
}

fn pointer(rt: &mut Runtime, position: Point, phase: PointerPhase) {
    rt.dispatch(UiEvent::Pointer(PointerEvent {
        position,
        phase,
        button: PointerButton::Left,
        pointer_id: 1,
        modifiers: Modifiers::default(),
    }));
}

#[test]
fn notification_lifetime_is_caller_owned_and_close_follows_resized_position() {
    let dismissed = Arc::new(AtomicUsize::new(0));
    let mut rt = Runtime::new();
    rt.set_viewport(Viewport::new(900, 600, 1.0));
    let count = dismissed.clone();
    rt.set_root(Portal::new().child(Notification::new("Action required: requested changes were not applied. Previously accepted settings remain effective.", move || { count.fetch_add(1, Ordering::Relaxed); }).placement(crate::widgets::notification::NotificationPlacement { horizontal: Alignment::End, vertical: Alignment::Start, inset: 12.0 })));
    rt.update(Instant::now());
    assert!(rt.events.input().focused.is_none());
    assert!(
        EventRouter::hit_test_walk(&rt.arena, rt.root_id.unwrap(), Point::new(10.0, 500.0))
            .is_none()
    );
    rt.update(Instant::now() + std::time::Duration::from_secs(3600));
    assert!(
        !rt.scene_graph.items().is_empty(),
        "reusable presentation does not impose the application's timeout"
    );
    rt.set_viewport(Viewport::new(1200, 800, 2.0));
    rt.update(Instant::now());
    let close = rt
        .scene_graph
        .items()
        .iter()
        .find_map(|item| match item.primitive {
            Primitive::Icon {
                icon: crate::LucideIcon::X,
                rect,
                ..
            } => Some(rect),
            _ => None,
        })
        .expect("vector close follows resized overlay geometry");
    let position = Point::new(
        (close.min.x + close.max.x) / 2.0,
        (close.min.y + close.max.y) / 2.0,
    );
    pointer(&mut rt, position, PointerPhase::Down);
    pointer(&mut rt, position, PointerPhase::Up);
    assert_eq!(dismissed.load(Ordering::Relaxed), 1);
    assert!(
        rt.events.input().focused.is_none(),
        "dismissal never requests focus"
    );
}

#[test]
fn nested_portals_paint_once_in_matching_hit_order_and_escape_inner_clip() {
    let mut rt = Runtime::new();
    rt.set_viewport(Viewport::new(300, 200, 1.0));
    rt.set_root(
        Portal::new().child(
            ConstrainedBox::new()
                .max_width(10.0)
                .max_height(10.0)
                .child(
                    DecoratedBox::new(BoxDecoration::new())
                        .clip_behavior(ClipBehavior::HardEdge)
                        .child(
                            Portal::new().child(
                                Align::new(crate::Alignment2D::BOTTOM_RIGHT).child(target()),
                            ),
                        ),
                ),
        ),
    );
    rt.update(Instant::now());
    assert_eq!(rt.scene_graph.items().len(), 1);
    assert_eq!(red_rect(&rt).min, Point::new(260.0, 170.0));
    assert!(
        EventRouter::hit_test_walk(&rt.arena, rt.root_id.unwrap(), Point::new(280.0, 180.0))
            .is_some()
    );
}

#[test]
fn failed_escaped_measurement_does_not_partially_commit_ordinary_geometry() {
    struct FailingLeaf(bool);
    impl AnyView for FailingLeaf {
        fn intrinsic_size(&self, _: BoxConstraints, _: &TextMetrics) -> Size {
            Size::new(if self.0 { f32::NAN } else { 40.0 }, 30.0)
        }
    }
    let bad = Rc::new(Cell::new(false));
    let mut rt = Runtime::new();
    let state = bad.clone();
    rt.set_root(move |cx: &mut BuildCx| {
        Column::new()
            .push_child(
                SizedBox::new(Size::new(if state.get() { 200.0 } else { 100.0 }, 60.0))
                    .unwrap()
                    .color(Color::BLUE),
            )
            .push_child(Portal::new().child(View::new(FailingLeaf(state.get()), vec![], None)))
            .build(cx)
    });
    rt.update(Instant::now());
    let root = rt.root_id.unwrap();
    let previous = rt.arena.get(root).unwrap().layout_rect();
    bad.set(true);
    rt.invalidate_external(ExternalInvalidation::new());
    rt.update(Instant::now());
    assert_eq!(rt.arena.get(root).unwrap().layout_rect(), previous);
    assert!(rt.arena.get(root).unwrap().layout_error().is_some());
    bad.set(false);
    rt.invalidate_external(ExternalInvalidation::new());
    rt.update(Instant::now());
    assert!(rt.arena.get(root).unwrap().layout_error().is_none());
}

#[test]
fn focusing_escaped_content_does_not_scroll_its_declaring_scroll_view() {
    let scroll = crate::ScrollController::new();
    let handle = crate::FocusHandle::new();
    let content = Column::new()
        .push_child(SizedBox::new(Size::new(100.0, 600.0)).unwrap())
        .push_child(
            Portal::new().child(
                Align::new(crate::Alignment2D::BOTTOM_RIGHT)
                    .child(crate::Focus::new(target()).handle(handle)),
            ),
        );
    let mut rt = Runtime::new();
    rt.set_viewport(Viewport::new(300, 200, 1.0));
    rt.set_root(
        ConstrainedBox::new()
            .max_width(100.0)
            .max_height(50.0)
            .child(
                crate::ScrollView::new()
                    .controller(scroll.clone())
                    .child(content),
            ),
    );
    rt.update(Instant::now());
    assert_eq!(scroll.offset(), 0.0);
    rt.request_focus(&handle);
    rt.update(Instant::now());
    assert!(rt.events.input().focused.is_some());
    assert_eq!(
        scroll.offset(),
        0.0,
        "logical ancestors must not reveal root-relative content"
    );
}
