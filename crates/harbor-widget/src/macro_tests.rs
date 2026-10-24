use crate::construction::{Children, ComponentExt, WithChildren};
use crate::input::event::{PointerButton, PointerEvent, PointerPhase, UiEvent};
use crate::layout::{Point, Rect, Size};
use crate::runtime::Runtime;
use crate::scene::primitive::{Color, Primitive};
use crate::signal::Signal;
use crate::view::{BuildCx, Component, View};
use crate::widgets::button::Button;
use crate::widgets::column::Column;
use crate::widgets::padding::Padding;
use crate::widgets::sized_box::SizedBox;
use std::any::TypeId;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

#[derive(Clone)]
struct MacroTree {
    values: Vec<Size>,
    show_extra: bool,
}

impl Component for MacroTree {
    fn build(&self, cx: &mut BuildCx) -> View {
        crate::view! { Column::new() => {
            SizedBox::new(Size::new(10.0, 10.0)).unwrap().color(Color::RED);
            SizedBox::new(Size::new(20.0, 10.0)).unwrap().color(Color::GREEN);
            for (index, size) in self.values.iter().copied().enumerate() {
                SizedBox::new(size).unwrap().color(Color::BLUE).keyed(index.to_string());
            }
            if self.show_extra {
                SizedBox::new(Size::new(30.0, 10.0)).unwrap().color(Color::WHITE);
            } else {
                SizedBox::new(Size::new(40.0, 10.0)).unwrap().color(Color::BLACK);
            }
            match self.values.first().copied() {
                Some(size) if size.width > 0.0 => {
                    SizedBox::new(size).unwrap().color(Color::TRANSPARENT);
                },
                Some(_) => {},
                None => {},
            }
        } }
    }
}

#[derive(Clone)]
struct HandwrittenTree {
    values: Vec<Size>,
    show_extra: bool,
}

impl Component for HandwrittenTree {
    fn build(&self, cx: &mut BuildCx) -> View {
        let mut children = Children::new();
        children.push(
            SizedBox::new(Size::new(10.0, 10.0))
                .unwrap()
                .color(Color::RED),
        );
        children.push(
            SizedBox::new(Size::new(20.0, 10.0))
                .unwrap()
                .color(Color::GREEN),
        );
        for (index, size) in self.values.iter().copied().enumerate() {
            children.push(
                SizedBox::new(size)
                    .unwrap()
                    .color(Color::BLUE)
                    .keyed(index.to_string()),
            );
        }
        if self.show_extra {
            children.push(
                SizedBox::new(Size::new(30.0, 10.0))
                    .unwrap()
                    .color(Color::WHITE),
            );
        } else {
            children.push(
                SizedBox::new(Size::new(40.0, 10.0))
                    .unwrap()
                    .color(Color::BLACK),
            );
        }
        match self.values.first().copied() {
            Some(size) if size.width > 0.0 => {
                children.push(SizedBox::new(size).unwrap().color(Color::TRANSPARENT));
            }
            Some(_) | None => {}
        }

        Column::new().with_children(children).unwrap().build(cx)
    }
}

#[derive(Clone)]
struct KeyedMacroList {
    items: Rc<RefCell<Option<Signal<Vec<&'static str>>>>>,
    initial: Vec<&'static str>,
}

impl Component for KeyedMacroList {
    fn build(&self, cx: &mut BuildCx) -> View {
        let state = cx.use_state(|| self.initial.clone());
        *self.items.borrow_mut() = Some(state.clone());
        let current_items = state.read();

        crate::view! { Column::new() => {
            for item in current_items.iter() {
                SizedBox::new(Size::new(10.0, 10.0)).unwrap().keyed(*item);
            }
        } }
    }
}

#[derive(Clone)]
struct MacroButtonTree {
    clicked: Arc<AtomicBool>,
}

impl Component for MacroButtonTree {
    fn build(&self, cx: &mut BuildCx) -> View {
        let clicked = self.clicked.clone();
        crate::view! { Column::new() => {
            Button::text("Click").on_click(move |_| clicked.store(true, Ordering::SeqCst));
        } }
    }
}

#[derive(Clone)]
struct HandwrittenButtonTree {
    clicked: Arc<AtomicBool>,
}

impl Component for HandwrittenButtonTree {
    fn build(&self, cx: &mut BuildCx) -> View {
        let clicked = self.clicked.clone();
        let children = Children::one(
            Button::text("Click").on_click(move |_| clicked.store(true, Ordering::SeqCst)),
        );
        Column::new().with_children(children).unwrap().build(cx)
    }
}

