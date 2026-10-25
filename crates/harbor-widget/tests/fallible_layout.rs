use harbor_widget::fiber::FiberId;
use harbor_widget::layout::{Point, Size};
use harbor_widget::renderer::Viewport;
use harbor_widget::runtime::Runtime;
use harbor_widget::signal::Signal;
use harbor_widget::view::{BuildCx, Component, Key, View};
use harbor_widget::widgets::text_label::TextLabel;
use harbor_widget::{
    Align, Alignment2D, Axis, Button, Children, Column, ComponentExt, Flex, FlexOptions, Insets,
    LayoutConfigError, Padding, Row, SizedBox, WithChildren, view,
};
use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::time::Instant;

fn update(runtime: &mut Runtime) {
    runtime.update(Instant::now());
}
fn child(runtime: &Runtime, id: FiberId, index: usize) -> FiberId {
    runtime.arena().get(id).unwrap().children()[index]
}
fn panic_message(f: impl FnOnce()) -> String {
    let panic = catch_unwind(AssertUnwindSafe(f)).expect_err("expected construction panic");
    panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap()
}

#[test]
fn scalar_matrix_is_recoverable_and_equivalent_across_entry_points() {
    fn error<T>(value: Result<T, LayoutConfigError>, field: &str, scalar: f32) {
        let err = value
            .err()
            .expect("invalid scalar must fail at construction");
        assert_eq!(err.field, field);
        assert_eq!(err.value.to_bits(), scalar.to_bits());
        assert!(err.to_string().contains("finite and nonnegative"));
    }
    for scalar in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.01] {
        error(SizedBox::width(scalar), "width", scalar);
        error(SizedBox::height(scalar), "height", scalar);
        error(SizedBox::new(Size::new(scalar, 1.0)), "width", scalar);
        error(SizedBox::new(Size::new(1.0, scalar)), "height", scalar);
        error(Padding::all(scalar), "top", scalar);
        for (field, insets) in [
            (
                "top",
                Insets {
                    top: scalar,
                    ..Default::default()
                },
            ),
            (
                "right",
                Insets {
                    right: scalar,
                    ..Default::default()
                },
            ),
            (
                "bottom",
                Insets {
                    bottom: scalar,
                    ..Default::default()
                },
            ),
            (
                "left",
                Insets {
                    left: scalar,
                    ..Default::default()
                },
            ),
        ] {
            error(Padding::new(insets), field, scalar);
            error(
                Align::new(Alignment2D::CENTER).insets(insets),
                field,
                scalar,
            );
        }
        let options = FlexOptions {
            gap: scalar,
            ..Default::default()
        };
        error(Row::with_options(options), "gap", scalar);
        error(Column::with_options(options), "gap", scalar);
        error(Flex::with_options(Axis::Horizontal, options), "gap", scalar);
        error(Row::new().gap(scalar), "gap", scalar);
        error(Column::new().gap(scalar), "gap", scalar);
        error(Flex::new(Axis::Vertical).gap(scalar), "gap", scalar);
    }
    for scalar in [0.0, -0.0, 5.0, f32::MAX] {
        assert!(SizedBox::new(Size::new(scalar, scalar)).is_ok());
        assert!(Padding::all(scalar).is_ok());
        assert!(
            Align::new(Alignment2D::CENTER)
                .insets(Insets::all(scalar))
                .is_ok()
        );
        assert!(
            Row::with_options(FlexOptions {
                gap: scalar,
                ..Default::default()
            })
            .is_ok()
        );
    }
    assert_eq!(Row::new().options(), FlexOptions::default());
    assert_eq!(Column::new().options(), FlexOptions::default());
    assert_eq!(
        Flex::new(Axis::Horizontal).options(),
        FlexOptions::default()
    );
}

