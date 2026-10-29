use super::*;
use crate::input::event::{
    Key, KeyboardEvent, Modifiers, PointerButton, PointerEvent, PointerPhase,
};
use crate::input::event_ctx::EventCtx;
use crate::scene::primitive::ExternalScheduleDemand;
use crate::view::{Component, View};
use crate::widgets::button::Button;
use crate::widgets::column::Column;
use crate::widgets::external_surface::ExternalSurface;
use crate::widgets::focus::Focus;
use crate::widgets::focus_scope::FocusScope;
use crate::widgets::layout_observer::LayoutObserver;
use crate::widgets::sized_box::SizedBox;
use crate::widgets::text_label::TextLabel;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

fn now() -> Instant {
    Instant::now()
}

struct TeardownProbeRoot {
    drops: Arc<AtomicUsize>,
    callback_probe: Arc<()>,
}

impl Drop for TeardownProbeRoot {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

impl Component for TeardownProbeRoot {
    fn build(&self, cx: &mut BuildCx) -> View {
        let draw_probe = Arc::clone(&self.callback_probe);
        let schedule_probe = Arc::clone(&self.callback_probe);
        let layout_probe = Arc::clone(&self.callback_probe);
        let draw: Arc<ExternalDrawFn<'static>> = Arc::new(move |_, _, _, _, _| {
            let _ = &draw_probe;
        });
        let schedule: Arc<ExternalScheduleFn> = Arc::new(move |_, _| {
            let _ = &schedule_probe;
            ExternalScheduleDemand::empty()
        });
        LayoutObserver::new(move |_| {
            let _ = &layout_probe;
        })
        .child(
            ExternalSurface::new(91)
                .on_draw(draw)
                .schedule_with(schedule),
        )
        .build(cx)
    }
}

#[test]
fn clear_root_drops_tree_callbacks_and_resets_root_derived_state() {
    let drops = Arc::new(AtomicUsize::new(0));
    let callback_probe = Arc::new(());
    let weak_callback = Arc::downgrade(&callback_probe);
    let mut rt = Runtime::new();
    let viewport = Viewport::new(640, 480, 1.0);
    rt.set_viewport(viewport.clone());
    rt.set_root(TeardownProbeRoot {
        drops: Arc::clone(&drops),
        callback_probe,
    });
    assert!(rt.update(now()).request_redraw);
    assert!(rt.focus_first_focusable());
    assert!(rt.has_external_draws());
    assert!(weak_callback.upgrade().is_some());
    let scene_count = rt.scene_graph.item_count();
    assert!(scene_count > 0);
    let _presented_delta = rt.pending_delta.take().expect("initial scene delta");
    let old_root = rt.root_id.expect("mounted root");

    rt.clear_root();

    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert!(weak_callback.upgrade().is_none());
    assert!(rt.root_id.is_none());
    assert!(!rt.arena.contains(old_root));
    assert!(rt.external_draws.is_empty());
    assert!(rt.external_schedules.is_empty());
    assert!(rt.external_eligible.is_empty());
    assert!(rt.pending_layout_notifications.is_empty());
    assert!(rt.scene_graph.items().is_empty());
    assert!(rt.input().focused.is_none());
    assert_eq!(rt.current_viewport(), Some(&viewport));
    let delta = rt.pending_delta.as_ref().expect("root removal delta");
    assert_eq!(delta.removed.len(), scene_count);
    assert!(rt.pending_effects.request_redraw);
    assert_eq!(rt.pending_effects.cursor, Some(CursorEffect::Reset));
    assert_eq!(rt.pending_effects.ime, Some(ImeEffect::set_allowed(false)));

    rt.clear_root();
    rt.set_root(SizedBox::new(Size::new(10.0, 10.0)).unwrap());
    let new_root = rt.root_id.expect("replacement root");
    assert!(rt.arena.contains(new_root));
    assert_ne!(new_root, old_root);
}

#[test]
fn set_root_preserves_a_pending_stable_focus_request() {
    let handle = crate::widgets::FocusHandle::new();
    let mut rt = Runtime::new();
    rt.set_root(SizedBox::new(Size::new(1.0, 1.0)).unwrap());
    rt.request_focus(&handle);

    rt.set_root(
        Focus::new()
            .child(Button::text("replacement"))
            .focus_handle(handle),
    );

    assert!(rt.input().focused().is_some());
}

struct PanicOnUnsubscribe;

impl crate::signal::Hook for PanicOnUnsubscribe {
    fn unsubscribe_all(&self, _id: FiberId) {
        panic!("unsubscribe panic probe");
    }

    fn as_any_ref(&self) -> &dyn std::any::Any {
        self
    }
}

#[test]
fn clear_root_contains_unsubscribe_panics_after_detaching_old_state() {
    let mut rt = Runtime::new();
    rt.set_root(SizedBox::new(Size::new(1.0, 1.0)).unwrap());
    let root = rt.root_id.expect("mounted root");
    rt.arena
        .get_mut(root)
        .expect("mounted fiber")
        .hooks
        .push(Box::new(PanicOnUnsubscribe));

    rt.clear_root();

    assert!(rt.root_id.is_none());
    assert!(!rt.arena.contains(root));
    assert!(rt.external_draws.is_empty());
    assert!(rt.scene_graph.items().is_empty());
}
#[test]
fn dispatch_does_not_attribute_pending_focus_redraw_to_consumed_keyboard_event() {
    use crate::input::event_ctx::EventHandled;

    let deliveries = Arc::new(AtomicU64::new(0));
    let delivered = deliveries.clone();
    let mut rt = Runtime::new();
    rt.set_root(
        ExternalSurface::new(42).on_input(Arc::new(move |event, _ctx| {
            if matches!(event, UiEvent::Keyboard(_)) {
                delivered.fetch_add(1, Ordering::SeqCst);
            }
            // Like the terminal input gate: consume without requesting paint.
            EventHandled::Handled
        })),
    );
    assert!(rt.update(now()).request_redraw);
    assert!(rt.focus_first_focusable());
    let event = UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::PageUp,
        modifiers: Modifiers::default(),
    });

    assert_eq!(rt.dispatch(event.clone()), RuntimeEffects::default());
    assert_eq!(deliveries.load(Ordering::SeqCst), 1);
    assert!(rt.pending_effects.request_redraw);
    assert!(rt.update(now()).request_redraw);
    assert!(!rt.update(now()).request_redraw);
    assert_eq!(rt.dispatch(event), RuntimeEffects::default());
    assert!(!rt.update(now()).request_redraw);
}

