//! Fiber tree storage, reconciliation, layout, and paint.

mod arena;
mod layout;
mod paint;
mod reconcile;

pub use arena::{DirtyFlags, Fiber, FiberArena, FiberId};

pub(crate) use layout::layout_fiber;
pub(crate) use paint::paint_fiber;
pub(crate) use reconcile::{reconcile_children_with_externals, unmount_fiber};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{BoxConstraints, Point, Size};
    use crate::signal::{PENDING_DIRTY, Signal};
    use crate::view::{AnyView, Component, Key, PaintPhase, View};
    pub(crate) use reconcile::{create_fiber_from_view, reconcile_children, unmount_fiber};
    use std::{
        any::TypeId,
        cell::{Cell, RefCell},
        rc::Rc,
    };

    /// Helper: returns None for root fibers that have no parent.
    fn no_parent() -> Option<FiberId> {
        None
    }

    fn clear_dirty_queue() {
        PENDING_DIRTY.with(|q| q.borrow_mut().clear());
    }

    fn layout_fiber(
        arena: &mut FiberArena,
        id: FiberId,
        constraints: BoxConstraints,
        origin: Point,
    ) {
        super::layout_fiber(
            arena,
            id,
            constraints,
            origin,
            &crate::runtime::DEFAULT_TEXT_METRICS,
        );
    }

    fn paint_fiber(
        arena: &mut FiberArena,
        id: FiberId,
        base_order: u32,
        next_scene_item_id: &mut u64,
    ) -> Vec<crate::scene::SceneItem> {
        super::paint_fiber(
            arena,
            id,
            base_order,
            next_scene_item_id,
            &crate::runtime::DEFAULT_TEXT_METRICS,
        )
    }

    // ── DirtyFlags ─────────────────────────────────────────────────────

    #[test]
    fn dirty_flags_operations() {
        let mut flags = DirtyFlags::NONE;
        assert!(flags.is_empty());
        assert!(!flags.contains(DirtyFlags::BUILD_DIRTY));

        flags.insert(DirtyFlags::BUILD_DIRTY);
        assert!(flags.contains(DirtyFlags::BUILD_DIRTY));
        assert!(!flags.contains(DirtyFlags::LAYOUT_DIRTY));
        assert!(!flags.is_empty());

        flags.insert(DirtyFlags::LAYOUT_DIRTY);
        assert!(flags.contains(DirtyFlags::BUILD_DIRTY));
        assert!(flags.contains(DirtyFlags::LAYOUT_DIRTY));

        flags.remove(DirtyFlags::BUILD_DIRTY);
        assert!(!flags.contains(DirtyFlags::BUILD_DIRTY));
        assert!(flags.contains(DirtyFlags::LAYOUT_DIRTY));

        flags.remove(DirtyFlags::LAYOUT_DIRTY);
        assert!(flags.is_empty());
        assert_eq!(flags.bits(), 0);
    }

    #[test]
    fn dirty_flags_combined() {
        let mut combined = DirtyFlags::BUILD_DIRTY;
        combined.insert(DirtyFlags::LAYOUT_DIRTY);
        combined.insert(DirtyFlags::PAINT_DIRTY);
        assert_eq!(combined.bits(), 0b0111);
    }

    #[test]
    fn dirty_flags_all_set() {
        let mut flags = DirtyFlags::NONE;
        flags.insert(DirtyFlags::BUILD_DIRTY);
        flags.insert(DirtyFlags::LAYOUT_DIRTY);
        flags.insert(DirtyFlags::PAINT_DIRTY);
        flags.insert(DirtyFlags::HIT_TEST_DIRTY);
        assert_eq!(flags.bits(), 0b1111);
        assert!(flags.contains(DirtyFlags::BUILD_DIRTY));
        assert!(flags.contains(DirtyFlags::LAYOUT_DIRTY));
        assert!(flags.contains(DirtyFlags::PAINT_DIRTY));
        assert!(flags.contains(DirtyFlags::HIT_TEST_DIRTY));
        // Contains should pass for subsets
        assert!(flags.contains(DirtyFlags::BUILD_DIRTY));
        assert!(!DirtyFlags::BUILD_DIRTY.contains(DirtyFlags::LAYOUT_DIRTY));
    }

    #[test]
    fn dirty_flags_is_empty_after_remove_all() {
        let mut flags = DirtyFlags::NONE;
        flags.insert(DirtyFlags::BUILD_DIRTY);
        flags.insert(DirtyFlags::LAYOUT_DIRTY);
        assert!(!flags.is_empty());
        flags.remove(DirtyFlags::BUILD_DIRTY);
        flags.remove(DirtyFlags::LAYOUT_DIRTY);
        assert!(flags.is_empty());
    }

    // ── FiberArena ──────────────────────────────────────────────────────

    fn dummy_fiber() -> Fiber {
        Fiber::new(None, TypeId::of::<()>(), None)
    }

    #[test]
    fn arena_insert_and_get() {
        let mut arena = FiberArena::new();
        let fiber = dummy_fiber();
        let id = arena.insert(fiber);
        assert!(arena.contains(id));
        assert!(arena.get(id).is_some());
        assert_eq!(arena.get(id).unwrap().id, Some(id));
    }

    #[test]
    fn arena_stale_id_after_remove() {
        let mut arena = FiberArena::new();
        let fiber = dummy_fiber();
        let id = arena.insert(fiber);
        assert!(arena.contains(id));

        let removed = arena.remove(id);
        assert!(removed.is_some());

        // Stale access returns None due to generation mismatch
        assert!(!arena.contains(id));
        assert!(arena.get(id).is_none());
    }

    #[test]
    fn arena_remove_parent_does_not_implicitly_remove_children() {
        let mut arena = FiberArena::new();

        let child = dummy_fiber();
        let child_id = arena.insert(child);

        let mut parent = dummy_fiber();
        parent.children.push(child_id);
        let parent_id = arena.insert(parent);
        arena.get_mut(child_id).unwrap().parent = Some(parent_id);

        arena.remove(parent_id);
        // Child still exists -- caller is responsible for recursive cleanup
        assert!(arena.contains(child_id));
    }

    // ── Reconciliation ──────────────────────────────────────────────────

    /// A simple test AnyView that holds a name for identification.
    #[allow(dead_code)]
    struct TestView(String, Vec<View>);

    impl AnyView for TestView {
        fn key(&self) -> Option<&Key> {
            None
        }
        fn widget_type(&self) -> TypeId {
            TypeId::of::<Self>()
        }
        fn intrinsic_size(&self, c: BoxConstraints, _metrics: &crate::text::TextMetrics) -> Size {
            c.constrain(Size::new(10.0, 10.0))
        }
    }

    #[derive(Clone)]
    struct PhasePaintView {
        before: Vec<crate::scene::primitive::Color>,
        after: Vec<crate::scene::primitive::Color>,
    }

    impl PhasePaintView {
        fn before(color: crate::scene::primitive::Color) -> Self {
            Self {
                before: vec![color],
                after: Vec::new(),
            }
        }
    }

    impl AnyView for PhasePaintView {
        fn key(&self) -> Option<&Key> {
            None
        }

        fn widget_type(&self) -> TypeId {
            TypeId::of::<Self>()
        }

        fn intrinsic_size(
            &self,
            constraints: BoxConstraints,
            _metrics: &crate::text::TextMetrics,
        ) -> Size {
            constraints.constrain(Size::new(10.0, 10.0))
        }

        fn paint_primitives_for_phase(
            &self,
            phase: PaintPhase,
            rect: crate::layout::Rect,
            _metrics: &crate::text::TextMetrics,
        ) -> Vec<crate::scene::primitive::Primitive> {
            let colors = match phase {
                PaintPhase::BeforeChildren => &self.before,
                PaintPhase::AfterChildren => &self.after,
            };
            colors
                .iter()
                .copied()
                .map(|color| crate::scene::primitive::Primitive::Quad {
                    rect,
                    color,
                    corner_radius: 0.0,
                })
                .collect()
        }
    }

    #[derive(Clone)]
    struct ConstraintRecordingView {
        layout_constraints: Rc<RefCell<Vec<BoxConstraints>>>,
    }

    impl Component for ConstraintRecordingView {
        fn build(&self, _cx: &mut crate::view::BuildCx) -> View {
            View::new(self.clone(), vec![], None)
        }
    }

    impl AnyView for ConstraintRecordingView {
        fn key(&self) -> Option<&Key> {
            None
        }

        fn widget_type(&self) -> TypeId {
            TypeId::of::<Self>()
        }

        fn intrinsic_size(
            &self,
            constraints: BoxConstraints,
            _metrics: &crate::text::TextMetrics,
        ) -> Size {
            constraints.max
        }

        fn layout_children(
            &self,
            constraints: BoxConstraints,
            _child_sizes: &[Size],
            _metrics: &crate::text::TextMetrics,
        ) -> (Size, Vec<Point>) {
            self.layout_constraints.borrow_mut().push(constraints);
            (constraints.max, vec![])
        }
    }

    #[derive(Clone)]
    struct LayoutPassCountingView {
        layout_calls: Rc<Cell<usize>>,
        intrinsic_calls: Rc<Cell<usize>>,
        child: Option<View>,
    }

    impl LayoutPassCountingView {
        fn new(layout_calls: Rc<Cell<usize>>, intrinsic_calls: Rc<Cell<usize>>) -> Self {
            Self {
                layout_calls,
                intrinsic_calls,
                child: None,
            }
        }

        fn child(mut self, child: impl Component + 'static) -> Self {
            self.child = Some(View::deferred(child));
            self
        }
    }

    impl Component for LayoutPassCountingView {
        fn build(&self, _cx: &mut crate::view::BuildCx) -> View {
            View::new(self.clone(), self.child.iter().cloned().collect(), None)
        }
    }

    impl AnyView for LayoutPassCountingView {
        fn key(&self) -> Option<&Key> {
            None
        }

        fn widget_type(&self) -> TypeId {
            TypeId::of::<Self>()
        }

        fn intrinsic_size(
            &self,
            constraints: BoxConstraints,
            _metrics: &crate::text::TextMetrics,
        ) -> Size {
            self.intrinsic_calls.set(self.intrinsic_calls.get() + 1);
            constraints.max
        }

        fn layout_children(
            &self,
            constraints: BoxConstraints,
            child_sizes: &[Size],
            _metrics: &crate::text::TextMetrics,
        ) -> (Size, Vec<Point>) {
            self.layout_calls.set(self.layout_calls.get() + 1);
            (constraints.max, vec![Point::ZERO; child_sizes.len()])
        }
    }

    fn test_view(name: &str) -> View {
        View::new(TestView(name.to_string(), vec![]), vec![], None)
    }

    fn test_view_with_children(name: &str, children: Vec<View>) -> View {
        View::new(TestView(name.to_string(), vec![]), children, None)
    }

    /// A second view type for type-change tests.
    struct OtherView;
    impl AnyView for OtherView {
        fn key(&self) -> Option<&Key> {
            None
        }
        fn widget_type(&self) -> TypeId {
            TypeId::of::<Self>()
        }
        fn intrinsic_size(&self, c: BoxConstraints, _metrics: &crate::text::TextMetrics) -> Size {
            c.constrain(Size::new(20.0, 20.0))
        }
    }

    fn other_view() -> View {
        View::new(OtherView, vec![], None)
    }

    #[derive(Clone)]
    struct KeyedTestView {
        key: Key,
    }

    impl AnyView for KeyedTestView {
        fn key(&self) -> Option<&Key> {
            Some(&self.key)
        }
        fn widget_type(&self) -> TypeId {
            TypeId::of::<Self>()
        }
        fn intrinsic_size(&self, c: BoxConstraints, _metrics: &crate::text::TextMetrics) -> Size {
            c.constrain(Size::new(20.0, 20.0))
        }
    }

    fn keyed_test_view(_name: &str, key: &str) -> View {
        View::new(KeyedTestView { key: Key::new(key) }, vec![], None)
    }

    #[test]
    fn reconcile_static_tree_all_reused() {
        let mut arena = FiberArena::new();

        // First build: create parent fiber with one child
        let parent_view = test_view_with_children("parent", vec![test_view("child")]);
        let parent_id = create_fiber_from_view(&mut arena, no_parent(), parent_view);

        let old_children = arena.get(parent_id).unwrap().children.clone();
        assert_eq!(old_children.len(), 1);

        // Second build: same View structure -- fibers should be reused
        let new_parent = test_view_with_children("parent", vec![test_view("child")]);
        let new_children =
            reconcile_children(&mut arena, parent_id, &old_children, new_parent.children);

        assert_eq!(new_children.len(), 1);
        // The child fiber should be the same (reused)
        assert_eq!(new_children[0], old_children[0]);
    }

    #[test]
    fn reconcile_type_change_unmounts_and_creates() {
        let mut arena = FiberArena::new();

        let parent_view = test_view_with_children("parent", vec![test_view("child")]);
        let parent_id = create_fiber_from_view(&mut arena, no_parent(), parent_view);

        let old_children = arena.get(parent_id).unwrap().children.clone();
        let old_child = old_children[0];

        // Replace with a different widget type
        let new_view = test_view_with_children("parent", vec![other_view()]);
        let new_children =
            reconcile_children(&mut arena, parent_id, &old_children, new_view.children);

        assert_eq!(new_children.len(), 1);
        assert_ne!(new_children[0], old_child); // New fiber created
        assert!(!arena.contains(old_child)); // Old fiber unmounted
    }

    #[test]
    fn reconcile_key_change_unmounts_and_creates() {
        let mut arena = FiberArena::new();

        let parent_view = test_view_with_children("parent", vec![keyed_test_view("child", "old")]);
        let parent_id = create_fiber_from_view(&mut arena, no_parent(), parent_view);

        let old_children = arena.get(parent_id).unwrap().children.clone();
        let old_child = old_children[0];

        let new_view = test_view_with_children("parent", vec![keyed_test_view("child", "new")]);
        let new_children =
            reconcile_children(&mut arena, parent_id, &old_children, new_view.children);

        assert_eq!(new_children.len(), 1);
        assert_ne!(new_children[0], old_child);
        assert!(!arena.contains(old_child));
    }

    #[test]
    fn reconcile_child_list_grows() {
        let mut arena = FiberArena::new();

        let parent_view = test_view_with_children("parent", vec![test_view("a"), test_view("b")]);
        let parent_id = create_fiber_from_view(&mut arena, no_parent(), parent_view);

        let old_children = arena.get(parent_id).unwrap().children.clone();
        assert_eq!(old_children.len(), 2);

        // Grow: 2 -> 4 children (a, b, c, d)
        let new_view = test_view_with_children(
            "parent",
            vec![
                test_view("a"),
                test_view("b"),
                test_view("c"),
                test_view("d"),
            ],
        );
        let new_children =
            reconcile_children(&mut arena, parent_id, &old_children, new_view.children);

        assert_eq!(new_children.len(), 4);
        // First two should be reused
        assert_eq!(new_children[0], old_children[0]);
        assert_eq!(new_children[1], old_children[1]);
    }

    #[test]
    fn reconcile_child_list_shrinks() {
        let mut arena = FiberArena::new();

        let parent_view = test_view_with_children(
            "parent",
            vec![test_view("a"), test_view("b"), test_view("c")],
        );
        let parent_id = create_fiber_from_view(&mut arena, no_parent(), parent_view);

        let old_children = arena.get(parent_id).unwrap().children.clone();
        assert_eq!(old_children.len(), 3);

        // Shrink: 3 -> 2 children
        let new_view = test_view_with_children("parent", vec![test_view("a"), test_view("b")]);
        let new_children =
            reconcile_children(&mut arena, parent_id, &old_children, new_view.children);

        assert_eq!(new_children.len(), 2);
        // First two reused
        assert_eq!(new_children[0], old_children[0]);
        assert_eq!(new_children[1], old_children[1]);
        // Third should be unmounted
        assert!(!arena.contains(old_children[2]));
    }

    #[test]
    fn reconcile_empty_to_empty() {
        let mut arena = FiberArena::new();
        let parent = dummy_fiber();
        let parent_id = arena.insert(parent);

        // Both old children and new views are empty
        let new_children = reconcile_children(&mut arena, parent_id, &[], vec![]);
        assert!(new_children.is_empty());
    }

    #[test]
    fn unmount_clears_subscriptions() {
        let mut arena = FiberArena::new();

        // Create a fiber to get a valid FiberId for testing subscriptions
        let dummy = dummy_fiber();
        let fid = arena.insert(dummy);

        // Create a fiber with a Signal hook
        let signal = Signal::new(42u32);
        signal.subscribe(fid);

        let mut fiber = dummy_fiber();
        fiber.hooks.push(Box::new(signal.clone()));
        let fiber_id = arena.insert(fiber);

        // Signal has the fiber as subscriber
        signal.set(100);
        // (just checking no panic)

        unmount_fiber(&mut arena, fiber_id);
        assert!(!arena.contains(fiber_id));

        // After unmount, the fiber is unsubscribed
        // (Signal::set would no longer try to mark fiber_id dirty via mark_dirty)
    }

    #[test]
    fn unmount_recursively_removes_children_and_unsubscribes_hooks() {
        clear_dirty_queue();

        let mut arena = FiberArena::new();
        let signal = Signal::new(0u32);

        let mut child = dummy_fiber();
        child.hooks.push(Box::new(signal.clone()));
        let child_id = arena.insert(child);
        signal.subscribe(child_id);

        let mut parent = dummy_fiber();
        parent.children.push(child_id);
        let parent_id = arena.insert(parent);
        arena.get_mut(child_id).unwrap().parent = Some(parent_id);

        unmount_fiber(&mut arena, parent_id);

        assert!(!arena.contains(parent_id));
        assert!(!arena.contains(child_id));

        signal.set(1);
        let dirty = PENDING_DIRTY.with(|q| {
            q.borrow()
                .get(&crate::signal::DEFAULT_RUNTIME_ID)
                .cloned()
                .unwrap_or_default()
        });
        assert!(dirty.is_empty());
        clear_dirty_queue();
    }

    // ── Layout ──────────────────────────────────────────────────────────

    #[test]
    fn layout_sets_rect_on_fiber() {
        let mut arena = FiberArena::new();

        use crate::view::Component;
        use crate::widgets::sized_box::SizedBox;
        let sb = SizedBox::new(Size::new(100.0, 50.0));
        let mut cx = crate::view::BuildCx::stub();
        let view = sb.build(&mut cx);
        let fiber_id = create_fiber_from_view(&mut arena, no_parent(), view);

        layout_fiber(
            &mut arena,
            fiber_id,
            BoxConstraints::loose(Size::new(800.0, 600.0)),
            Point::new(10.0, 20.0),
        );

        let fiber = arena.get(fiber_id).unwrap();
        assert!(fiber.layout_rect.is_some());
        let rect = fiber.layout_rect.unwrap();
        assert_eq!(rect.min, Point::new(10.0, 20.0));
        assert_eq!(rect.size(), Size::new(100.0, 50.0));
    }

    // The production Fiber stores type-erased views in an Arc even though
    // widgets are UI-thread-owned; this test replaces that view directly.
    #[allow(clippy::arc_with_non_send_sync)]
    #[test]
    fn paint_fiber_collects_primitives() {
        let mut arena = FiberArena::new();

        use crate::scene::primitive::Color;
        use crate::view::Component;
        use crate::widgets::sized_box::SizedBox;
        let sb = SizedBox::new(Size::new(100.0, 50.0)).color(Color::RED);
        let mut cx = crate::view::BuildCx::stub();
        let view = sb.build(&mut cx);
        let fiber_id = create_fiber_from_view(&mut arena, no_parent(), view);

        // Layout first so the fiber has a rect
        layout_fiber(
            &mut arena,
            fiber_id,
            BoxConstraints::loose(Size::new(800.0, 600.0)),
            Point::ZERO,
        );

        let mut next_scene_item_id = 1;
        let items = paint_fiber(&mut arena, fiber_id, 0, &mut next_scene_item_id);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].paint_order, 0);

        arena.get_mut(fiber_id).unwrap().view =
            Some(std::sync::Arc::new(SizedBox::new(Size::new(100.0, 50.0))));
        let items = paint_fiber(&mut arena, fiber_id, 0, &mut next_scene_item_id);
        assert!(items.is_empty());
        assert!(arena.get(fiber_id).unwrap().scene_item_ids.is_empty());
    }

    #[test]
    fn paint_fiber_orders_phase_primitives_and_retains_after_ids() {
        use crate::scene::primitive::{Color, Primitive};

        // Arrange: the parent emits before and after its child.
        let parent = PhasePaintView {
            before: vec![Color::RED],
            after: vec![Color::BLUE],
        };
        let child = PhasePaintView::before(Color::GREEN);
        let mut arena = FiberArena::new();
        let root_id = create_fiber_from_view(
            &mut arena,
            no_parent(),
            View::new(parent, vec![View::new(child, vec![], None)], None),
        );
        layout_fiber(
            &mut arena,
            root_id,
            BoxConstraints::loose(Size::new(100.0, 100.0)),
            Point::ZERO,
        );

        // Act: paint once, then add one descendant primitive.
        let mut next_scene_item_id = 1;
        let first = paint_fiber(&mut arena, root_id, 0, &mut next_scene_item_id);
        let child_id = arena.get(root_id).unwrap().children[0];
        arena.get_mut(child_id).unwrap().view = Some(std::sync::Arc::new(PhasePaintView {
            before: vec![Color::GREEN, Color::BLACK],
            after: Vec::new(),
        }));
        let second = paint_fiber(&mut arena, root_id, 0, &mut next_scene_item_id);

        // Assert: before → descendant → after; after identity remains stable.
        assert_eq!(first.len(), 3);
        assert_eq!(
            first
                .iter()
                .map(|item| match item.primitive {
                    Primitive::Quad { color, .. } => color,
                    _ => unreachable!("phase test emits only quads"),
                })
                .collect::<Vec<_>>(),
            vec![Color::RED, Color::GREEN, Color::BLUE]
        );
        assert_eq!(first[2].id, second[3].id);
        assert_eq!(
            second
                .iter()
                .map(|item| item.paint_order)
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
        assert_eq!(
            second
                .iter()
                .map(|item| match item.primitive {
                    Primitive::Quad { color, .. } => color,
                    _ => unreachable!("phase test emits only quads"),
                })
                .collect::<Vec<_>>(),
            vec![Color::RED, Color::GREEN, Color::BLACK, Color::BLUE]
        );
    }

    #[derive(Clone)]
    struct ClipAncestor(crate::scene::clip::RoundedClip);

    impl AnyView for ClipAncestor {
        fn key(&self) -> Option<&Key> {
            None
        }

        fn widget_type(&self) -> TypeId {
            TypeId::of::<Self>()
        }

        fn intrinsic_size(
            &self,
            constraints: BoxConstraints,
            _metrics: &crate::text::TextMetrics,
        ) -> Size {
            constraints.constrain(Size::new(20.0, 20.0))
        }

        fn descendant_clip(
            &self,
            _rect: crate::layout::Rect,
        ) -> Option<crate::scene::clip::RoundedClip> {
            Some(self.0.clone())
        }
    }

    #[test]
    fn paint_fiber_attaches_only_inherited_clips_to_descendant_items() {
        let clip = crate::scene::clip::RoundedClip::new(
            crate::layout::Rect::from_min_size(Point::ZERO, Size::new(20.0, 20.0)),
            crate::decoration::BorderRadius::all(4.0).unwrap(),
            crate::decoration::ClipBehavior::HardEdge,
        )
        .unwrap();
        let child = PhasePaintView::before(crate::scene::primitive::Color::RED);
        let mut arena = FiberArena::new();
        let root = create_fiber_from_view(
            &mut arena,
            no_parent(),
            View::new(
                ClipAncestor(clip.clone()),
                vec![View::new(child, vec![], None)],
                None,
            ),
        );
        layout_fiber(
            &mut arena,
            root,
            BoxConstraints::loose(Size::new(20.0, 20.0)),
            Point::ZERO,
        );

        let items = paint_fiber(&mut arena, root, 0, &mut 1);

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].clips, vec![clip]);
    }

    #[test]
    fn paint_fiber_nested_column_correct_paint_order() {
        use crate::scene::primitive::Color;
        use crate::view::Component;
        use crate::widgets::column::Column;
        use crate::widgets::sized_box::SizedBox;

        let mut arena = FiberArena::new();

        // Column with 3 colored SizedBox children
        let column = Column::new()
            .child(SizedBox::new(Size::new(50.0, 30.0)).color(Color::RED))
            .child(SizedBox::new(Size::new(50.0, 30.0)).color(Color::GREEN))
            .child(SizedBox::new(Size::new(50.0, 30.0)).color(Color::BLUE));
        let mut cx = crate::view::BuildCx::stub();
        let view = column.build(&mut cx);
        let fiber_id = create_fiber_from_view(&mut arena, no_parent(), view);

        // Layout
        layout_fiber(
            &mut arena,
            fiber_id,
            BoxConstraints::loose(Size::new(800.0, 600.0)),
            Point::ZERO,
        );

        let items = paint_fiber(&mut arena, fiber_id, 0, &mut 1);
        // Each child has one prim (Quad), plus parent has 0 (no background)
        assert_eq!(items.len(), 3);
        // Paint order should be sequential: 0, 1, 2
        assert_eq!(items[0].paint_order, 0);
        assert_eq!(items[1].paint_order, 1);
        assert_eq!(items[2].paint_order, 2);
        // Check colors via primitive
        use crate::scene::primitive::Primitive;
        match &items[0].primitive {
            Primitive::Quad { color, .. } => assert_eq!(*color, Color::RED),
            _ => panic!("expected Quad"),
        }
        match &items[1].primitive {
            Primitive::Quad { color, .. } => assert_eq!(*color, Color::GREEN),
            _ => panic!("expected Quad"),
        }
        match &items[2].primitive {
            Primitive::Quad { color, .. } => assert_eq!(*color, Color::BLUE),
            _ => panic!("expected Quad"),
        }
    }

    #[test]
    fn paint_fiber_column_with_background_produces_paint_order() {
        use crate::scene::primitive::Color;
        use crate::view::Component;
        use crate::widgets::column::Column;
        use crate::widgets::sized_box::SizedBox;

        let mut arena = FiberArena::new();

        // Column with background + one child
        let column = Column::new()
            .background(Color::BLACK)
            .child(SizedBox::new(Size::new(50.0, 30.0)).color(Color::RED));
        let mut cx = crate::view::BuildCx::stub();
        let view = column.build(&mut cx);
        let fiber_id = create_fiber_from_view(&mut arena, no_parent(), view);

        layout_fiber(
            &mut arena,
            fiber_id,
            BoxConstraints::loose(Size::new(800.0, 600.0)),
            Point::ZERO,
        );

        let items = paint_fiber(&mut arena, fiber_id, 0, &mut 1);
        // Parent background (paint_order 0) + child quad (paint_order 1)
        assert_eq!(items.len(), 2);
        assert_eq!(
            items[0].paint_order, 0,
            "parent background should paint first"
        );
        assert_eq!(items[1].paint_order, 1, "child should paint after parent");
    }

    #[test]
    fn paint_fiber_row_with_nested_padding() {
        use crate::scene::primitive::Color;
        use crate::view::Component;
        use crate::widgets::padding::Padding;
        use crate::widgets::row::Row;
        use crate::widgets::sized_box::SizedBox;

        let mut arena = FiberArena::new();

        // Row containing a Padding containing a SizedBox
        let row = Row::new().child(
            Padding::new(8.0, 8.0, 8.0, 8.0)
                .background(Color::BLACK)
                .child(SizedBox::new(Size::new(32.0, 32.0)).color(Color::RED)),
        );
        let mut cx = crate::view::BuildCx::stub();
        let view = row.build(&mut cx);
        let fiber_id = create_fiber_from_view(&mut arena, no_parent(), view);

        layout_fiber(
            &mut arena,
            fiber_id,
            BoxConstraints::loose(Size::new(800.0, 600.0)),
            Point::ZERO,
        );

        let items = paint_fiber(&mut arena, fiber_id, 0, &mut 1);
        // Row has no background, Padding has background, SizedBox has color
        // Paint order: Row (0 prims), then Padding (1 prim), then SizedBox (1 prim) = 2
        assert_eq!(items.len(), 2, "padding bg + sized box color = 2 prims");
        // Both items should have incrementing paint orders
        assert_eq!(items[0].paint_order, 0);
        assert_eq!(items[1].paint_order, 1);
    }

    #[test]
    fn paint_fiber_stack_overlapping_children() {
        use crate::scene::primitive::Color;
        use crate::view::Component;
        use crate::widgets::sized_box::SizedBox;
        use crate::widgets::stack::Stack;

        let mut arena = FiberArena::new();

        let stack = Stack::new()
            .background(Color::BLACK)
            .child(SizedBox::new(Size::new(100.0, 100.0)).color(Color::RED))
            .child(SizedBox::new(Size::new(80.0, 80.0)).color(Color::GREEN));
        let mut cx = crate::view::BuildCx::stub();
        let view = stack.build(&mut cx);
        let fiber_id = create_fiber_from_view(&mut arena, no_parent(), view);

        layout_fiber(
            &mut arena,
            fiber_id,
            BoxConstraints::loose(Size::new(800.0, 600.0)),
            Point::ZERO,
        );

        let items = paint_fiber(&mut arena, fiber_id, 0, &mut 1);
        // Stack background + 2 children = 3
        assert_eq!(items.len(), 3);
        // Paint order: stack bg (0), first child (1), second child (2)
        assert_eq!(items[0].paint_order, 0);
        assert_eq!(items[1].paint_order, 1);
        assert_eq!(items[2].paint_order, 2);
    }

    #[test]
    fn should_measure_nested_padding_from_its_child() {
        use crate::widgets::padding::Padding;
        use crate::widgets::sized_box::SizedBox;

        // Arrange
        let mut arena = FiberArena::new();
        let mut cx = crate::view::BuildCx::stub();
        let root_id = create_fiber_from_view(
            &mut arena,
            no_parent(),
            Padding::all(4.0)
                .child(Padding::all(3.0).child(SizedBox::new(Size::new(20.0, 10.0))))
                .build(&mut cx),
        );

        // Act
        layout_fiber(
            &mut arena,
            root_id,
            BoxConstraints::loose(Size::new(100.0, 100.0)),
            Point::ZERO,
        );

        // Assert
        let inner_padding_id = arena.get(root_id).unwrap().children[0];
        let child_id = arena.get(inner_padding_id).unwrap().children[0];
        assert_eq!(
            arena.get(root_id).unwrap().layout_rect.unwrap().size(),
            Size::new(34.0, 24.0)
        );
        assert_eq!(
            arena
                .get(inner_padding_id)
                .unwrap()
                .layout_rect
                .unwrap()
                .min,
            Point::new(4.0, 4.0)
        );
        assert_eq!(
            arena
                .get(inner_padding_id)
                .unwrap()
                .layout_rect
                .unwrap()
                .size(),
            Size::new(26.0, 16.0)
        );
        assert_eq!(
            arena.get(child_id).unwrap().layout_rect.unwrap().min,
            Point::new(7.0, 7.0)
        );
        assert_eq!(
            arena.get(child_id).unwrap().layout_rect.unwrap().size(),
            Size::new(20.0, 10.0)
        );
    }

    #[test]
    fn should_allocate_terminal_content_inside_uniform_root_padding() {
        use crate::widgets::custom_paint::CustomPaint;
        use crate::widgets::padding::Padding;

        // Arrange
        let mut arena = FiberArena::new();
        let padding = Padding::all(16.0).child(CustomPaint::new(1));
        let mut cx = crate::view::BuildCx::stub();
        let root_id = create_fiber_from_view(&mut arena, no_parent(), padding.build(&mut cx));

        // Act
        layout_fiber(
            &mut arena,
            root_id,
            BoxConstraints::tight(Size::new(100.0, 80.0)),
            Point::ZERO,
        );

        // Assert
        let child_id = arena.get(root_id).unwrap().children[0];
        let root_rect = arena.get(root_id).unwrap().layout_rect.unwrap();
        let child_rect = arena.get(child_id).unwrap().layout_rect.unwrap();
        assert_eq!(root_rect.size(), Size::new(100.0, 80.0));
        assert_eq!(child_rect.min, Point::new(16.0, 16.0));
        assert_eq!(child_rect.size(), Size::new(68.0, 48.0));
    }

    #[test]
    fn should_propagate_deflated_constraints_when_padding_wraps_child() {
        use crate::widgets::padding::Padding;

        // Arrange
        let layout_constraints = Rc::new(RefCell::new(vec![]));
        let child = ConstraintRecordingView {
            layout_constraints: Rc::clone(&layout_constraints),
        };
        let mut arena = FiberArena::new();
        let mut cx = crate::view::BuildCx::stub();
        let root_id = create_fiber_from_view(
            &mut arena,
            no_parent(),
            Padding::all(16.0).child(child).build(&mut cx),
        );

        // Act
        layout_fiber(
            &mut arena,
            root_id,
            BoxConstraints::tight(Size::new(100.0, 80.0)),
            Point::ZERO,
        );

        // Assert
        assert_eq!(
            layout_constraints.borrow().as_slice(),
            &[BoxConstraints::tight(Size::new(68.0, 48.0))]
        );
    }

    #[test]
    fn should_measure_each_subtree_once_without_intrinsic_probes() {
        // Arrange
        let root_layout_calls = Rc::new(Cell::new(0));
        let root_intrinsic_calls = Rc::new(Cell::new(0));
        let child_layout_calls = Rc::new(Cell::new(0));
        let child_intrinsic_calls = Rc::new(Cell::new(0));
        let root = LayoutPassCountingView::new(
            Rc::clone(&root_layout_calls),
            Rc::clone(&root_intrinsic_calls),
        )
        .child(LayoutPassCountingView::new(
            Rc::clone(&child_layout_calls),
            Rc::clone(&child_intrinsic_calls),
        ));
        let mut arena = FiberArena::new();
        let mut cx = crate::view::BuildCx::stub();
        let root_id = create_fiber_from_view(&mut arena, no_parent(), root.build(&mut cx));

        // Act
        layout_fiber(
            &mut arena,
            root_id,
            BoxConstraints::tight(Size::new(100.0, 80.0)),
            Point::ZERO,
        );

        // Assert
        assert_eq!(root_layout_calls.get(), 1);
        assert_eq!(child_layout_calls.get(), 1);
        assert_eq!(root_intrinsic_calls.get(), 0);
        assert_eq!(child_intrinsic_calls.get(), 0);
    }

    #[test]
    fn should_allocate_zero_sized_child_when_root_is_smaller_than_padding_insets() {
        use crate::widgets::custom_paint::CustomPaint;
        use crate::widgets::padding::Padding;

        // Arrange
        let mut arena = FiberArena::new();
        let mut cx = crate::view::BuildCx::stub();
        let root_id = create_fiber_from_view(
            &mut arena,
            no_parent(),
            Padding::all(16.0).child(CustomPaint::new(1)).build(&mut cx),
        );

        // Act
        layout_fiber(
            &mut arena,
            root_id,
            BoxConstraints::tight(Size::new(30.0, 20.0)),
            Point::ZERO,
        );

        // Assert
        let child_id = arena.get(root_id).unwrap().children[0];
        assert_eq!(
            arena.get(child_id).unwrap().layout_rect.unwrap().size(),
            Size::ZERO
        );
    }

    #[test]
    fn should_retain_last_staged_child_when_padding_child_is_chained() {
        use crate::widgets::padding::Padding;
        use crate::widgets::sized_box::SizedBox;

        // Arrange
        let mut arena = FiberArena::new();
        let mut cx = crate::view::BuildCx::stub();
        let root_id = create_fiber_from_view(
            &mut arena,
            no_parent(),
            Padding::all(4.0)
                .child(SizedBox::new(Size::new(10.0, 10.0)))
                .child(SizedBox::new(Size::new(20.0, 20.0)))
                .build(&mut cx),
        );

        // Act
        layout_fiber(
            &mut arena,
            root_id,
            BoxConstraints::loose(Size::new(100.0, 100.0)),
            Point::ZERO,
        );

        // Assert
        let child_ids = arena.get(root_id).unwrap().children.clone();
        assert_eq!(child_ids.len(), 1);
        assert_eq!(
            arena.get(child_ids[0]).unwrap().layout_rect.unwrap().size(),
            Size::new(20.0, 20.0)
        );
    }

    #[test]
    fn layout_fiber_with_no_children() {
        let mut arena = FiberArena::new();

        use crate::view::Component;
        use crate::widgets::sized_box::SizedBox;
        let sb = SizedBox::new(Size::new(100.0, 50.0));
        let mut cx = crate::view::BuildCx::stub();
        let view = sb.build(&mut cx);
        let fiber_id = create_fiber_from_view(&mut arena, no_parent(), view);

        // This tests that layout doesn't panic when children list is empty
        layout_fiber(
            &mut arena,
            fiber_id,
            BoxConstraints::loose(Size::new(800.0, 600.0)),
            Point::ZERO,
        );

        let fiber = arena.get(fiber_id).unwrap();
        assert!(fiber.layout_rect.is_some());
    }

    #[test]
    fn layout_fiber_with_dead_id() {
        let mut arena = FiberArena::new();
        let fiber = dummy_fiber();
        let id = arena.insert(fiber);
        arena.remove(id);

        // Layout on dead fiber should be a no-op (no panic)
        layout_fiber(
            &mut arena,
            id,
            BoxConstraints::loose(Size::new(800.0, 600.0)),
            Point::ZERO,
        );
        // No panic = pass
    }

    #[test]
    fn paint_fiber_with_dead_id() {
        let mut arena = FiberArena::new();
        let fiber = dummy_fiber();
        let id = arena.insert(fiber);
        arena.remove(id);

        let items = paint_fiber(&mut arena, id, 0, &mut 1);
        assert!(items.is_empty());
    }

    #[test]
    fn paint_fiber_respects_base_order() {
        use crate::scene::primitive::Color;
        use crate::view::Component;
        use crate::widgets::sized_box::SizedBox;

        let mut arena = FiberArena::new();

        let sb = SizedBox::new(Size::new(100.0, 50.0)).color(Color::RED);
        let mut cx = crate::view::BuildCx::stub();
        let view = sb.build(&mut cx);
        let fiber_id = create_fiber_from_view(&mut arena, no_parent(), view);

        layout_fiber(
            &mut arena,
            fiber_id,
            BoxConstraints::loose(Size::new(800.0, 600.0)),
            Point::ZERO,
        );

        // Paint with non-zero base_order
        let items = paint_fiber(&mut arena, fiber_id, 10, &mut 1);
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].paint_order, 10,
            "paint order should start at base_order"
        );
    }
}