// Keep explicit source-line assertions stable under formatting.
#[rustfmt::skip]
#[test]
fn invalid_final_nodes_report_the_origin_and_preserve_children_before_parent() {
    let line = line!() + 1;
    let message = panic_message(|| { let _ = view! { SizedBox::width(-1.0); }; });
    assert!(message.contains(&format!("{}:{line}:", file!())), "{message}");
    assert!(message.contains("width must be finite and nonnegative; got -1"), "{message}");
    let trace = RefCell::new(Vec::new());
    let line = line!() + 2;
    let message = panic_message(|| { let _ = view! {
        { trace.borrow_mut().push("parent"); Padding::all(-2.0) } => {
            { trace.borrow_mut().push("child"); SizedBox::height(10.0) };
        }
    }; });
    assert_eq!(*trace.borrow(), ["child", "parent"]);
    assert!(message.contains(&format!("{}:{line}:", file!())), "{message}");
    assert!(message.contains("top must be finite and nonnegative; got -2"));
    trace.borrow_mut().clear();
    let line = line!() + 3;
    let message = panic_message(|| { let _ = view! {
        { trace.borrow_mut().push("parent"); Column::new() } => {
            { trace.borrow_mut().push("bad-child"); SizedBox::width(f32::NAN) } => {
                { trace.borrow_mut().push("grandchild"); TextLabel::new("ok") };
            }
            { trace.borrow_mut().push("later"); TextLabel::new("never") };
        }
    }; });
    assert_eq!(*trace.borrow(), ["grandchild", "bad-child"]);
    assert!(message.contains(&format!("{}:{line}:", file!())), "{message}");
    assert!(message.contains("width must be finite and nonnegative; got NaN"));
    let message = panic_message(|| { let _ = view! { Column::new() => { SizedBox::height(-3.0); } }; });
    assert!(message.contains("height must be finite and nonnegative; got -3"));
    // Handwritten callers can recover before the macro imposes its policy.
    let recovered = SizedBox::width(-1.0).unwrap_or_else(|_| SizedBox::width(23.0).unwrap());
    let mut runtime = Runtime::new();
    runtime.set_root(view! { recovered; });
    update(&mut runtime);
    assert_eq!(runtime.arena().get(runtime.root_id().unwrap()).unwrap().layout_rect().unwrap().size(), Size::new(23.0, 0.0));
}

struct External {
    trace: Rc<RefCell<Vec<&'static str>>>,
    name: &'static str,
}
impl Component for External {
    fn build(&self, cx: &mut BuildCx) -> View {
        self.trace.borrow_mut().push(self.name);
        TextLabel::new(self.name).build(cx)
    }
}
#[test]
fn final_results_keep_deferred_builds_branch_laziness_moves_and_existing_view_keys() {
    let trace = Rc::new(RefCell::new(Vec::new()));
    let moved = String::from("owned");
    let existing = view! { TextLabel::new("existing").keyed("built-key"); };
    let existing_type = existing.widget_type();
    let tree = view! { Column::with_options(FlexOptions::default()) => {
        { trace.borrow_mut().push("first"); Ok::<_, &str>(External { trace: trace.clone(), name: "build-first" }) };
        if true {
            SizedBox::width(50.0) => { TextLabel::new(moved); }
        } else { Err::<TextLabel, _>("unselected error"); }
        match 2 {
            2 => { Ok::<_, &str>(existing); },
            _ => { Err::<TextLabel, _>("unselected match error"); }
        }
        for name in ["build-loop-1", "build-loop-2"] {
            { trace.borrow_mut().push("loop"); Ok::<_, &str>(External { trace: trace.clone(), name }) };
        }
    }};
    assert_eq!(*trace.borrow(), ["first", "loop", "loop"]);
    let mut runtime = Runtime::new();
    runtime.set_root(tree);
    update(&mut runtime);
    assert_eq!(
        *trace.borrow(),
        [
            "first",
            "loop",
            "loop",
            "build-first",
            "build-loop-1",
            "build-loop-2"
        ]
    );
    let existing = runtime
        .arena()
        .get(child(&runtime, runtime.root_id().unwrap(), 2))
        .unwrap();
    assert_eq!(existing.widget_type(), existing_type);
    assert_eq!(existing.key(), Some(&Key::new("built-key")));
    let root = view! { Ok::<_, &str>(view! { SizedBox::height(7.0); }); };
    runtime.set_root(root);
    update(&mut runtime);
    assert_eq!(
        runtime
            .arena()
            .get(runtime.root_id().unwrap())
            .unwrap()
            .layout_rect()
            .unwrap()
            .size(),
        Size::new(0.0, 7.0)
    );
}

#[test]
fn sized_slot_setters_blocks_and_empty_blocks_replace_and_dynamic_overcount_fails() {
    for clear in [false, true] {
        let staged = SizedBox::width(80.0)
            .unwrap()
            .child(TextLabel::new("old"))
            .child(TextLabel::new("replaced"));
        let tree = if clear {
            view! { Ok::<_, &str>(staged) => {} }
        } else {
            view! { Ok::<_, &str>(staged) => { TextLabel::new("selected").keyed("selected"); } }
        };
        let mut runtime = Runtime::new();
        runtime.set_root(tree);
        update(&mut runtime);
        let root = runtime.arena().get(runtime.root_id().unwrap()).unwrap();
        assert_eq!(root.children().len(), usize::from(!clear));
        if !clear {
            assert_eq!(
                runtime.arena().get(root.children()[0]).unwrap().key(),
                Some(&Key::new("selected"))
            );
        }
    }
    let incoming: Children = (0..2).map(|_| TextLabel::new("child")).collect();
    let error = SizedBox::height(5.0)
        .unwrap()
        .with_children(incoming)
        .err()
        .unwrap();
    assert_eq!(error.widget, "SizedBox");
    assert_eq!(error.received, 2);
    let count = 2;
    let message = panic_message(|| {
        let _ =
            view! { SizedBox::width(5.0) => { for _ in 0..count { TextLabel::new("child"); } } };
    });
    assert!(
        message.contains("SizedBox accepts at most one child; received 2 children"),
        "{message}"
    );
}

