use harbor_widget::layout::{Point, Size};
use harbor_widget::runtime::Runtime;
use harbor_widget::scene::primitive::{Color, Primitive};
use harbor_widget::signal::Signal;
use harbor_widget::view::{BuildCx, Component, Key, View};
use harbor_widget::widgets::sized_box::SizedBox;
use harbor_widget::{Border, BorderRadius, BoxDecoration, ClipBehavior, DecoratedBox};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

// ── Helper component that nests a SizedBox inside ────────────────────────────

struct Wrapper {
    inner_size: Size,
}

impl Component for Wrapper {
    fn build(&self, cx: &mut BuildCx) -> View {
        // Build the inner SizedBox using the same BuildCx
        let inner = SizedBox::new(self.inner_size).build(cx);
        // Wrap in a container view (using SizedBox as the container)
        View::new(SizedBox::new(Size::new(200.0, 200.0)), vec![inner], None)
    }
}

// ── Integration Tests ────────────────────────────────────────────────────────

#[test]
fn sized_box_layout_integration() {
    let mut rt = Runtime::new();
    rt.set_root(SizedBox::new(Size::new(100.0, 50.0)));

    let req = rt.update(Instant::now());
    assert!(req.request_redraw, "first build should request redraw");

    // Verify root fiber has layout_rect with size 100x50
    let root_id = rt.root_id().unwrap();
    let arena = rt.arena();
    let fiber = arena.get(root_id).unwrap();
    assert!(
        fiber.layout_rect().is_some(),
        "root should have layout_rect"
    );
    let rect = fiber.layout_rect().unwrap();
    assert_eq!(rect.min, Point::ZERO);
    assert_eq!(rect.size(), Size::new(100.0, 50.0));
}

#[test]
fn second_update_without_changes_no_redraw() {
    let mut rt = Runtime::new();
    rt.set_root(SizedBox::new(Size::new(100.0, 50.0)));

    let req1 = rt.update(Instant::now());
    assert!(req1.request_redraw);

    let req2 = rt.update(Instant::now());
    assert!(
        !req2.request_redraw,
        "second update without changes should skip redraw"
    );
}

#[test]
fn set_root_twice_replaces_tree() {
    let mut rt = Runtime::new();
    rt.set_root(SizedBox::new(Size::new(100.0, 50.0)));
    rt.update(Instant::now());

    let first_root = rt.root_id().unwrap();

    // Replace with new root
    rt.set_root(SizedBox::new(Size::new(200.0, 100.0)));
    let second_root = rt.root_id().unwrap();

    // Should be a new fiber (different generation)
    assert_ne!(
        first_root, second_root,
        "new root should have a different FiberId"
    );

    let req = rt.update(Instant::now());
    assert!(req.request_redraw);

    let fiber = rt.arena().get(second_root).unwrap();
    assert_eq!(fiber.layout_rect().unwrap().size(), Size::new(200.0, 100.0));
}

#[test]
fn nested_sized_box_produces_correct_layout_rects() {
    let mut rt = Runtime::new();
    rt.set_root(Wrapper {
        inner_size: Size::new(50.0, 25.0),
    });

    let req = rt.update(Instant::now());
    assert!(req.request_redraw);

    let root_id = rt.root_id().unwrap();
    let arena = rt.arena();
    let root = arena.get(root_id).unwrap();

    // Root (Wrapper container) should have 200x200
    assert_eq!(root.layout_rect().unwrap().size(), Size::new(200.0, 200.0));

    // Child (inner SizedBox) should have 50x25
    assert_eq!(root.children().len(), 1, "root should have one child");
    let child = arena.get(root.children()[0]).unwrap();
    assert_eq!(child.layout_rect().unwrap().size(), Size::new(50.0, 25.0));
}

#[test]
fn empty_tree_no_crash() {
    let mut rt = Runtime::new();
    rt.set_root(SizedBox::new(Size::new(100.0, 50.0)));
    rt.update(Instant::now());
    rt.update(Instant::now());
    rt.update(Instant::now());
}