#[test]
fn consumed_resize_delta_only_modifies_content_and_preserves_registrations() {
    use crate::layout::CrossAxisAlignment;
    use crate::scene::primitive::{Color, Primitive};
    use crate::view::View;
    use crate::{ConstrainedBox, Expanded, Row, Separator};
    use std::cell::Cell;
    use std::rc::Rc;

    struct RegisteredRoot(Rc<Cell<usize>>);
    impl Component for RegisteredRoot {
        fn build(&self, cx: &mut BuildCx) -> View {
            self.0.set(self.0.get() + 1);
            // Fresh registrations on each build make identity checks detect
            // even a rebuild which otherwise emits identical scene content.
            let handler: Arc<ExternalDrawFn<'static>> = Arc::new(|_, _, _, _, _| {});
            let schedule: Arc<ExternalScheduleFn> =
                Arc::new(|_, _| ExternalScheduleDemand::empty());
            Row::new()
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .push_child(
                    ConstrainedBox::new()
                        .min_width(200.0)
                        .max_width(200.0)
                        .child(
                            SizedBox::new(Size::new(200.0, 80.0))
                                .unwrap()
                                .background(Color::RED)
                                .expect("valid color"),
                        ),
                )
                .push_child(
                    Separator::vertical()
                        .color(Color::WHITE)
                        .expect("valid color"),
                )
                .push_child(
                    Expanded::new().child(
                        ExternalSurface::new(42)
                            .on_draw(handler)
                            .schedule_with(schedule),
                    ),
                )
                .build(cx)
        }
    }

    let builds = Rc::new(Cell::new(0));
    let mut rt = Runtime::new();
    rt.set_viewport(Viewport::new(1000, 80, 1.0));
    rt.set_root(RegisteredRoot(builds.clone()));
    rt.update(now());
    let initial = rt.pending_delta.take().unwrap();
    assert_eq!(initial.added.len(), 3);
    let initial_ids = initial.added.iter().map(|item| item.id).collect::<Vec<_>>();
    let external_id = initial
        .added
        .iter()
        .find_map(|item| {
            matches!(item.primitive, Primitive::External { draw, .. } if draw == 42)
                .then_some(item.id)
        })
        .unwrap();
    let draw = rt.external_draws[&42].clone();
    let schedule = rt.external_schedules[&42].clone();

    rt.set_viewport(Viewport::new(1200, 80, 1.0));
    let effects = rt.dispatch(UiEvent::Pointer(PointerEvent::new(
        Point::new(1199.0, 40.0),
        PointerPhase::Move,
        PointerButton::Left,
        0,
    )));
    assert_eq!(effects, RuntimeEffects::default());
    assert!(rt.pending_effects.request_redraw);
    let resized = rt.pending_delta.take().unwrap();
    assert!(resized.added.is_empty());
    assert!(resized.removed.is_empty());
    assert_eq!(resized.modified.len(), 1);
    assert_eq!(resized.modified[0].id, external_id);
    assert!(matches!(resized.modified[0].primitive,
        Primitive::External { rect, draw } if draw == 42 && rect.size() == Size::new(999.0, 80.0)));
    assert_eq!(
        rt.scene_graph
            .items()
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        initial_ids
    );
    assert_eq!(builds.get(), 1);
    assert_eq!(rt.external_draws.len(), 1);
    assert_eq!(rt.external_schedules.len(), 1);
    assert!(Arc::ptr_eq(&draw, &rt.external_draws[&42]));
    assert!(Arc::ptr_eq(&schedule, &rt.external_schedules[&42]));

    assert!(!rt.set_viewport(Viewport::new(1200, 80, 1.0)));
    assert!(rt.update(now()).request_redraw);
    assert!(!rt.update(now()).request_redraw);
    assert!(rt.pending_delta.is_none());
    assert!(rt.set_viewport(Viewport::new(2400, 160, 2.0)));
    assert!(rt.update(now()).request_redraw);
    assert!(rt.pending_delta.take().unwrap().is_empty());
    assert_eq!(builds.get(), 1);
    assert!(Arc::ptr_eq(&draw, &rt.external_draws[&42]));
    assert!(Arc::ptr_eq(&schedule, &rt.external_schedules[&42]));
}

#[test]
fn failed_layout_attempt_clears_tree_flags_and_waits_for_new_invalidation() {
    use crate::layout::Rect;
    use crate::view::{AnyView, Key, View};
    use crate::widgets::padding::Padding;
    use std::cell::Cell;
    use std::rc::Rc;

    #[derive(Clone)]
    struct FragileLeaf {
        fail: Rc<Cell<bool>>,
        measurements: Rc<Cell<usize>>,
    }
    impl Component for FragileLeaf {
        fn build(&self, _cx: &mut BuildCx) -> View {
            View::new(self.clone(), vec![], None)
        }
    }
    impl AnyView for FragileLeaf {
        fn key(&self) -> Option<&Key> {
            None
        }
        fn widget_type(&self) -> std::any::TypeId {
            std::any::TypeId::of::<Self>()
        }
        fn intrinsic_size(&self, constraints: BoxConstraints, _metrics: &TextMetrics) -> Size {
            self.measurements.set(self.measurements.get() + 1);
            if self.fail.get() {
                Size::new(f32::NAN, 10.0)
            } else {
                constraints.constrain(Size::new(20.0, 10.0))
            }
        }
    }
    fn snapshot(rt: &Runtime) -> Vec<(FiberId, Rect)> {
        let mut pending = vec![rt.root_id.unwrap()];
        let mut rects = vec![];
        while let Some(id) = pending.pop() {
            let fiber = rt.arena.get(id).unwrap();
            assert!(fiber.flags.is_empty(), "unfinished flags on {id:?}");
            rects.push((id, fiber.layout_rect.unwrap()));
            pending.extend_from_slice(&fiber.children);
        }
        rects
    }

    let fail = Rc::new(Cell::new(false));
    let measurements = Rc::new(Cell::new(0));
    let mut rt = Runtime::new();
    rt.set_root(Padding::all(2.0).unwrap().child(FragileLeaf {
        fail: fail.clone(),
        measurements: measurements.clone(),
    }));
    rt.update(now());
    let committed = snapshot(&rt);
    let child = committed[1].0;
    fail.set(true);
    rt.arena
        .get_mut(child)
        .unwrap()
        .flags
        .insert(DirtyFlags::LAYOUT_DIRTY);
    rt.arena
        .get_mut(child)
        .unwrap()
        .flags
        .insert(DirtyFlags::PAINT_DIRTY);
    rt.arena
        .get_mut(child)
        .unwrap()
        .flags
        .insert(DirtyFlags::HIT_TEST_DIRTY);
    rt.set_viewport(Viewport::new(100, 100, 1.0));
    assert!(rt.update(now()).request_redraw);
    assert_eq!(snapshot(&rt), committed);
    assert!(
        rt.arena
            .get(rt.root_id.unwrap())
            .unwrap()
            .layout_error()
            .is_some()
    );
    let failed_measurements = measurements.get();
    assert!(!rt.update(now()).request_redraw);
    rt.dispatch(UiEvent::Pointer(PointerEvent::new(
        Point::ZERO,
        PointerPhase::Move,
        PointerButton::Left,
        0,
    )));
    assert_eq!(measurements.get(), failed_measurements);

    fail.set(false);
    rt.set_text_metrics(TextMetrics {
        line_height: 21.0,
        ..DEFAULT_TEXT_METRICS
    });
    assert!(rt.update(now()).request_redraw);
    assert_eq!(snapshot(&rt), committed);
    assert!(
        rt.arena
            .get(rt.root_id.unwrap())
            .unwrap()
            .layout_error()
            .is_none()
    );
    assert_eq!(measurements.get(), failed_measurements + 1);
    assert!(!rt.update(now()).request_redraw);
}