#[test]
fn layout_only_resize_keeps_identity_and_color_covers_allocated_rectangle() {
    let builds = Rc::new(Cell::new(0));
    let output = builds.clone();
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(600, 100, 1.0));
    runtime.set_root(move |_: &mut BuildCx| {
        output.set(output.get() + 1);
        view! { SizedBox::width(440.0).map(|s| s.background(harbor_widget::scene::primitive::Color::RED).expect("valid color")) => { TextLabel::new("content"); } }
    });
    update(&mut runtime);
    let root = runtime.root_id().unwrap();
    let content = child(&runtime, root, 0);
    assert_eq!(builds.get(), 1);
    for width in [300, 700] {
        runtime.set_viewport(Viewport::new(width, 100, 1.0));
        update(&mut runtime);
        assert_eq!(runtime.root_id(), Some(root));
        assert_eq!(child(&runtime, root, 0), content);
        assert_eq!(builds.get(), 1);
        let rect = runtime.arena().get(root).unwrap().layout_rect().unwrap();
        assert_eq!(rect.min, Point::ZERO);
        assert_eq!(rect.size(), Size::new((width as f32).min(440.0), 20.0));
        assert!(!runtime.arena().get(root).unwrap().is_focusable());
        assert!(runtime.pending_delta().unwrap().added.iter().chain(&runtime.pending_delta().unwrap().modified).any(|item| matches!(item.primitive, harbor_widget::scene::primitive::Primitive::Quad { rect: painted, color, .. } if painted == rect && color == harbor_widget::scene::primitive::Color::RED)));
    }
}

struct Stateful {
    builds: Rc<Cell<usize>>,
    initializations: Rc<Cell<usize>>,
}
impl Component for Stateful {
    fn build(&self, cx: &mut BuildCx) -> View {
        self.builds.set(self.builds.get() + 1);
        let state = cx.use_state(|| {
            self.initializations.set(self.initializations.get() + 1);
            13
        });
        assert_eq!(*state.read(), 13);
        Button::text("stateful").build(cx)
    }
}
#[test]
fn fallible_keyed_children_retain_state_focus_and_remount_only_when_parent_type_changes() {
    let order = Rc::new(RefCell::new(None::<Signal<Vec<&'static str>>>));
    let wrapped = Rc::new(RefCell::new(None::<Signal<bool>>));
    let builds = Rc::new(Cell::new(0));
    let initializations = Rc::new(Cell::new(0));
    let (o, w, b, i) = (
        order.clone(),
        wrapped.clone(),
        builds.clone(),
        initializations.clone(),
    );
    let mut runtime = Runtime::new();
    runtime.set_root(move |cx: &mut BuildCx| {
        let order = cx.use_state(|| vec!["a", "b"]);
        let wrapped = cx.use_state(|| false);
        *o.borrow_mut() = Some(order.clone()); *w.borrow_mut() = Some(wrapped.clone());
        let list = view! { Column::with_options(FlexOptions::default()) => {
            for key in order.read().iter() {
                Ok::<_, &str>(Stateful { builds: b.clone(), initializations: i.clone() }.keyed(*key));
            }
        }};
        view! { Column::new() => {
            if *wrapped.read() { Padding::all(0.0) => { list; } }
            else { SizedBox::width(200.0) => { list; } }
        }}
    });
    update(&mut runtime);
    let root = runtime.root_id().unwrap();
    let wrapper = child(&runtime, root, 0);
    let list = child(&runtime, wrapper, 0);
    let (a, b) = (child(&runtime, list, 0), child(&runtime, list, 1));
    runtime.set_focus(b);
    assert_eq!(runtime.input().focused(), Some(b));
    order.borrow().as_ref().unwrap().set(vec!["b", "c", "a"]);
    update(&mut runtime);
    assert_eq!(child(&runtime, list, 0), b);
    assert_eq!(child(&runtime, list, 2), a);
    assert_eq!(initializations.get(), 3);
    assert_eq!(runtime.input().focused(), Some(b));
    wrapped.borrow().as_ref().unwrap().set(true);
    update(&mut runtime);
    assert!(!runtime.arena().contains(a));
    assert!(!runtime.arena().contains(b));
    assert_eq!(
        initializations.get(),
        6,
        "a deliberate wrapper type change remounts its children"
    );
    runtime.set_root(view! { SizedBox::new(Size::ZERO); });
    update(&mut runtime);
    let before = builds.get();
    order.borrow().as_ref().unwrap().set(vec!["a"]);
    update(&mut runtime);
    assert_eq!(
        builds.get(),
        before,
        "unmounted subscriptions must not rebuild"
    );
}

#[test]
fn explicit_wrapper_order_preserves_scene_clips_and_hit_ranges() {
    use harbor_widget::input::event::{PointerButton, PointerEvent, PointerPhase, UiEvent};
    use harbor_widget::scene::primitive::Primitive;
    use harbor_widget::widgets::custom_paint::CustomPaint;
    use harbor_widget::{BorderRadius, BoxDecoration, ClipBehavior, DecoratedBox};
    for padding_outside in [true, false] {
        let decoration = BoxDecoration::new().border_radius(BorderRadius::all(8.0).unwrap());
        let content = if padding_outside {
            view! { Padding::all(8.0) => {
                DecoratedBox::new(decoration).clip_behavior(ClipBehavior::HardEdge) => { CustomPaint::new(91); }
            }}
        } else {
            view! { DecoratedBox::new(decoration).clip_behavior(ClipBehavior::HardEdge) => {
                Padding::all(8.0) => { CustomPaint::new(91); }
            }}
        };
        let mut runtime = Runtime::new();
        runtime.set_root(view! { SizedBox::new(Size::new(48.0, 48.0)) => { content; } });
        update(&mut runtime);
        let external = runtime
            .pending_delta()
            .unwrap()
            .added
            .iter()
            .find(|item| matches!(item.primitive, Primitive::External { .. }))
            .unwrap();
        let expected = if padding_outside {
            harbor_widget::layout::Rect::from_min_size(Point::new(8.0, 8.0), Size::new(32.0, 32.0))
        } else {
            harbor_widget::layout::Rect::from_min_size(Point::ZERO, Size::new(48.0, 48.0))
        };
        assert_eq!(external.clips.last().unwrap().rect(), expected);
        runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
            Point::new(8.0, 8.0),
            PointerPhase::Move,
            PointerButton::Left,
            1,
        )));
        let hits = runtime.drain_external_input();
        assert_eq!(!hits.is_empty(), !padding_outside);
    }
}