#[derive(Clone, Debug, PartialEq)]
struct FiberSignature {
    widget_type: TypeId,
    rect: Option<Rect>,
    children: Vec<FiberSignature>,
}

fn fiber_signature(runtime: &Runtime, id: crate::fiber::FiberId) -> FiberSignature {
    let fiber = runtime.arena().get(id).unwrap();
    FiberSignature {
        widget_type: fiber.widget_type(),
        rect: fiber.layout_rect(),
        children: fiber
            .children()
            .iter()
            .copied()
            .map(|child| fiber_signature(runtime, child))
            .collect(),
    }
}

/// Compares fiber trees from the root's first child, ignoring root identity:
/// the mounted description's component type differs by design from the
/// handwritten build's returned concrete root type (ADR 0079: the root Fiber
/// identity belongs to the deferred description, not the flattened result).
fn child_tree_signature(sig: &FiberSignature) -> Vec<(Option<Rect>, TypeId)> {
    fn walk(sig: &FiberSignature) -> Vec<(Option<Rect>, TypeId)> {
        let mut out = vec![(sig.rect, sig.widget_type)];
        for child in &sig.children {
            out.extend(walk(child));
        }
        out
    }
    sig.children.iter().flat_map(walk).collect()
}
fn scene_signature(runtime: &Runtime) -> Vec<(Primitive, u32)> {
    runtime
        .pending_delta()
        .unwrap()
        .added
        .iter()
        .map(|item| (item.primitive.clone(), item.paint_order))
        .collect()
}

fn click(runtime: &mut Runtime, position: Point) {
    for phase in [PointerPhase::Down, PointerPhase::Up] {
        runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
            position,
            phase,
            PointerButton::Left,
            0,
        )));
    }
}

#[test]
fn declarative_tree_matches_handwritten_layout_and_scene_output() {
    let values = vec![Size::new(15.0, 10.0), Size::new(25.0, 10.0)];
    let mut macro_runtime = Runtime::new();
    macro_runtime.set_root(MacroTree {
        values: values.clone(),
        show_extra: true,
    });
    macro_runtime.update(Instant::now());

    let mut handwritten_runtime = Runtime::new();
    handwritten_runtime.set_root(HandwrittenTree {
        values,
        show_extra: true,
    });
    handwritten_runtime.update(Instant::now());

    assert_eq!(
        child_tree_signature(&fiber_signature(
            &macro_runtime,
            macro_runtime.root_id().unwrap()
        )),
        child_tree_signature(&fiber_signature(
            &handwritten_runtime,
            handwritten_runtime.root_id().unwrap(),
        )),
    );
    assert_eq!(
        macro_runtime
            .arena()
            .get(macro_runtime.root_id().unwrap())
            .unwrap()
            .widget_type(),
        TypeId::of::<MacroTree>(),
    );
    assert_eq!(
        scene_signature(&macro_runtime),
        scene_signature(&handwritten_runtime),
    );
}

#[test]
fn declarative_static_leaf_and_single_child_trees_match_handwritten_fibers() {
    #[derive(Clone)]
    struct MacroLeaf;

    impl Component for MacroLeaf {
        fn build(&self, cx: &mut BuildCx) -> View {
            crate::view! { SizedBox::new(Size::new(10.0, 10.0)); }
        }
    }

    #[derive(Clone)]
    struct HandwrittenLeaf;

    impl Component for HandwrittenLeaf {
        fn build(&self, cx: &mut BuildCx) -> View {
            SizedBox::new(Size::new(10.0, 10.0)).unwrap().build(cx)
        }
    }

    #[derive(Clone)]
    struct MacroSingle;

    impl Component for MacroSingle {
        fn build(&self, cx: &mut BuildCx) -> View {
            crate::view! { Padding::all(2.0) => {
                SizedBox::new(Size::new(10.0, 10.0));
            } }
        }
    }

    #[derive(Clone)]
    struct HandwrittenSingle;

    impl Component for HandwrittenSingle {
        fn build(&self, cx: &mut BuildCx) -> View {
            Padding::all(2.0)
                .unwrap()
                .child(SizedBox::new(Size::new(10.0, 10.0)).unwrap())
                .build(cx)
        }
    }

    let mut macro_leaf = Runtime::new();
    macro_leaf.set_root(MacroLeaf);
    macro_leaf.update(Instant::now());
    let mut handwritten_leaf = Runtime::new();
    handwritten_leaf.set_root(HandwrittenLeaf);
    handwritten_leaf.update(Instant::now());
    assert_eq!(
        child_tree_signature(&fiber_signature(&macro_leaf, macro_leaf.root_id().unwrap())),
        child_tree_signature(&fiber_signature(
            &handwritten_leaf,
            handwritten_leaf.root_id().unwrap()
        )),
    );
    // The macro root fiber keeps the mounted description's component identity
    // instead of the flattened concrete root type.
    assert_eq!(
        macro_leaf
            .arena()
            .get(macro_leaf.root_id().unwrap())
            .unwrap()
            .widget_type(),
        TypeId::of::<MacroLeaf>(),
    );

    let mut macro_single = Runtime::new();
    macro_single.set_root(MacroSingle);
    macro_single.update(Instant::now());
    let mut handwritten_single = Runtime::new();
    handwritten_single.set_root(HandwrittenSingle);
    handwritten_single.update(Instant::now());
    assert_eq!(
        child_tree_signature(&fiber_signature(
            &macro_single,
            macro_single.root_id().unwrap()
        )),
        child_tree_signature(&fiber_signature(
            &handwritten_single,
            handwritten_single.root_id().unwrap()
        )),
    );
}