#[test]
fn unencoded_repaint_preserves_scene_ids_and_initial_additions() {
    use crate::scene::primitive::Color;

    let mut rt = Runtime::new();
    rt.set_root(
        SizedBox::new(Size::new(100.0, 50.0))
            .unwrap()
            .background(Color::RED)
            .expect("valid color"),
    );
    let scene_ids: Vec<u64> = rt.scene_graph.items().iter().map(|item| item.id).collect();

    rt.run_paint_pass();

    assert_eq!(
        rt.scene_graph
            .items()
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        scene_ids
    );
    let delta = rt.pending_delta().unwrap();
    assert_eq!(
        delta.added.iter().map(|item| item.id).collect::<Vec<_>>(),
        scene_ids
    );
    assert!(delta.removed.is_empty());
    assert!(delta.modified.is_empty());
}

#[test]
fn unencoded_modified_item_remains_added_with_its_latest_contents() {
    use crate::effects::ExternalInvalidation;
    use crate::scene::primitive::Color;
    use crate::view::{BuildCx, Component, View};
    use std::cell::Cell;
    use std::rc::Rc;

    struct ColorToggle(Rc<Cell<bool>>);

    impl Component for ColorToggle {
        fn build(&self, cx: &mut BuildCx) -> View {
            SizedBox::new(Size::new(100.0, 50.0))
                .unwrap()
                .background(if self.0.get() {
                    Color::BLUE
                } else {
                    Color::RED
                })
                .expect("valid color")
                .build(cx)
        }
    }

    let blue = Rc::new(Cell::new(false));
    let mut rt = Runtime::new();
    rt.set_root(ColorToggle(Rc::clone(&blue)));
    let scene_id = rt.scene_graph.items()[0].id;

    blue.set(true);
    rt.invalidate_external(ExternalInvalidation::new());
    rt.update(now());

    let delta = rt.pending_delta().unwrap();
    assert_eq!(delta.added.len(), 1);
    assert_eq!(delta.added[0].id, scene_id);
    assert_eq!(delta.added[0], rt.scene_graph.items()[0]);
    assert!(delta.removed.is_empty());
    assert!(delta.modified.is_empty());
}

#[test]
fn keyed_children_middle_removal_preserves_retained_scene_item_identity() {
    use crate::construction::ComponentExt;
    use crate::scene::primitive::Color;
    use crate::signal::Signal;
    use crate::view::View;
    use crate::widgets::column::Column;

    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone)]
    struct TabContainer {
        signal: Rc<RefCell<Option<Signal<Vec<&'static str>>>>>,
    }

    impl Component for TabContainer {
        fn build(&self, cx: &mut BuildCx) -> View {
            let items_state = cx.use_state(|| vec!["a", "b", "c"]);
            *self.signal.borrow_mut() = Some(items_state.clone());
            let items = items_state.read();
            let mut col = Column::new();
            for key in items.iter() {
                col = col.push_child(
                    SizedBox::new(Size::new(50.0, 20.0))
                        .unwrap()
                        .background(Color::RED)
                        .expect("valid color")
                        .keyed(*key),
                );
            }
            col.build(cx)
        }
    }

    let signal_holder = Rc::new(RefCell::new(None));
    let mut rt = Runtime::new();
    rt.set_root(TabContainer {
        signal: signal_holder.clone(),
    });
    rt.update(now());

    let initial_items: Vec<_> = rt.scene_graph.items().to_vec();
    assert_eq!(initial_items.len(), 3);
    let id_a = initial_items[0].id;
    let id_b = initial_items[1].id;
    let id_c = initial_items[2].id;
    let initial_delta = rt.pending_delta.take().unwrap();
    assert_eq!(initial_delta.added.len(), 3);

    // Remove child B by updating parent's items signal: ["a", "c"]
    signal_holder.borrow().as_ref().unwrap().set(vec!["a", "c"]);
    rt.update(now());

    let remaining_items = rt.scene_graph.items();
    assert_eq!(remaining_items.len(), 2);
    assert_eq!(remaining_items[0].id, id_a);
    assert_eq!(
        remaining_items[1].id, id_c,
        "scene item ID of surviving child C must be preserved"
    );

    let delta = rt
        .pending_delta
        .take()
        .expect("expected delta after child removal");
    assert!(
        delta.removed.contains(&id_b),
        "unmounted child B's scene item must be in removed delta"
    );
    assert!(
        !delta.added.iter().any(|item| item.id == id_c),
        "surviving child C must not be re-allocated as an added item"
    );
    assert_eq!(
        delta.modified.len(),
        1,
        "surviving child C repositioned within column"
    );
    assert_eq!(delta.modified[0].id, id_c);
}

#[test]
fn keyed_children_reorder_preserves_scene_item_identities() {
    use crate::construction::ComponentExt;
    use crate::scene::primitive::Color;
    use crate::signal::Signal;
    use crate::view::View;
    use crate::widgets::column::Column;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone)]
    struct ReorderContainer {
        order: Rc<RefCell<Option<Signal<Vec<&'static str>>>>>,
    }

    impl Component for ReorderContainer {
        fn build(&self, cx: &mut BuildCx) -> View {
            let state = cx.use_state(|| vec!["a", "b", "c"]);
            *self.order.borrow_mut() = Some(state.clone());
            let items = state.read();
            let mut col = Column::new();
            for key in items.iter() {
                col = col.push_child(
                    SizedBox::new(Size::new(50.0, 20.0))
                        .unwrap()
                        .background(Color::RED)
                        .expect("valid color")
                        .keyed(*key),
                );
            }
            col.build(cx)
        }
    }

    let order_holder = Rc::new(RefCell::new(None));
    let mut rt = Runtime::new();
    rt.set_root(ReorderContainer {
        order: order_holder.clone(),
    });
    rt.update(now());

    let initial_items: Vec<_> = rt.scene_graph.items().to_vec();
    assert_eq!(initial_items.len(), 3);
    let (id_a, id_b, id_c) = (
        initial_items[0].id,
        initial_items[1].id,
        initial_items[2].id,
    );
    let _ = rt.pending_delta.take();

    // Reorder to ["c", "a", "b"]
    order_holder
        .borrow()
        .as_ref()
        .unwrap()
        .set(vec!["c", "a", "b"]);
    rt.update(now());

    let new_items = rt.scene_graph.items();
    assert_eq!(new_items.len(), 3);
    assert_eq!(new_items[0].id, id_c);
    assert_eq!(new_items[1].id, id_a);
    assert_eq!(new_items[2].id, id_b);

    let delta = rt
        .pending_delta
        .take()
        .expect("expected delta after reorder");
    assert!(
        delta.added.is_empty(),
        "reordered items must not be re-added"
    );
    assert!(
        delta.removed.is_empty(),
        "reordered items must not be removed"
    );
}