#[test]
fn signal_write_triggers_rebuild() {
    #[derive(Clone)]
    struct StatefulWidget {
        state: Rc<RefCell<Option<Signal<u32>>>>,
    }

    impl Component for StatefulWidget {
        fn build(&self, cx: &mut BuildCx) -> View {
            let state: Signal<u32> = cx.use_state(|| 0u32);
            *self.state.borrow_mut() = Some(state.clone());
            SizedBox::new(Size::new(100.0, 50.0)).build(cx)
        }
    }

    let shared_state: Rc<RefCell<Option<Signal<u32>>>> = Rc::new(RefCell::new(None));
    let mut rt = Runtime::new();
    rt.set_root(StatefulWidget {
        state: shared_state.clone(),
    });

    let req1 = rt.update(Instant::now());
    assert!(req1.request_redraw);

    let state = shared_state.borrow().as_ref().unwrap().clone();
    state.set(1);

    let req2 = rt.update(Instant::now());
    assert!(req2.request_redraw, "signal change should trigger redraw");

    let req3 = rt.update(Instant::now());
    assert!(
        !req3.request_redraw,
        "after processing the signal change, the tree should be clean"
    );
}

#[test]
fn runtimes_consume_only_their_own_signal_dirty_notifications() {
    #[derive(Clone)]
    struct StatefulWidget {
        state: Rc<RefCell<Option<Signal<u32>>>>,
        size: Size,
    }

    impl Component for StatefulWidget {
        fn build(&self, cx: &mut BuildCx) -> View {
            let state = cx.use_state(|| 0u32);
            *self.state.borrow_mut() = Some(state);
            SizedBox::new(self.size).build(cx)
        }
    }

    let first_state = Rc::new(RefCell::new(None));
    let second_state = Rc::new(RefCell::new(None));
    let mut first = Runtime::new();
    let mut second = Runtime::new();
    first.set_root(StatefulWidget {
        state: first_state.clone(),
        size: Size::new(100.0, 50.0),
    });
    second.set_root(StatefulWidget {
        state: second_state.clone(),
        size: Size::new(200.0, 100.0),
    });
    assert!(first.update(Instant::now()).request_redraw);
    assert!(second.update(Instant::now()).request_redraw);

    first_state.borrow().as_ref().unwrap().set(1);
    assert!(first.update(Instant::now()).request_redraw);
    assert!(!second.update(Instant::now()).request_redraw);

    second_state.borrow().as_ref().unwrap().set(1);
    assert!(second.update(Instant::now()).request_redraw);
    assert!(!first.update(Instant::now()).request_redraw);
}

#[test]
fn view_key_preserved() {
    let sb = SizedBox::new(Size::new(10.0, 10.0));
    let view = View::new(sb, vec![], Some(Key::new("test_key")));
    assert_eq!(view.key(), Some(&Key::new("test_key")));
    assert_eq!(view.widget_type(), std::any::TypeId::of::<SizedBox>());
}

// ── FocusScope modal integration ────────────────────────────────────────────

#[test]
fn runtime_detects_modal_in_deeply_nested_tree() {
    use harbor_widget::widgets::column::Column;
    use harbor_widget::widgets::focus_scope::FocusScope;
    use harbor_widget::widgets::padding::Padding;

    let mut rt = Runtime::new();
    // Non-modal root wrapping a modal scope
    let root = Padding::new(8.0, 8.0, 8.0, 8.0).child(
        Column::new().child(
            FocusScope::new()
                .modal(true)
                .child(SizedBox::new(Size::new(100.0, 50.0))),
        ),
    );
    rt.set_root(root);
    rt.update(Instant::now());
    assert!(
        rt.has_modal(),
        "modal nested inside padding+column should be detected"
    );
}

#[test]
fn runtime_no_modal_when_tree_is_purely_non_modal() {
    use harbor_widget::widgets::column::Column;

    let mut rt = Runtime::new();
    let root = Column::new()
        .child(SizedBox::new(Size::new(100.0, 50.0)))
        .child(SizedBox::new(Size::new(100.0, 50.0)));
    rt.set_root(root);
    rt.update(Instant::now());
    assert!(!rt.has_modal());
}

// ── Viewport change re-layout ───────────────────────────────────────────────

#[test]
fn viewport_change_preserves_correct_layout_rect() {
    use harbor_widget::renderer::Viewport;

    let mut rt = Runtime::new();
    rt.set_root(SizedBox::new(Size::new(100.0, 50.0)));
    rt.update(Instant::now());

    // Set viewport to a smaller size
    rt.set_viewport(Viewport::new(200, 150, 1.0));
    rt.update(Instant::now());

    let root_id = rt.root_id().unwrap();
    let arena = rt.arena();
    let fiber = arena.get(root_id).unwrap();
    let rect = fiber.layout_rect().unwrap();
    // SizedBox(100, 50) fits within 200x150 viewport
    assert_eq!(rect.size(), Size::new(100.0, 50.0));
}

// ── Signal write after root replacement ─────────────────────────────────────