#[test]
fn declarative_tree_matches_handwritten_event_target() {
    let macro_clicked = Arc::new(AtomicBool::new(false));
    let handwritten_clicked = Arc::new(AtomicBool::new(false));
    let mut macro_runtime = Runtime::new();
    macro_runtime.set_root(MacroButtonTree {
        clicked: macro_clicked.clone(),
    });
    macro_runtime.update(Instant::now());

    let mut handwritten_runtime = Runtime::new();
    handwritten_runtime.set_root(HandwrittenButtonTree {
        clicked: handwritten_clicked.clone(),
    });
    handwritten_runtime.update(Instant::now());

    let point = Point::new(10.0, 16.0);
    click(&mut macro_runtime, point);
    click(&mut handwritten_runtime, point);

    assert!(macro_clicked.load(Ordering::SeqCst));
    assert_eq!(
        macro_clicked.load(Ordering::SeqCst),
        handwritten_clicked.load(Ordering::SeqCst),
        "the same pointer sequence must reach the corresponding macro and handwritten button",
    );
}

#[test]
fn keyed_for_children_reorder_without_fiber_replacement() {
    let items = Rc::new(RefCell::new(None));
    let mut runtime = Runtime::new();
    runtime.set_root(KeyedMacroList {
        items: items.clone(),
        initial: vec!["a", "b", "c"],
    });
    runtime.update(Instant::now());

    let root_id = runtime.root_id().unwrap();
    let initial = runtime.arena().get(root_id).unwrap().children().to_vec();
    assert_eq!(initial.len(), 3);

    items.borrow().as_ref().unwrap().set(vec!["c", "a", "b"]);
    runtime.update(Instant::now());

    assert_eq!(
        runtime.arena().get(root_id).unwrap().children(),
        &[initial[2], initial[0], initial[1]],
        "macro expansion must retain keyed fibers while changing source order",
    );
}

#[test]
fn macro_preserves_expression_evaluation_order_and_reachability() {
    fn logged_column(log: &RefCell<Vec<&'static str>>, label: &'static str) -> Column {
        log.borrow_mut().push(label);
        Column::new()
    }

    fn logged_choice(log: &RefCell<Vec<&'static str>>) -> Option<i32> {
        log.borrow_mut().push("match-expression");
        Some(1)
    }

    fn logged_leaf(log: &RefCell<Vec<&'static str>>, label: &'static str) -> SizedBox {
        log.borrow_mut().push(label);
        SizedBox::new(Size::new(1.0, 1.0)).unwrap()
    }

    fn logged_bool(log: &RefCell<Vec<&'static str>>, label: &'static str, value: bool) -> bool {
        log.borrow_mut().push(label);
        value
    }

    let log = RefCell::new(Vec::new());
    let _ = crate::view! {
        logged_column(&log, "root") => {
            logged_leaf(&log, "first");
            logged_column(&log, "parent") => {
                logged_leaf(&log, "nested");
            }
            for _ in { log.borrow_mut().push("iterator"); 0..2 } {
                logged_leaf(&log, "loop");
            }
            if logged_bool(&log, "if-condition", false) {
                logged_leaf(&log, "untaken");
            } else {
                logged_leaf(&log, "else");
            }
            match logged_choice(&log) {
                Some(value) if logged_bool(&log, "guard", value == 1) => {
                    logged_leaf(&log, "match");
                }
                _ => {
                    logged_leaf(&log, "wrong-arm");
                }
            }
        }
    };

    assert_eq!(
        *log.borrow(),
        [
            "first",
            "nested",
            "parent",
            "iterator",
            "loop",
            "loop",
            "if-condition",
            "else",
            "match-expression",
            "guard",
            "match",
            "root",
        ]
    );
}