#[test]
fn nested_deferred_component_preserves_state_and_subscribes_its_fiber() {
    use crate::signal::Signal;
    use crate::view::{BuildCx, Component, View};
    use crate::widgets::column::Column;
    use crate::widgets::padding::Padding;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone)]
    struct StatefulChild {
        signal: Rc<RefCell<Option<Signal<u32>>>>,
        observed_values: Rc<RefCell<Vec<u32>>>,
    }

    impl Component for StatefulChild {
        fn build(&self, cx: &mut BuildCx) -> View {
            let state = cx.use_state(|| 0u32);
            self.observed_values.borrow_mut().push(*state.read());
            *self.signal.borrow_mut() = Some(state);
            SizedBox::new(Size::new(10.0, 10.0)).unwrap().build(cx)
        }
    }

    let signal = Rc::new(RefCell::new(None));
    let observed_values = Rc::new(RefCell::new(Vec::new()));
    let child = StatefulChild {
        signal: Rc::clone(&signal),
        observed_values: Rc::clone(&observed_values),
    };
    let mut rt = Runtime::new();
    rt.set_root(
        Column::new().push_child(
            Padding::new(crate::Insets {
                top: 1.0,
                right: 1.0,
                bottom: 1.0,
                left: 1.0,
            })
            .unwrap()
            .child(child),
        ),
    );

    let root_id = rt.root_id().unwrap();
    let padding_id = rt.arena().get(root_id).unwrap().children[0];
    let child_id = rt.arena().get(padding_id).unwrap().children[0];
    assert_eq!(rt.arena().get(child_id).unwrap().hooks.len(), 1);
    assert_eq!(observed_values.borrow().as_slice(), &[0]);

    let state = signal.borrow().as_ref().unwrap().clone();
    state.set(7);
    let effects = rt.update(now());

    assert!(
        effects.request_redraw,
        "the nested Fiber subscribed to its state"
    );
    assert_eq!(observed_values.borrow().last(), Some(&7));
    let rebuilt_padding = rt.arena().get(root_id).unwrap().children[0];
    assert_eq!(
        rt.arena().get(rebuilt_padding).unwrap().children[0],
        child_id
    );
    assert_eq!(rt.arena().get(child_id).unwrap().hooks.len(), 1);
}

// ── is_descendant_of ───────────────────────────────────────────────

#[test]
fn is_descendant_of_direct_child() {
    use crate::widgets::padding::Padding;
    let mut rt = Runtime::new();
    rt.set_root(
        Padding::new(crate::Insets {
            top: 8.0,
            right: 8.0,
            bottom: 8.0,
            left: 8.0,
        })
        .unwrap()
        .child(SizedBox::new(Size::new(100.0, 50.0)).unwrap()),
    );
    rt.update(now());

    let root_id = rt.root_id().unwrap();
    let padding_fiber = rt.arena().get(root_id).unwrap();
    let child_id = padding_fiber.children[0];

    assert!(rt.is_descendant_of(Some(child_id), root_id));
}

#[test]
fn is_descendant_of_self_is_true() {
    let mut rt = Runtime::new();
    rt.set_root(SizedBox::new(Size::new(100.0, 50.0)).unwrap());
    rt.update(now());

    let root_id = rt.root_id().unwrap();
    assert!(rt.is_descendant_of(Some(root_id), root_id));
}

#[test]
fn is_descendant_of_none_is_false() {
    let mut rt = Runtime::new();
    rt.set_root(SizedBox::new(Size::new(100.0, 50.0)).unwrap());
    rt.update(now());

    let root_id = rt.root_id().unwrap();
    assert!(!rt.is_descendant_of(None, root_id));
}

// ── build_ancestor_path ────────────────────────────────────────────

#[test]
fn build_ancestor_path_none() {
    let rt = Runtime::new();
    let path = rt.build_ancestor_path(None);
    assert!(path.is_empty());
}

#[test]
fn build_ancestor_path_root_is_singleton() {
    let mut rt = Runtime::new();
    rt.set_root(SizedBox::new(Size::new(100.0, 50.0)).unwrap());
    rt.update(now());

    let root_id = rt.root_id().unwrap();
    let path = rt.build_ancestor_path(Some(root_id));
    assert_eq!(path, vec![root_id]);
}

// ── tree_has_modal ─────────────────────────────────────────────────

#[test]
fn tree_has_modal_returns_false_for_dead_fiber() {
    let rt = Runtime::new();
    let mut arena = FiberArena::new();
    let fid = arena.insert(Fiber::new(None, std::any::TypeId::of::<()>(), None));
    arena.remove(fid);
    // Stale key — tree_has_modal should return false
    assert!(!rt.tree_has_modal(fid));
}

#[test]
fn tree_has_modal_detects_modal_at_root() {
    let mut rt = Runtime::new();
    rt.set_root(
        FocusScope::new()
            .modal(true)
            .child(SizedBox::new(Size::new(100.0, 50.0)).unwrap()),
    );
    rt.update(now());

    let root_id = rt.root_id().unwrap();
    assert!(rt.tree_has_modal(root_id));
}

#[test]
fn tree_has_modal_detects_modal_in_deep_subtree() {
    use crate::widgets::column::Column;
    use crate::widgets::padding::Padding;

    let mut rt = Runtime::new();
    rt.set_root(
        Padding::new(crate::Insets {
            top: 8.0,
            right: 8.0,
            bottom: 8.0,
            left: 8.0,
        })
        .unwrap()
        .child(
            Column::new().push_child(
                FocusScope::new()
                    .modal(true)
                    .child(SizedBox::new(Size::new(100.0, 50.0)).unwrap()),
            ),
        ),
    );
    rt.update(now());

    let root_id = rt.root_id().unwrap();
    // tree_has_modal recursively scans the entire subtree of the given fiber.
    // The root (Padding) contains a Column containing a modal, so scanning
    // the root returns true.
    assert!(rt.tree_has_modal(root_id));

    // A separate non-modal SVG or SizedBox root returns false
    let mut rt2 = Runtime::new();
    rt2.set_root(SizedBox::new(Size::new(100.0, 50.0)).unwrap());
    rt2.update(now());
    let root2_id = rt2.root_id().unwrap();
    assert!(!rt2.tree_has_modal(root2_id));
}

#[test]
fn tree_has_modal_returns_false_for_non_modal_tree() {
    let mut rt = Runtime::new();
    rt.set_root(SizedBox::new(Size::new(100.0, 50.0)).unwrap());
    rt.update(now());

    let root_id = rt.root_id().unwrap();
    assert!(!rt.tree_has_modal(root_id));
}

// ── finish_event ───────────────────────────────────────────────────

#[test]
fn finish_event_returns_false_when_no_commands_no_paint() {
    let mut rt = Runtime::new();
    let ctx = EventCtx::new();
    assert!(!rt.finish_event(ctx));
}

#[test]
fn finish_event_returns_true_when_invalidate_paint_called() {
    let mut rt = Runtime::new();
    let mut ctx = EventCtx::new();
    ctx.invalidate_paint();
    assert!(rt.finish_event(ctx));
}

// ── route_to_single ────────────────────────────────────────────────

#[test]
fn route_to_single_with_live_fiber() {
    let mut rt = Runtime::new();
    rt.set_root(Button::text("OK"));
    rt.update(now());

    let root_id = rt.root_id().unwrap();
    let event = UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Enter,
        modifiers: Modifiers::default(),
    });
    let needs_redraw = rt.route_to_single(root_id, &event);
    // Button handles Enter, invalidates paint internally — but route_to_single
    // calls finish_event which may also return true from ctx.needs_paint()
    // Only check it doesn't panic
    let _ = needs_redraw;
}

// ── Accessors ──────────────────────────────────────────────────────

#[test]
fn runtime_without_external_draws_reports_none() {
    let rt = Runtime::new();

    assert!(!rt.has_external_draws());
}

#[test]
fn runtime_with_external_surface_root_reports_external_draws() {
    let handler: Arc<ExternalDrawFn<'static>> = Arc::new(|_, _, _, _, _| {});
    let mut rt = Runtime::new();
    rt.set_root(ExternalSurface::new(77).on_draw(handler));
    rt.update(now());

    assert!(rt.has_external_draws());
}

