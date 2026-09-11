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
use std::cell::{Cell, RefCell};
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
        crate::view!(cx, Column::new() => {
            SizedBox::new(Size::new(10.0, 10.0)).color(Color::RED) => {}
            { SizedBox::new(Size::new(20.0, 10.0)).color(Color::GREEN) }
            for (index, size) in self.values.iter().copied().enumerate() {
                SizedBox::new(size).color(Color::BLUE).keyed(index.to_string()) => {}
            }
            if self.show_extra {
                SizedBox::new(Size::new(30.0, 10.0)).color(Color::WHITE) => {}
            } else {
                SizedBox::new(Size::new(40.0, 10.0)).color(Color::BLACK) => {}
            }
            match self.values.first().copied() {
                Some(size) if size.width > 0.0 => {
                    SizedBox::new(size).color(Color::TRANSPARENT) => {}
                },
                Some(_) => {},
                None => {},
            }
        })
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
        children.push(SizedBox::new(Size::new(10.0, 10.0)).color(Color::RED));
        children.push(SizedBox::new(Size::new(20.0, 10.0)).color(Color::GREEN));
        for (index, size) in self.values.iter().copied().enumerate() {
            children.push(
                SizedBox::new(size)
                    .color(Color::BLUE)
                    .keyed(index.to_string()),
            );
        }
        if self.show_extra {
            children.push(SizedBox::new(Size::new(30.0, 10.0)).color(Color::WHITE));
        } else {
            children.push(SizedBox::new(Size::new(40.0, 10.0)).color(Color::BLACK));
        }
        match self.values.first().copied() {
            Some(size) if size.width > 0.0 => {
                children.push(SizedBox::new(size).color(Color::TRANSPARENT));
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

        crate::view!(cx, Column::new() => {
            for item in current_items.iter() {
                SizedBox::new(Size::new(10.0, 10.0)).keyed(*item) => {}
            }
        })
    }
}

#[derive(Clone)]
struct MacroButtonTree {
    clicked: Arc<AtomicBool>,
}

impl Component for MacroButtonTree {
    fn build(&self, cx: &mut BuildCx) -> View {
        let clicked = self.clicked.clone();
        crate::view!(cx, Column::new() => {
            Button::new("Click").on_click(move |_| clicked.store(true, Ordering::SeqCst)) => {}
        })
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
            Button::new("Click").on_click(move |_| clicked.store(true, Ordering::SeqCst)),
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
        fiber_signature(&macro_runtime, macro_runtime.root_id().unwrap()),
        fiber_signature(&handwritten_runtime, handwritten_runtime.root_id().unwrap()),
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
            crate::view!(cx, SizedBox::new(Size::new(10.0, 10.0)) => {})
        }
    }

    #[derive(Clone)]
    struct HandwrittenLeaf;

    impl Component for HandwrittenLeaf {
        fn build(&self, cx: &mut BuildCx) -> View {
            SizedBox::new(Size::new(10.0, 10.0)).build(cx)
        }
    }

    #[derive(Clone)]
    struct MacroSingle;

    impl Component for MacroSingle {
        fn build(&self, cx: &mut BuildCx) -> View {
            crate::view!(cx, Padding::all(2.0) => {
                SizedBox::new(Size::new(10.0, 10.0)) => {}
            })
        }
    }

    #[derive(Clone)]
    struct HandwrittenSingle;

    impl Component for HandwrittenSingle {
        fn build(&self, cx: &mut BuildCx) -> View {
            Padding::all(2.0)
                .child(SizedBox::new(Size::new(10.0, 10.0)))
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
        fiber_signature(&macro_leaf, macro_leaf.root_id().unwrap()),
        fiber_signature(&handwritten_leaf, handwritten_leaf.root_id().unwrap()),
    );

    let mut macro_single = Runtime::new();
    macro_single.set_root(MacroSingle);
    macro_single.update(Instant::now());
    let mut handwritten_single = Runtime::new();
    handwritten_single.set_root(HandwrittenSingle);
    handwritten_single.update(Instant::now());
    assert_eq!(
        fiber_signature(&macro_single, macro_single.root_id().unwrap()),
        fiber_signature(&handwritten_single, handwritten_single.root_id().unwrap()),
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
fn macro_evaluates_each_component_expression_once() {
    fn counted_column(calls: &Cell<u32>) -> Column {
        calls.set(calls.get() + 1);
        Column::new()
    }

    fn column_using_cx(_cx: &mut BuildCx, calls: &Cell<u32>) -> Column {
        calls.set(calls.get() + 1);
        Column::new()
    }

    fn counted_cx<'a>(cx: &'a mut BuildCx, calls: &Cell<u32>) -> &'a mut BuildCx {
        calls.set(calls.get() + 1);
        cx
    }

    let calls = Cell::new(0);
    let cx_calls = Cell::new(0);
    let child_cx_calls = Cell::new(0);
    let mut cx = BuildCx::stub();
    let _ = crate::view!(counted_cx(&mut cx, &cx_calls), counted_column(&calls) => {
        column_using_cx(&mut cx, &child_cx_calls) => {}
    });

    assert_eq!(calls.get(), 1);
    assert_eq!(child_cx_calls.get(), 1);
    assert_eq!(cx_calls.get(), 1);
}