#[test]
fn signal_write_after_root_replacement_triggers_rebuild() {
    #[derive(Clone)]
    struct CountingWidget {
        counter: Rc<RefCell<Option<Signal<u32>>>>,
    }

    impl Component for CountingWidget {
        fn build(&self, cx: &mut BuildCx) -> View {
            let state: Signal<u32> = cx.use_state(|| 0u32);
            *self.counter.borrow_mut() = Some(state.clone());
            SizedBox::new(Size::new(100.0, 50.0)).build(cx)
        }
    }

    let shared: Rc<RefCell<Option<Signal<u32>>>> = Rc::new(RefCell::new(None));
    let mut rt = Runtime::new();

    // First root
    rt.set_root(CountingWidget {
        counter: shared.clone(),
    });
    rt.update(Instant::now());
    let sig1 = shared.borrow().as_ref().unwrap().clone();

    // Replace root with same type
    rt.set_root(CountingWidget {
        counter: shared.clone(),
    });
    rt.update(Instant::now());
    let sig2 = shared.borrow().as_ref().unwrap().clone();

    // Signal from old root should no longer trigger rebuild
    sig1.set(42);
    let req = rt.update(Instant::now());
    assert!(
        !req.request_redraw,
        "old signal should not trigger rebuild after root replacement"
    );

    // Signal from new root should trigger rebuild
    sig2.set(99);
    let req2 = rt.update(Instant::now());
    assert!(req2.request_redraw, "new signal should trigger rebuild");
}

// ── Multiple set_root cycles ─────────────────────────────────────────────────

#[test]
fn multiple_set_root_without_update_works() {
    let mut rt = Runtime::new();
    rt.set_root(SizedBox::new(Size::new(100.0, 50.0)));
    rt.set_root(SizedBox::new(Size::new(200.0, 100.0)));
    rt.set_root(SizedBox::new(Size::new(300.0, 150.0)));
    let req = rt.update(Instant::now());
    assert!(req.request_redraw);

    let root_id = rt.root_id().unwrap();
    let fiber = rt.arena().get(root_id).unwrap();
    assert_eq!(fiber.layout_rect().unwrap().size(), Size::new(300.0, 150.0));
}

// ── Padding with zero in all directions ──────────────────────────────────────

#[test]
fn zero_padding_is_noop_at_runtime_level() {
    use harbor_widget::widgets::padding::Padding;

    let mut rt = Runtime::new();
    rt.set_root(
        Padding::new(0.0, 0.0, 0.0, 0.0)
            .child(SizedBox::new(Size::new(100.0, 50.0)).color(Color::RED)),
    );
    let req = rt.update(Instant::now());
    assert!(req.request_redraw);

    let root_id = rt.root_id().unwrap();
    let arena = rt.arena();
    let root = arena.get(root_id).unwrap();
    // Padding own size = child size (100x50) with zero inset
    assert_eq!(root.layout_rect().unwrap().size(), Size::new(100.0, 50.0));

    // Child should be at (0,0) and have same size
    let child = arena.get(root.children()[0]).unwrap();
    assert_eq!(child.layout_rect().unwrap().size(), Size::new(100.0, 50.0));
}

#[test]
fn should_paint_decorated_box_in_background_child_border_order_without_false_clip_metadata() {
    // Arrange
    let border = Border::all(Color::BLUE, 2.0).unwrap();
    let decoration = BoxDecoration::new()
        .try_color(Color::RED)
        .unwrap()
        .border(border)
        .border_radius(BorderRadius::all(8.0).unwrap());
    let mut rt = Runtime::new();
    rt.set_root(
        DecoratedBox::new(decoration)
            .clip_behavior(ClipBehavior::HardEdge)
            .child(SizedBox::new(Size::new(40.0, 24.0)).color(Color::GREEN)),
    );

    // Act
    rt.update(Instant::now());

    // Assert
    let delta = rt.pending_delta().unwrap();
    assert_eq!(delta.added.len(), 3);
    assert!(matches!(
        delta.added[0].primitive,
        Primitive::RoundedQuad { .. }
    ));
    assert!(matches!(delta.added[1].primitive, Primitive::Quad { .. }));
    assert!(matches!(
        delta.added[2].primitive,
        Primitive::RoundedBorder { .. }
    ));
    // Rounded masks are deliberately deferred: the current render-pass API
    // supports rectangular scissors only, so no rounded clip metadata is
    // advertised to child or external draws.
    assert!(delta.added.iter().all(|item| item.clips.is_empty()));
    assert_eq!(delta.added[0].paint_order, 0);
    assert_eq!(delta.added[1].paint_order, 1);
    assert_eq!(delta.added[2].paint_order, 2);
}