#[test]
fn leaf_root_defers_build_until_reconciliation() {
    #[derive(Clone)]
    struct LoggedRoot {
        log: Rc<RefCell<Vec<&'static str>>>,
    }

    impl Component for LoggedRoot {
        fn build(&self, _cx: &mut BuildCx) -> View {
            self.log.borrow_mut().push("build");
            SizedBox::new(Size::new(1.0, 1.0)).unwrap().build(_cx)
        }
    }

    fn root(log: &Rc<RefCell<Vec<&'static str>>>) -> LoggedRoot {
        log.borrow_mut().push("component");
        LoggedRoot { log: log.clone() }
    }

    let log = Rc::new(RefCell::new(Vec::new()));
    let description = crate::view! { root(&log); };
    drop(description);

    // The macro evaluates the root expression once and converts it through
    // `IntoChildView`; the component must not be built yet. Building happens
    // when a Runtime mounts the component against its own Fiber.
    assert_eq!(*log.borrow(), ["component"]);

    let mut runtime = Runtime::new();
    runtime.set_root(root(&log));
    runtime.update(Instant::now());
    runtime.update(Instant::now());
    assert_eq!(*log.borrow(), ["component", "component", "build"]);

    // A subsequent update with no dirty state must not rebuild the root
    // description again.
    runtime.update(Instant::now());
    assert_eq!(*log.borrow(), ["component", "component", "build"]);
}

#[test]
fn self_deferring_build_chain_fails_loudly() {
    #[derive(Clone)]
    struct SelfDefer;

    impl Component for SelfDefer {
        fn build(&self, _cx: &mut BuildCx) -> View {
            View::deferred(SelfDefer)
        }
    }

    let mut runtime = Runtime::new();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime.set_root(SelfDefer);
        runtime.update(Instant::now());
    }));
    assert!(
        outcome.is_err(),
        "a build chain that only defers itself must be rejected, not spin"
    );
}

#[test]
fn block_replacement_matches_builders_including_keys_scene_and_routing() {
    for nested in [false, true] {
        let mut runtimes = Vec::new();
        for use_macro in [false, true] {
            let clicked = Arc::new(AtomicBool::new(false));
            let callback = clicked.clone();
            let incoming = Button::text("replacement")
                .on_click(move |_| callback.store(true, Ordering::SeqCst))
                .keyed("b");
            let parent = Padding::all(4.0)
                .unwrap()
                .child(Button::text("old").keyed("a"));
            let description = if use_macro {
                crate::view! { parent => { incoming; } }
            } else {
                crate::construction::IntoChildView::into_child_view(parent.child(incoming))
            };
            let mut runtime = Runtime::new();
            if nested {
                runtime.set_root(crate::view! { Column::new() => { description; } });
            } else {
                runtime.set_root(description);
            }
            runtime.update(Instant::now());
            let parent_id = if nested {
                runtime
                    .arena()
                    .get(runtime.root_id().unwrap())
                    .unwrap()
                    .children()[0]
            } else {
                runtime.root_id().unwrap()
            };
            let children = runtime.arena().get(parent_id).unwrap().children();
            assert_eq!(children.len(), 1);
            assert_eq!(
                runtime.arena().get(children[0]).unwrap().key(),
                Some(&crate::view::Key::new("b"))
            );
            click(&mut runtime, Point::new(10.0, 16.0));
            assert!(clicked.load(Ordering::SeqCst));
            runtimes.push(runtime);
        }
        assert_eq!(
            fiber_signature(&runtimes[0], runtimes[0].root_id().unwrap()),
            fiber_signature(&runtimes[1], runtimes[1].root_id().unwrap())
        );
        assert_eq!(scene_signature(&runtimes[0]), scene_signature(&runtimes[1]));
    }
}