#[test]
fn should_report_no_external_draws_when_external_surface_has_no_handler() {
    let mut rt = Runtime::new();
    rt.set_root(ExternalSurface::new(77));
    rt.update(now());

    assert!(!rt.has_external_draws());
}

#[test]
fn should_report_no_external_draws_when_external_surface_registers_only_a_schedule() {
    let schedule: Arc<ExternalScheduleFn> = Arc::new(|_, _| ExternalScheduleDemand::empty());
    let mut rt = Runtime::new();
    rt.set_root(ExternalSurface::new(78).schedule_with(schedule));
    rt.update(now());

    assert!(!rt.has_external_draws());
}

#[test]
fn should_report_external_draws_when_handler_is_nested_in_subtree() {
    let handler: Arc<ExternalDrawFn<'static>> = Arc::new(|_, _, _, _, _| {});
    let mut rt = Runtime::new();
    rt.set_root(Column::new().push_child(ExternalSurface::new(79).on_draw(handler)));
    rt.update(now());

    assert!(rt.has_external_draws());
}

#[test]
fn should_drop_external_draws_when_root_is_replaced_with_plain_widget() {
    let handler: Arc<ExternalDrawFn<'static>> = Arc::new(|_, _, _, _, _| {});
    let mut rt = Runtime::new();
    rt.set_root(ExternalSurface::new(80).on_draw(handler));
    rt.update(now());
    assert!(rt.has_external_draws());

    // Act — replacing the root rebuilds and reinstalls the external bag.
    rt.set_root(Button::text("OK"));
    rt.update(now());

    // Assert
    assert!(!rt.has_external_draws());
}

#[test]
fn focused_external_ime_provider_tracks_layout_and_disables_after_focus_leaves() {
    let provider: Arc<ExternalImeFn> = Arc::new(|_, context| {
        Some(ImeEffect {
            allowed: Some(true),
            position: Some(context.logical_rect.max),
        })
    });
    let mut rt = Runtime::new();
    rt.set_viewport(Viewport::new(100, 40, 1.0));
    rt.set_root(ExternalSurface::new(81).ime_with(provider));

    let initial = rt.update(now());
    assert!(initial.ime.is_none());

    assert!(rt.focus_first_focusable());
    let focused = rt.update(now());
    assert_eq!(
        focused.ime,
        Some(ImeEffect {
            allowed: Some(true),
            position: Some(Point::new(100.0, 40.0)),
        })
    );

    rt.set_viewport(Viewport::new(160, 60, 1.0));
    let resized = rt.update(now());
    assert_eq!(
        resized.ime.and_then(|effect| effect.position),
        Some(Point::new(160.0, 60.0))
    );

    rt.clear_focus();
    let unfocused = rt.update(now());
    assert_eq!(unfocused.ime, Some(ImeEffect::set_allowed(false)));
}

#[test]
fn external_schedule_refreshes_state_before_focused_ime_is_collected() {
    let state = Arc::new(AtomicUsize::new(0));
    let schedule_state = Arc::clone(&state);
    let schedule: Arc<ExternalScheduleFn> = Arc::new(move |_, _| {
        schedule_state.store(7, Ordering::SeqCst);
        ExternalScheduleDemand::empty()
    });
    let ime_state = Arc::clone(&state);
    let ime: Arc<ExternalImeFn> = Arc::new(move |_, _| {
        Some(ImeEffect {
            allowed: Some(true),
            position: Some(Point::new(ime_state.load(Ordering::SeqCst) as f32, 0.0)),
        })
    });
    let mut rt = Runtime::new();
    rt.set_viewport(Viewport::new(100, 40, 1.0));
    rt.set_root(
        ExternalSurface::new(82)
            .schedule_with(schedule)
            .ime_with(ime),
    );
    rt.update(now());
    assert!(rt.focus_first_focusable());
    state.store(0, Ordering::SeqCst);

    let effects = rt.update(now());

    assert_eq!(
        effects.ime.and_then(|effect| effect.position),
        Some(Point::new(7.0, 0.0))
    );
}
#[test]
fn new_runtime_has_no_root() {
    let rt = Runtime::new();
    assert!(rt.root_id().is_none());
    assert!(rt.pending_delta().is_none());
    assert!(rt.input().focused.is_none());
    assert!(rt.current_viewport().is_none());
}

#[test]
fn should_store_viewport_and_report_change_when_set_viewport_differs() {
    // Arrange
    let mut rt = Runtime::new();
    let viewport = Viewport::new(800, 600, 1.0);

    // Act
    let changed = rt.set_viewport(viewport.clone());
    let unchanged = rt.set_viewport(viewport.clone());

    // Assert
    assert!(changed);
    assert!(!unchanged);
    assert_eq!(rt.current_viewport(), Some(&viewport));
}

#[test]
fn should_store_zero_sized_viewport_without_rejecting_it() {
    // Arrange
    let mut rt = Runtime::new();
    let viewport = Viewport::new(0, 0, 1.0);

    // Act
    assert!(rt.set_viewport(viewport.clone()));

    // Assert
    assert_eq!(rt.current_viewport(), Some(&viewport));
    assert!(!rt.current_viewport().unwrap().is_drawable());
}

#[test]
fn should_report_change_when_scale_changes_with_constant_physical_size() {
    // Arrange
    let mut rt = Runtime::new();
    assert!(rt.set_viewport(Viewport::new(800, 600, 1.0)));

    // Act
    let changed = rt.set_viewport(Viewport::new(800, 600, 2.0));

    // Assert
    assert!(changed);
    assert_eq!(rt.current_viewport().map(|vp| vp.scale_factor), Some(2.0));
    assert_eq!(
        rt.current_viewport().map(|vp| vp.logical_size),
        Some(crate::layout::Size::new(400.0, 300.0))
    );
}

#[test]
fn should_dirty_layout_when_scale_only_viewport_changes() {
    // Arrange
    use crate::widgets::sized_box::SizedBox;

    let mut rt = Runtime::new();
    rt.set_root(SizedBox::new(crate::layout::Size::new(100.0, 50.0)).unwrap());
    rt.set_viewport(Viewport::new(800, 600, 1.0));
    assert!(rt.update(now()).request_redraw);
    assert!(!rt.update(now()).request_redraw);

    // Act
    assert!(rt.set_viewport(Viewport::new(800, 600, 2.0)));
    let effects = rt.update(now());

    // Assert
    assert!(effects.request_redraw);
}

#[test]
fn focus_transition_notifies_old_and_new_fibers_and_requests_redraw() {
    let mut rt = Runtime::new();
    rt.set_root(
        FocusScope::new().child(
            crate::widgets::stack::Stack::new()
                .push_child(Focus::new().child(ExternalSurface::new(1)))
                .push_child(Focus::new().child(ExternalSurface::new(2))),
        ),
    );
    rt.update(now());
    assert!(rt.focus_first_focusable());
    rt.drain_external_input();

    let first = rt.input().focused;
    let effects = rt.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Tab,
        modifiers: Modifiers::default(),
    }));

    assert!(effects.request_redraw);
    assert_ne!(rt.input().focused, first);
    let focus_events = rt.drain_external_input();
    assert!(
        focus_events.iter().any(|(_, event)| matches!(
            event,
            UiEvent::Focus(crate::input::event::FocusEvent::Lost)
        ))
    );
    assert!(focus_events.iter().any(|(_, event)| matches!(
        event,
        UiEvent::Focus(crate::input::event::FocusEvent::GainedVisible)
    )));
}

