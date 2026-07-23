use harbor_widget::layout::{Point, Size};
use harbor_widget::runtime::Runtime;
use harbor_widget::signal::Signal;
use harbor_widget::view::{BuildCx, Component, Key, SizedBox, SizedBoxState, View};
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
        // Wrap in a container view (using SizedBoxState as the container)
        View::new(
            SizedBoxState {
                size: Size::new(200.0, 200.0),
            },
            vec![inner],
            None,
        )
    }
}

// ── Integration Tests ────────────────────────────────────────────────────────

#[test]
fn sized_box_layout_integration() {
    let mut rt = Runtime::new();
    rt.set_root(SizedBox::new(Size::new(100.0, 50.0)));

    let req = rt.update(Instant::now());
    assert!(req.needs_redraw, "first build should request redraw");

    // Verify root fiber has layout_rect with size 100x50
    let root_id = rt.root_id().unwrap();
    let arena = rt.arena();
    let fiber = arena.get(root_id).unwrap();
    assert!(fiber.layout_rect.is_some(), "root should have layout_rect");
    let rect = fiber.layout_rect.unwrap();
    assert_eq!(rect.min, Point::ZERO);
    assert_eq!(rect.size(), Size::new(100.0, 50.0));
}

#[test]
fn second_update_without_changes_no_redraw() {
    let mut rt = Runtime::new();
    rt.set_root(SizedBox::new(Size::new(100.0, 50.0)));

    let req1 = rt.update(Instant::now());
    assert!(req1.needs_redraw);

    let req2 = rt.update(Instant::now());
    assert!(
        !req2.needs_redraw,
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
    assert!(req.needs_redraw);

    let fiber = rt.arena().get(second_root).unwrap();
    assert_eq!(fiber.layout_rect.unwrap().size(), Size::new(200.0, 100.0));
}

#[test]
fn nested_sized_box_produces_correct_layout_rects() {
    let mut rt = Runtime::new();
    rt.set_root(Wrapper {
        inner_size: Size::new(50.0, 25.0),
    });

    let req = rt.update(Instant::now());
    assert!(req.needs_redraw);

    let root_id = rt.root_id().unwrap();
    let arena = rt.arena();
    let root = arena.get(root_id).unwrap();

    // Root (Wrapper container) should have 200x200
    assert_eq!(root.layout_rect.unwrap().size(), Size::new(200.0, 200.0));

    // Child (inner SizedBox) should have 50x25
    assert_eq!(root.children.len(), 1, "root should have one child");
    let child = arena.get(root.children[0]).unwrap();
    assert_eq!(child.layout_rect.unwrap().size(), Size::new(50.0, 25.0));
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
    assert!(req1.needs_redraw);

    let state = shared_state.borrow().as_ref().unwrap().clone();
    state.set(1);

    let req2 = rt.update(Instant::now());
    assert!(req2.needs_redraw, "signal change should trigger redraw");

    let req3 = rt.update(Instant::now());
    assert!(
        !req3.needs_redraw,
        "after processing the signal change, the tree should be clean"
    );
}

#[test]
fn view_key_preserved() {
    let view = View::new(
        SizedBoxState {
            size: Size::new(10.0, 10.0),
        },
        vec![],
        Some(Key::new("test_key")),
    );
    assert_eq!(view.key(), Some(&Key::new("test_key")));
    assert_eq!(view.widget_type(), std::any::TypeId::of::<SizedBoxState>());
}