#[test]
fn empty_block_clears_but_value_expression_preserves_staged_content() {
    for clear in [false, true] {
        let staged = Padding::all(2.0)
            .unwrap()
            .child(SizedBox::new(Size::new(3.0, 4.0)).unwrap());
        let tree = if clear {
            crate::view! { staged => {} }
        } else {
            crate::view! { staged; }
        };
        let mut runtime = Runtime::new();
        runtime.set_root(tree);
        runtime.update(Instant::now());
        assert_eq!(
            runtime
                .arena()
                .get(runtime.root_id().unwrap())
                .unwrap()
                .children()
                .len(),
            usize::from(!clear)
        );
    }
}

#[test]
fn dynamic_macro_validates_zero_one_many_at_construction_and_not_layout() {
    for staged in [false, true] {
        for count in 0..=2 {
            let evaluated = RefCell::new(Vec::new());
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::view! {
                    {
                        evaluated.borrow_mut().push("parent");
                        let parent = Padding::all(1.0).unwrap();
                        if staged { parent.child(SizedBox::new(Size::ZERO).unwrap()) } else { parent }
                    } => {
                        for _ in 0..count {
                            { evaluated.borrow_mut().push("child"); SizedBox::new(Size::new(2.0, 3.0)) };
                        }
                    }
                }
            }));
            assert_eq!(&evaluated.borrow()[..count], vec!["child"; count]);
            assert_eq!(evaluated.borrow().last(), Some(&"parent"));
            match outcome {
                Ok(tree) => {
                    assert!(count <= 1);
                    let mut runtime = Runtime::new();
                    runtime.set_root(tree);
                    runtime.update(Instant::now());
                    assert_eq!(
                        runtime
                            .arena()
                            .get(runtime.root_id().unwrap())
                            .unwrap()
                            .children()
                            .len(),
                        count
                    );
                }
                Err(payload) => {
                    assert_eq!(count, 2);
                    let message = payload.downcast_ref::<String>().unwrap();
                    assert!(message.contains("view! child construction failed: Padding accepts at most one child; received 2 children"));
                }
            }
        }
    }
}

#[test]
fn bounded_alternatives_are_lazy_single_assignments_and_builds_stay_deferred() {
    for choice in 0..3 {
        let log = Rc::new(RefCell::new(Vec::new()));
        let child = |name| {
            log.borrow_mut().push(name);
            let log = log.clone();
            move |cx: &mut BuildCx| {
                log.borrow_mut().push("build");
                SizedBox::new(Size::new(2.0, 3.0)).unwrap().build(cx)
            }
        };
        let tree = crate::view! {
            { log.borrow_mut().push("parent"); Padding::all(0.0).unwrap().child(SizedBox::new(Size::ZERO).unwrap()) } => {
                if { log.borrow_mut().push("condition"); choice == 0 } {
                    child("if-child");
                } else if choice == 1 {
                    match { log.borrow_mut().push("match"); choice } {
                        1 if { log.borrow_mut().push("guard"); true } => { child("match-child"); },
                        _ => { child("unselected"); },
                    }
                } else {}
            }
        };
        let expected = match choice {
            0 => vec!["condition", "if-child", "parent"],
            1 => vec!["condition", "match", "guard", "match-child", "parent"],
            _ => vec!["condition", "parent"],
        };
        assert_eq!(*log.borrow(), expected);
        let mut runtime = Runtime::new();
        runtime.set_root(tree);
        runtime.update(Instant::now());
        assert_eq!(
            runtime
                .arena()
                .get(runtime.root_id().unwrap())
                .unwrap()
                .children()
                .len(),
            usize::from(choice < 2)
        );
        assert_eq!(
            log.borrow().iter().filter(|&&s| s == "build").count(),
            usize::from(choice < 2)
        );
    }
}

#[test]
fn overwritten_staged_expression_still_runs_once_after_block_children() {
    let log = RefCell::new(Vec::new());
    let tree = crate::view! {
        {
            log.borrow_mut().push("parent");
            Padding::all(1.0).unwrap().child({
                log.borrow_mut().push("staged");
                SizedBox::new(Size::ZERO).unwrap()
            })
        } => {
            { log.borrow_mut().push("incoming"); SizedBox::new(Size::new(1.0, 2.0)) };
        }
    };
    assert_eq!(*log.borrow(), vec!["incoming", "parent", "staged"]);
    let mut runtime = Runtime::new();
    runtime.set_root(tree);
    runtime.update(Instant::now());
    assert_eq!(*log.borrow(), vec!["incoming", "parent", "staged"]);
}