#[test]
fn set_focus_and_clear_focus() {
    let mut rt = Runtime::new();
    rt.set_root(Button::text("OK"));
    rt.update(now());

    let root_id = rt.root_id().unwrap();
    assert!(rt.input().focused.is_none());

    rt.set_focus(root_id);
    assert_eq!(rt.input().focused, Some(root_id));

    rt.clear_focus();
    assert!(rt.input().focused.is_none());
}

// ── route_event keyboard with no focused widget ────────────────────

#[test]
fn route_event_keyboard_no_focused_returns_false() {
    let mut rt = Runtime::new();
    rt.set_root(SizedBox::new(Size::new(100.0, 50.0)).unwrap());
    rt.update(now());

    let event = UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Enter,
        modifiers: Modifiers::default(),
    });
    // No focused widget; keyboard path = [root_id] only
    let needs_redraw = rt.route_event(&event);
    assert!(!needs_redraw);
}

// ── route_event pointer cancel without capture ─────────────────────

#[test]
fn route_event_pointer_cancel_without_capture_returns_false() {
    let mut rt = Runtime::new();
    rt.set_root(Button::text("OK"));
    rt.update(now());

    let event = UiEvent::Pointer(PointerEvent::new(
        Point::new(50.0, 16.0),
        PointerPhase::Cancel,
        PointerButton::Left,
        42,
    ));
    // No capture for pointer 42 — should return false without crash
    let needs_redraw = rt.route_event(&event);
    assert!(!needs_redraw);
}

// ── route_event dead captor ────────────────────────────────────────

#[test]
fn route_event_dead_captor_falls_through_to_hit_test() {
    // Test: when a pointer is captured but the captor fiber is dead,
    // the runtime should not panic. We verify via public API by
    // capturing a pointer on one tree, replacing the tree, then
    // sending events for the captured pointer.
    //
    // After set_root replaces the tree, the old captor fiber is
    // unmounted and `clear_capture_if_dead` in rebuild_root removes
    // the capture. Sending events for the stale pointer is a no-op.
    let clicked = Arc::new(AtomicBool::new(false));
    let mut rt = Runtime::new();
    rt.set_root(Button::text("OK").on_click(move |_ctx| {
        clicked.store(true, Ordering::SeqCst);
    }));
    rt.update(now());

    // Down to capture pointer 99
    rt.dispatch(UiEvent::Pointer(PointerEvent::new(
        Point::new(46.0, 16.0),
        PointerPhase::Down,
        PointerButton::Left,
        99,
    )));
    assert!(
        rt.input().captor(99).is_some(),
        "pointer 99 should be captured"
    );

    // Replace tree — old fibers are unmounted, captures cleared
    rt.set_root(Button::text("Replacement"));
    rt.update(now());

    // After tree replacement, capture should be cleared
    assert!(
        rt.input().captor(99).is_none(),
        "capture should be cleared when tree is replaced"
    );

    // Sending events for now-released pointer should not panic
    let req = rt.dispatch(UiEvent::Pointer(PointerEvent::new(
        Point::new(46.0, 16.0),
        PointerPhase::Up,
        PointerButton::Left,
        99,
    )));
    let _ = req;
}

// ── layout_fiber edge cases ────────────────────────────────────────

#[test]
fn layout_fiber_with_stale_fiber_id_does_not_panic() {
    let mut arena = FiberArena::new();
    let id = arena.insert(Fiber::new(None, std::any::TypeId::of::<()>(), None));
    arena.remove(id);
    // Calling layout_fiber with a stale id should be a no-op
    layout_fiber(
        &mut arena,
        id,
        BoxConstraints::loose(Size::new(800.0, 600.0)),
        Point::ZERO,
        &DEFAULT_TEXT_METRICS,
    );
    // No panic = pass
}

// ── has_modal with no root ─────────────────────────────────────────

#[test]
fn has_modal_returns_false_with_no_root() {
    let rt = Runtime::new();
    assert!(!rt.has_modal());
}
// ── focus_first_focusable ──────────────────────────────────────────

#[test]
fn should_focus_button_when_it_is_focusable() {
    // Arrange
    let mut rt = Runtime::new();
    rt.set_root(Button::text("OK"));
    rt.update(now());

    // Act
    let found = rt.focus_first_focusable();

    // Assert
    assert!(
        found,
        "focus_first_focusable should return true for a Button"
    );
    assert!(rt.input().focused.is_some(), "focused widget should be set");
}

#[test]
fn should_skip_non_focusable_sized_box_and_focus_button() {
    use crate::widgets::column::Column;
    // Arrange
    let mut rt = Runtime::new();
    rt.set_root(
        Column::new()
            .push_child(SizedBox::new(Size::new(100.0, 50.0)).unwrap())
            .push_child(Button::text("Next")),
    );
    rt.update(now());

    // Act
    let found = rt.focus_first_focusable();

    // Assert
    assert!(
        found,
        "should find the Button even though SizedBox comes first"
    );
    let focused_id = rt.input().focused.expect("focused widget should be set");
    let fiber = rt.arena().get(focused_id).unwrap();
    assert!(
        fiber.is_focusable(),
        "focused widget should be focusable (the Button, not the SizedBox)"
    );
}

#[test]
fn should_return_false_when_no_root_set() {
    // Arrange
    let mut rt = Runtime::new();

    // Act
    let found = rt.focus_first_focusable();

    // Assert
    assert!(!found, "should return false when no root widget is set");
    assert!(rt.input().focused.is_none(), "focused should remain None");
}

#[test]
fn should_return_false_when_no_focusable_widgets_exist() {
    // Arrange
    let mut rt = Runtime::new();
    rt.set_root(SizedBox::new(Size::new(100.0, 50.0)).unwrap());
    rt.update(now());

    // Act
    let found = rt.focus_first_focusable();

    // Assert
    assert!(
        !found,
        "should return false when tree has no focusable widgets"
    );
    assert!(rt.input().focused.is_none(), "focused should remain None");
}

// ── text run preparation ───────────────────────────────────────────

#[test]
fn text_preparation_reuses_unchanged_scene_runs_and_releases_removed_runs() {
    let mut rt = Runtime::new();
    rt.set_root(TextLabel::new("Confirm paste?"));
    rt.prepare_text_runs(&|_ch| None);
    assert_eq!(rt.text_run_cache().len(), 1);

    // Preparing an unchanged retained scene must not allocate another run.
    rt.prepare_text_runs(&|_ch| None);
    assert_eq!(rt.text_run_cache().len(), 1);

    rt.set_root(SizedBox::new(Size::new(1.0, 1.0)).unwrap());
    rt.prepare_text_runs(&|_ch| None);
    assert!(rt.text_run_cache().is_empty());
}