#[test]
fn clearing_sized_slot_releases_external_draw_schedule_ime_and_unmount_ownership() {
    use harbor_widget::scene::primitive::{
        ExternalDrawFn, ExternalImeFn, ExternalScheduleDemand, ExternalScheduleFn,
    };
    use harbor_widget::widgets::custom_paint::CustomPaint;
    use std::sync::Arc;
    let selected = Signal::new(true);
    let input = selected.clone();
    let unmounts = Rc::new(Cell::new(0));
    let unmounted = unmounts.clone();
    let draw: Arc<ExternalDrawFn<'static>> = Arc::new(|_, _, _, _, _| {});
    let schedule: Arc<ExternalScheduleFn> = Arc::new(|_, _| ExternalScheduleDemand::empty());
    let ime: Arc<ExternalImeFn> = Arc::new(|_, _| None);
    let weak_draw = Arc::downgrade(&draw);
    let weak_schedule = Arc::downgrade(&schedule);
    let weak_ime = Arc::downgrade(&ime);
    let providers = Rc::new(RefCell::new(Some((draw, schedule, ime))));
    let pending = providers.clone();
    let mut runtime = Runtime::new();
    runtime.set_root(move |cx: &mut BuildCx| {
        cx.track(&input);
        view! { SizedBox::new(Size::new(40.0, 20.0)) => {
            if *input.read() {
                {
                    let (draw, schedule, ime) = pending.borrow_mut().take().unwrap();
                    let unmounted = unmounted.clone();
                    Ok::<_, &str>(CustomPaint::new(92).handler(draw).schedule(schedule).ime(ime)
                        .on_unmount(Arc::new(move || unmounted.set(unmounted.get() + 1))))
                };
            }
        }}
    });
    update(&mut runtime);
    assert!(runtime.has_external_draws());
    assert!(
        weak_draw.upgrade().is_some()
            && weak_schedule.upgrade().is_some()
            && weak_ime.upgrade().is_some()
    );
    selected.set(false);
    update(&mut runtime);
    assert!(!runtime.has_external_draws());
    assert!(weak_draw.upgrade().is_none());
    assert!(weak_schedule.upgrade().is_none());
    assert!(weak_ime.upgrade().is_none());
    assert_eq!(unmounts.get(), 1);
    assert!(
        runtime
            .arena()
            .get(runtime.root_id().unwrap())
            .unwrap()
            .children()
            .is_empty()
    );
}