#[test]
fn should_preserve_layout_and_one_child_contract_for_decorated_box() {
    // Arrange
    let mut rt = Runtime::new();
    rt.set_root(
        DecoratedBox::new(BoxDecoration::new().try_color(Color::RED).unwrap())
            .child(SizedBox::new(Size::new(40.0, 24.0))),
    );

    // Act
    rt.update(Instant::now());

    // Assert
    let root = rt.arena().get(rt.root_id().unwrap()).unwrap();
    assert_eq!(root.children().len(), 1);
    assert_eq!(root.layout_rect().unwrap().size(), Size::new(40.0, 24.0));
    assert_eq!(
        rt.arena()
            .get(root.children()[0])
            .unwrap()
            .layout_rect()
            .unwrap()
            .size(),
        Size::new(40.0, 24.0)
    );
}

#[test]
fn should_emit_no_decoration_fill_when_color_is_absent() {
    // Arrange
    let mut rt = Runtime::new();
    rt.set_root(
        DecoratedBox::new(BoxDecoration::new())
            .child(SizedBox::new(Size::new(12.0, 12.0)).color(Color::GREEN)),
    );

    // Act
    rt.update(Instant::now());

    // Assert
    let delta = rt.pending_delta().unwrap();
    assert_eq!(delta.added.len(), 1);
    assert!(matches!(delta.added[0].primitive, Primitive::Quad { .. }));
}

// ── Align widget at runtime ─────────────────────────────────────────────────

#[test]
fn align_center_integration() {
    use harbor_widget::layout::Alignment;
    use harbor_widget::widgets::align::Align;

    let mut rt = Runtime::new();
    rt.set_root(Align::new(Alignment::Center).child(SizedBox::new(Size::new(50.0, 50.0))));
    let req = rt.update(Instant::now());
    assert!(req.request_redraw);

    let root_id = rt.root_id().unwrap();
    let arena = rt.arena();
    let root = arena.get(root_id).unwrap();
    // Align fills 800x600 (default viewport) in loose constraints
    assert_eq!(root.layout_rect().unwrap().size(), Size::new(800.0, 600.0));

    // Child should be centered
    let child = arena.get(root.children()[0]).unwrap();
    let child_rect = child.layout_rect().unwrap();
    assert_eq!(child_rect.size(), Size::new(50.0, 50.0));
    assert_eq!(child_rect.min, Point::new(375.0, 275.0)); // (800-50)/2, (600-50)/2
}

// ── Row at runtime with cross-axis alignment ────────────────────────────────

#[test]
fn row_cross_axis_center_integration() {
    use harbor_widget::layout::Alignment;
    use harbor_widget::widgets::row::Row;

    let mut rt = Runtime::new();
    rt.set_root(
        Row::new()
            .cross_axis_alignment(Alignment::Center)
            .child(SizedBox::new(Size::new(50.0, 20.0)))
            .child(SizedBox::new(Size::new(50.0, 100.0))),
    );
    let req = rt.update(Instant::now());
    assert!(req.request_redraw);

    let root_id = rt.root_id().unwrap();
    let arena = rt.arena();
    let root = arena.get(root_id).unwrap();
    assert_eq!(root.layout_rect().unwrap().size(), Size::new(100.0, 100.0));

    // First child (20 tall) centered in 100
    let child0 = arena.get(root.children()[0]).unwrap();
    assert_eq!(child0.layout_rect().unwrap().min, Point::new(0.0, 40.0));

    // Second child (100 tall) at y=0
    let child1 = arena.get(root.children()[1]).unwrap();
    assert_eq!(child1.layout_rect().unwrap().min, Point::new(50.0, 0.0));
}

// ── Stack with background at runtime ─────────────────────────────────────────

#[test]
fn stack_with_background_paint_count() {
    use harbor_widget::scene::primitive::Color;
    use harbor_widget::widgets::stack::Stack;

    let mut rt = Runtime::new();
    rt.set_root(
        Stack::new()
            .background(Color::BLACK)
            .child(SizedBox::new(Size::new(80.0, 80.0)).color(Color::RED))
            .child(SizedBox::new(Size::new(60.0, 60.0)).color(Color::GREEN)),
    );
    rt.update(Instant::now());

    let delta = rt.pending_delta().unwrap();
    assert_eq!(
        delta.added.len(),
        3,
        "an unchanged repaint must retain the initial additions until encode"
    );
    assert!(delta.removed.is_empty());
    assert!(delta.modified.is_empty());
}