#[test]
fn button_text_rebuilds_and_marks_instances_dirty_when_atlas_revision_changes() {
    let mut rt = Runtime::new();
    rt.set_root(Button::text("OK"));
    let glyph = |_ch| {
        Some(crate::text::AtlasGlyph {
            key: harbor_text::GlyphKey::new(
                harbor_text::FaceId::PRIMARY,
                harbor_text::GlyphId::new(1),
                harbor_text::FontSize::new(12.0).unwrap(),
                harbor_text::FontStyle::REGULAR,
            ),
            uv: crate::text::AtlasUv {
                left: 0.0,
                top: 0.0,
                right: 0.5,
                bottom: 0.5,
            },
            width: 8,
            height: 16,
            bearing_x: 0,
            bearing_y: 12,
            advance_width: 8.0,
            atlas_x: 0,
            atlas_y: 0,
        })
    };
    let text_id = rt
        .scene_graph
        .items()
        .iter()
        .find(|item| {
            matches!(
                item.primitive,
                crate::scene::primitive::Primitive::Text { .. }
            )
        })
        .expect("button label text primitive")
        .id;

    rt.encoder
        .prepare_text_runs(&rt.scene_graph, &rt.text_metrics, 1, &glyph);
    assert_eq!(
        rt.encoder
            .text_run_cache()
            .get(text_id)
            .unwrap()
            .glyphs
            .len(),
        2
    );
    assert!(rt.encoder.text_instances_dirty());

    rt.encoder.mark_text_instances_uploaded();
    rt.encoder
        .prepare_text_runs(&rt.scene_graph, &rt.text_metrics, 1, &glyph);
    assert!(!rt.encoder.text_instances_dirty());

    rt.encoder
        .prepare_text_runs_at_scale(&rt.scene_graph, &rt.text_metrics, 1, 2.0, &glyph);
    assert!(rt.encoder.text_instances_dirty());
    rt.encoder.mark_text_instances_uploaded();

    rt.encoder
        .prepare_text_runs(&rt.scene_graph, &rt.text_metrics, 2, &glyph);
    assert!(rt.encoder.text_instances_dirty());
    assert_eq!(
        rt.encoder
            .text_run_cache()
            .get(text_id)
            .unwrap()
            .glyphs
            .len(),
        2
    );
}

#[test]
fn text_metrics_are_isolated_per_runtime_and_relayout_the_owner() {
    let mut narrow = DEFAULT_TEXT_METRICS;
    narrow.cell_width = 4.0;
    let mut wide = DEFAULT_TEXT_METRICS;
    wide.cell_width = 20.0;

    let mut first = Runtime::with_text_metrics(narrow);
    let mut second = Runtime::with_text_metrics(wide);
    first.set_root(TextLabel::new("text"));
    second.set_root(TextLabel::new("text"));

    assert_eq!(
        first
            .arena()
            .get(first.root_id().unwrap())
            .unwrap()
            .layout_rect()
            .unwrap()
            .size()
            .width,
        16.0
    );
    assert_eq!(
        second
            .arena()
            .get(second.root_id().unwrap())
            .unwrap()
            .layout_rect()
            .unwrap()
            .size()
            .width,
        80.0
    );

    first.set_text_metrics(wide);
    assert!(first.update(now()).request_redraw);
    assert_eq!(
        first
            .arena()
            .get(first.root_id().unwrap())
            .unwrap()
            .layout_rect()
            .unwrap()
            .size()
            .width,
        80.0
    );
    assert_eq!(second.text_metrics().cell_width, 20.0);
}

// ── external schedule merge ─────────────────────────────────────────

#[test]
fn should_emit_wait_until_on_idle_when_schedule_reports_deadline() {
    // Arrange
    let deadline = Instant::now() + Duration::from_secs(5);
    let schedule: Arc<ExternalScheduleFn> = Arc::new(move |_, _| ExternalScheduleDemand {
        redraw_now: false,
        deadline: Some(deadline),
        ..ExternalScheduleDemand::empty()
    });
    let mut rt = Runtime::new();
    rt.set_root(ExternalSurface::new(1).schedule_with(schedule));
    let _ = rt.update(now());

    // Act
    let idle = rt.update(now());

    // Assert
    assert!(!idle.request_redraw);
    assert_eq!(
        idle.control_flow,
        Some(ControlFlowEffect::WaitUntil(deadline))
    );
}

#[test]
fn should_select_earliest_deadline_when_multiple_schedules_report() {
    // Arrange
    let early = Instant::now() + Duration::from_millis(100);
    let late = Instant::now() + Duration::from_secs(2);
    let late_schedule: Arc<ExternalScheduleFn> = Arc::new(move |_, _| ExternalScheduleDemand {
        redraw_now: false,
        deadline: Some(late),
        ..ExternalScheduleDemand::empty()
    });
    let early_schedule: Arc<ExternalScheduleFn> = Arc::new(move |_, _| ExternalScheduleDemand {
        redraw_now: false,
        deadline: Some(early),
        ..ExternalScheduleDemand::empty()
    });
    let mut rt = Runtime::new();
    rt.set_root(
        Column::new()
            .push_child(ExternalSurface::new(1).schedule_with(late_schedule))
            .push_child(ExternalSurface::new(2).schedule_with(early_schedule)),
    );
    let _ = rt.update(now());

    // Act
    let idle = rt.update(now());

    // Assert
    assert_eq!(idle.control_flow, Some(ControlFlowEffect::WaitUntil(early)));
}

#[test]
fn should_request_redraw_when_schedule_reports_redraw_now() {
    // Arrange
    let schedule: Arc<ExternalScheduleFn> = Arc::new(|_, _| ExternalScheduleDemand {
        redraw_now: true,
        deadline: None,
        ..ExternalScheduleDemand::empty()
    });
    let mut rt = Runtime::new();
    rt.set_root(ExternalSurface::new(4).schedule_with(schedule));
    let _ = rt.update(now());

    // Act
    let idle = rt.update(now());

    // Assert
    assert!(idle.request_redraw);
    assert!(idle.ordinary_present_eligible);
    assert!(!idle.force_present);
    assert!(idle.control_flow.is_none());
}

#[test]
fn should_merge_redraw_and_wait_until_when_schedule_reports_both() {
    // Arrange
    let deadline = Instant::now() + Duration::from_millis(250);
    let schedule: Arc<ExternalScheduleFn> = Arc::new(move |_, _| ExternalScheduleDemand {
        redraw_now: true,
        deadline: Some(deadline),
        ..ExternalScheduleDemand::empty()
    });
    let mut rt = Runtime::new();
    rt.set_root(ExternalSurface::new(5).schedule_with(schedule));
    let _ = rt.update(now());

    // Act
    let idle = rt.update(now());

    // Assert
    assert!(idle.request_redraw);
    assert_eq!(
        idle.control_flow,
        Some(ControlFlowEffect::WaitUntil(deadline))
    );
}

#[test]
fn should_not_emit_poll_when_external_schedule_is_empty() {
    // Arrange — empty demand must not invent blink-only Poll
    let schedule: Arc<ExternalScheduleFn> = Arc::new(|_, _| ExternalScheduleDemand::empty());
    let mut rt = Runtime::new();
    rt.set_root(ExternalSurface::new(6).schedule_with(schedule));
    let _ = rt.update(now());

    // Act
    let idle = rt.update(now());

    // Assert
    assert!(!idle.request_redraw);
    assert!(idle.ordinary_present_eligible);
    assert!(!idle.force_present);
    assert!(idle.control_flow.is_none());
    assert_ne!(idle.control_flow, Some(ControlFlowEffect::Poll));
}

#[test]
fn should_pass_registered_draw_id_when_collecting_external_schedule() {
    // Arrange
    let seen = Arc::new(AtomicU64::new(0));
    let seen_id = Arc::clone(&seen);
    let schedule: Arc<ExternalScheduleFn> = Arc::new(move |id, _| {
        seen_id.store(id.get(), Ordering::SeqCst);
        ExternalScheduleDemand::empty()
    });
    let mut rt = Runtime::new();
    rt.set_root(ExternalSurface::new(42).schedule_with(schedule));
    let _ = rt.update(now());

    // Act
    let _ = rt.update(now());

    // Assert
    assert_eq!(seen.load(Ordering::SeqCst), 42);
}

#[test]
fn should_clear_external_schedules_when_root_no_longer_registers_them() {
    // Arrange — provider reports a deadline, then root is replaced without one.
    let deadline = Instant::now() + Duration::from_secs(3);
    let schedule: Arc<ExternalScheduleFn> = Arc::new(move |_, _| ExternalScheduleDemand {
        redraw_now: false,
        deadline: Some(deadline),
        ..ExternalScheduleDemand::empty()
    });
    let mut rt = Runtime::new();
    rt.set_root(ExternalSurface::new(1).schedule_with(schedule));
    let _ = rt.update(now());
    assert_eq!(
        rt.update(now()).control_flow,
        Some(ControlFlowEffect::WaitUntil(deadline))
    );

    // Act — rebuild without a schedule provider
    rt.set_root(SizedBox::new(Size::new(20.0, 20.0)).unwrap());
    let _ = rt.update(now());
    let idle = rt.update(now());

    // Assert
    assert!(idle.control_flow.is_none());
    assert!(!idle.request_redraw);
    assert!(idle.ordinary_present_eligible);
    assert!(!idle.has_deferred_externals);
    assert!(!idle.force_present);
}

#[test]
fn should_not_request_redraw_when_schedule_defers_ordinary_present() {
    let schedule: Arc<ExternalScheduleFn> = Arc::new(|_, _| ExternalScheduleDemand {
        redraw_now: true,
        ordinary_present_eligible: false,
        ..ExternalScheduleDemand::empty()
    });
    let mut rt = Runtime::new();
    rt.set_root(ExternalSurface::new(7).schedule_with(schedule));
    let _ = rt.update(now());

    let idle = rt.update(now());

    assert!(!idle.request_redraw);
    assert!(idle.ordinary_present_eligible);
    assert!(idle.has_deferred_externals);
    assert!(!idle.force_present);
}

#[test]
fn should_keep_wait_until_when_schedule_defers_ordinary_present() {
    let deadline = Instant::now() + Duration::from_millis(400);
    let schedule: Arc<ExternalScheduleFn> = Arc::new(move |_, _| ExternalScheduleDemand {
        redraw_now: true,
        deadline: Some(deadline),
        ordinary_present_eligible: false,
    });
    let mut rt = Runtime::new();
    rt.set_root(ExternalSurface::new(8).schedule_with(schedule));
    let _ = rt.update(now());

    let idle = rt.update(now());

    assert!(!idle.request_redraw);
    assert!(idle.ordinary_present_eligible);
    assert!(idle.has_deferred_externals);
    assert_eq!(
        idle.control_flow,
        Some(ControlFlowEffect::WaitUntil(deadline))
    );
    assert_ne!(idle.control_flow, Some(ControlFlowEffect::Poll));
}

#[test]
fn should_keep_window_eligible_when_any_schedule_reports_ineligible() {
    let eligible: Arc<ExternalScheduleFn> = Arc::new(|_, _| ExternalScheduleDemand {
        redraw_now: true,
        ..ExternalScheduleDemand::empty()
    });
    let deferred: Arc<ExternalScheduleFn> = Arc::new(|_, _| ExternalScheduleDemand {
        ordinary_present_eligible: false,
        ..ExternalScheduleDemand::empty()
    });
    let mut rt = Runtime::new();
    rt.set_root(
        Column::new()
            .push_child(ExternalSurface::new(1).schedule_with(eligible))
            .push_child(ExternalSurface::new(2).schedule_with(deferred)),
    );
    let _ = rt.update(now());

    let idle = rt.update(now());

    assert!(idle.request_redraw);
    assert!(idle.ordinary_present_eligible);
    assert!(idle.has_deferred_externals);
    assert!(!idle.force_present);
}

#[test]
fn should_remain_eligible_when_no_external_schedule_is_registered() {
    // Arrange
    let mut rt = Runtime::new();
    rt.set_root(SizedBox::new(Size::new(20.0, 20.0)).unwrap());
    let _ = rt.update(now());

    // Act
    let idle = rt.update(now());

    // Assert
    assert!(idle.ordinary_present_eligible);
    assert!(!idle.has_deferred_externals);
    assert!(!idle.force_present);
    assert!(!idle.request_redraw);
}

#[test]
fn should_keep_dirty_redraw_deferred_when_schedule_is_ineligible() {
    // Arrange
    let schedule: Arc<ExternalScheduleFn> = Arc::new(|_, _| ExternalScheduleDemand {
        ordinary_present_eligible: false,
        ..ExternalScheduleDemand::empty()
    });
    let mut rt = Runtime::new();
    rt.set_root(ExternalSurface::new(9).schedule_with(schedule));

    // Act — first update rebuilds dirty Fibers and then folds schedule demand
    let first = rt.update(now());

    // Assert
    assert!(first.request_redraw);
    assert!(first.ordinary_present_eligible);
    assert!(first.has_deferred_externals);
    assert!(!first.force_present);
}

#[test]
fn should_ignore_deferred_redraw_when_eligible_sibling_reports_none() {
    // Arrange
    let eligible: Arc<ExternalScheduleFn> = Arc::new(|_, _| ExternalScheduleDemand::empty());
    let deferred: Arc<ExternalScheduleFn> = Arc::new(|_, _| ExternalScheduleDemand {
        redraw_now: true,
        ordinary_present_eligible: false,
        ..ExternalScheduleDemand::empty()
    });
    let mut rt = Runtime::new();
    rt.set_root(
        Column::new()
            .push_child(ExternalSurface::new(10).schedule_with(eligible))
            .push_child(ExternalSurface::new(11).schedule_with(deferred)),
    );
    let _ = rt.update(now());

    // Act
    let idle = rt.update(now());

    // Assert
    assert!(!idle.request_redraw);
    assert!(idle.ordinary_present_eligible);
    assert!(idle.has_deferred_externals);
    assert!(!idle.force_present);
}

#[test]
fn should_clear_deferred_externals_when_ineligible_schedule_is_removed() {
    // Arrange
    let schedule: Arc<ExternalScheduleFn> = Arc::new(|_, _| ExternalScheduleDemand {
        ordinary_present_eligible: false,
        ..ExternalScheduleDemand::empty()
    });
    let mut rt = Runtime::new();
    rt.set_root(ExternalSurface::new(12).schedule_with(schedule));
    let _ = rt.update(now());
    assert!(rt.update(now()).has_deferred_externals);

    // Act
    rt.set_root(SizedBox::new(Size::new(20.0, 20.0)).unwrap());
    let _ = rt.update(now());
    let idle = rt.update(now());

    // Assert
    assert!(!idle.has_deferred_externals);
    assert!(idle.ordinary_present_eligible);
    assert!(!idle.request_redraw);
}
