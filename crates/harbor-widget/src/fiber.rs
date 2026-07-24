use crate::layout::{BoxConstraints, Point, Rect, Size};
use crate::scene::SceneItem;
use crate::signal::Hook;
use crate::view::{AnyView, Key, View};
use slotmap::SlotMap;
use std::any::TypeId;
use std::sync::Arc;

// ── FiberId ──────────────────────────────────────────────────────────────────

slotmap::new_key_type! {
    /// A generation-checked handle to a Fiber in the arena.
    pub struct FiberId;
}

// ── DirtyFlags ───────────────────────────────────────────────────────────────

/// Bitflags for incremental update targeting.
///
/// Hand-rolled bit operations on a `u8` -- no `bitflags` crate dependency.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct DirtyFlags(u8);

impl DirtyFlags {
    pub const NONE: Self = DirtyFlags(0);
    pub const BUILD_DIRTY: Self = DirtyFlags(0b0001);
    pub const LAYOUT_DIRTY: Self = DirtyFlags(0b0010);
    pub const PAINT_DIRTY: Self = DirtyFlags(0b0100);
    pub const HIT_TEST_DIRTY: Self = DirtyFlags(0b1000);

    /// Returns true if every flag in `other` is set within `self`.
    #[must_use]
    pub fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Sets the given flags.
    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }

    /// Clears the given flags.
    pub fn remove(&mut self, other: Self) {
        self.0 &= !other.0;
    }

    /// Returns true if no flags are set.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Returns the raw u8 bits.
    #[must_use]
    pub fn bits(self) -> u8 {
        self.0
    }
}

// ── Fiber ────────────────────────────────────────────────────────────────────

/// A long-lived component instance with hooks, state, and children.
pub struct Fiber {
    pub id: Option<FiberId>,
    pub key: Option<Key>,
    pub widget_type: TypeId,
    #[allow(private_interfaces)]
    pub hooks: Vec<Box<dyn Hook>>,
    pub children: Vec<FiberId>,
    pub parent: Option<FiberId>,
    pub flags: DirtyFlags,
    pub layout_rect: Option<Rect>,
    /// SceneItem ids owned by this fiber (for incremental paint tracking).
    pub scene_item_ids: Vec<u64>,
    /// The type-erased widget data for layout and rebuild.
    pub(crate) view: Option<Arc<dyn AnyView>>,
}

impl Fiber {
    pub(crate) fn new(
        key: Option<Key>,
        widget_type: TypeId,
        view: Option<Arc<dyn AnyView>>,
    ) -> Self {
        Fiber {
            id: None, // set by FiberArena::insert
            key,
            widget_type,
            hooks: Vec::new(),
            children: Vec::new(),
            parent: None,
            flags: DirtyFlags::NONE,
            layout_rect: None,
            scene_item_ids: Vec::new(),
            view,
        }
    }
}

// ── FiberArena ───────────────────────────────────────────────────────────────

/// Generation-checked fiber storage backed by a slotmap.
pub struct FiberArena {
    fibers: SlotMap<FiberId, Fiber>,
}

impl Default for FiberArena {
    fn default() -> Self {
        Self::new()
    }
}

impl FiberArena {
    pub fn new() -> Self {
        FiberArena {
            fibers: SlotMap::with_key(),
        }
    }

    /// Inserts a fiber and returns its generation-checked key.
    /// The fiber's `id` field is updated to the assigned key.
    pub fn insert(&mut self, fiber: Fiber) -> FiberId {
        let id = self.fibers.insert(fiber);
        if let Some(f) = self.fibers.get_mut(id) {
            f.id = Some(id);
        }
        id
    }

    /// Looks up a fiber by key. Returns `None` if the key is stale (generation
    /// mismatch) or the fiber was removed.
    pub fn get(&self, id: FiberId) -> Option<&Fiber> {
        self.fibers.get(id)
    }

    /// Looks up a fiber mutably by key.
    pub fn get_mut(&mut self, id: FiberId) -> Option<&mut Fiber> {
        self.fibers.get_mut(id)
    }

    /// Removes a fiber, returning the owned value. Returns `None` on stale key.
    pub fn remove(&mut self, id: FiberId) -> Option<Fiber> {
        self.fibers.remove(id)
    }

    /// Returns true if the arena contains a live fiber for the given key.
    pub fn contains(&self, id: FiberId) -> bool {
        self.fibers.contains_key(id)
    }
}

// ── Reconciliation ───────────────────────────────────────────────────────────

/// Recursively unmounts a fiber and its entire subtree.
///
/// Unsubscribes all hooks and removes all fibers from the arena.
pub(crate) fn unmount_fiber(arena: &mut FiberArena, id: FiberId) {
    // Clone children before borrowing mutably below
    let children = arena
        .get(id)
        .map(|f| f.children.clone())
        .unwrap_or_default();
    for child_id in children {
        unmount_fiber(arena, child_id);
    }
    if let Some(fiber) = arena.remove(id) {
        for hook in &fiber.hooks {
            hook.unsubscribe_all(id);
        }
    }
}

/// Creates a new Fiber from a View and recursively reconciles its children.
/// Pass `None` for `parent_id` for root-level fibers.
pub(crate) fn create_fiber_from_view(
    arena: &mut FiberArena,
    parent_id: Option<FiberId>,
    view: View,
) -> FiberId {
    let key = view.key().cloned();
    let widget_type = view.widget_type();
    let (inner, children, _key) = view.decompose();

    let mut fiber = Fiber::new(key, widget_type, Some(inner));
    fiber.parent = parent_id;
    fiber.flags.insert(DirtyFlags::BUILD_DIRTY);
    fiber.flags.insert(DirtyFlags::LAYOUT_DIRTY);
    let id = arena.insert(fiber);

    // Reconcile children of the new fiber
    let new_children = reconcile_children(arena, id, &[], children);
    if let Some(f) = arena.get_mut(id) {
        f.children = new_children;
    }

    id
}

/// Reconciles a parent fiber's children against new Views.
///
/// Matches old and new children by position, widget type, and key.
/// Returns the new list of child FiberIds.
pub(crate) fn reconcile_children(
    arena: &mut FiberArena,
    parent_id: FiberId,
    old_children: &[FiberId],
    new_views: Vec<View>,
) -> Vec<FiberId> {
    let max_len = old_children.len().max(new_views.len());
    let mut new_child_ids = Vec::with_capacity(max_len);
    let mut view_iter = new_views.into_iter();

    for i in 0..max_len {
        let old_id = old_children.get(i).copied();
        let view = view_iter.next();

        match (old_id, view) {
            (Some(old_id), Some(view)) => {
                let can_reuse = match arena.get(old_id) {
                    Some(old_fiber) => {
                        old_fiber.widget_type == view.widget_type()
                            && old_fiber.key.as_ref() == view.key()
                    }
                    None => false,
                };

                if can_reuse {
                    // Reuse the old fiber -- update view and reconcile children
                    let (inner, view_children, _key) = view.decompose();

                    let grand_old = arena
                        .get(old_id)
                        .map(|f| f.children.clone())
                        .unwrap_or_default();

                    if let Some(fiber) = arena.get_mut(old_id) {
                        fiber.view = Some(inner);
                    }

                    let new_grandchildren =
                        reconcile_children(arena, old_id, &grand_old, view_children);

                    if let Some(fiber) = arena.get_mut(old_id) {
                        fiber.children = new_grandchildren;
                    }

                    new_child_ids.push(old_id);
                } else {
                    // Type or key mismatch -- unmount old, create new
                    unmount_fiber(arena, old_id);
                    let new_id = create_fiber_from_view(arena, Some(parent_id), view);
                    new_child_ids.push(new_id);
                }
            }
            (Some(old_id), None) => {
                // Old child no longer exists in the new View tree
                unmount_fiber(arena, old_id);
            }
            (None, Some(view)) => {
                // New child with no matching old fiber
                let new_id = create_fiber_from_view(arena, Some(parent_id), view);
                new_child_ids.push(new_id);
            }
            (None, None) => unreachable!("loop bound is max(old, new) so at least one is Some"),
        }
    }

    new_child_ids
}

// ── Layout ───────────────────────────────────────────────────────────────────

/// Two-pass layout walk:
///   1. Collect child intrinsic sizes bottom-up.
///   2. Call `layout_children` on the parent to compute own size and child origins.
///   3. Recurse into children with computed positions.
pub(crate) fn layout_fiber(
    arena: &mut FiberArena,
    id: FiberId,
    constraints: BoxConstraints,
    origin: Point,
) {
    // Phase 1: collect child intrinsic sizes (bottom-up)
    let child_specs: Vec<(FiberId, Size)> = {
        let fiber = match arena.get(id) {
            Some(f) => f,
            None => return,
        };
        let children = fiber.children.clone();
        let mut specs = Vec::with_capacity(children.len());
        for &cid in &children {
            let child_size = if let Some(child) = arena.get(cid) {
                child
                    .view
                    .as_ref()
                    .map(|v| v.intrinsic_size(constraints))
                    .unwrap_or(Size::ZERO)
            } else {
                Size::ZERO
            };
            specs.push((cid, child_size));
        }
        specs
    };

    // Phase 2: compute own size and child origins
    let (own_size, child_origins) = {
        let fiber = match arena.get(id) {
            Some(f) => f,
            None => return,
        };
        let sizes: Vec<Size> = child_specs.iter().map(|(_, s)| *s).collect();

        fiber
            .view
            .as_ref()
            .map(|v| v.layout_children(constraints, &sizes))
            .unwrap_or((constraints.constrain(Size::ZERO), vec![]))
    };

    // Store own rect
    if let Some(fiber) = arena.get_mut(id) {
        fiber.layout_rect = Some(Rect::from_min_size(origin, own_size));
    }

    // Phase 3: recurse into children with computed positions
    for ((cid, _child_size), child_pos) in child_specs.iter().zip(child_origins.iter()) {
        let child_origin = Point::new(origin.x + child_pos.x, origin.y + child_pos.y);
        let child_constraints = BoxConstraints::loose(own_size);
        layout_fiber(arena, *cid, child_constraints, child_origin);
    }
}

// ── Paint ────────────────────────────────────────────────────────────────────

static NEXT_SCENE_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn next_scene_id() -> u64 {
    NEXT_SCENE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Walks the fiber tree top-down, accumulates Primitives from each fiber
/// via `AnyView::paint_primitives`, assigns incrementing paint_order and
/// scene ids, and returns a flat Vec of SceneItems.
pub(crate) fn paint_fiber(arena: &FiberArena, id: FiberId, base_order: u32) -> Vec<SceneItem> {
    let mut items = Vec::new();
    let mut order = base_order;

    let fiber = match arena.get(id) {
        Some(f) => f,
        None => return items,
    };

    let rect = fiber.layout_rect;
    let children = fiber.children.clone();
    let _has_view = fiber.view.is_some();

    // Collect self primitives
    if let Some(ref view) = fiber.view
        && let Some(r) = rect
    {
        for prim in view.paint_primitives(r) {
            items.push(SceneItem {
                id: next_scene_id(),
                primitive: prim,
                paint_order: order,
            });
            order += 1;
        }
    }

    // Collect children (in order)
    for child_id in &children {
        let child_items = paint_fiber(arena, *child_id, order);
        order += child_items.len() as u32;
        items.extend(child_items);
    }

    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Size;
    use crate::signal::{PENDING_DIRTY, Signal};
    use crate::view::BuildCx;

    /// Helper: returns None for root fibers that have no parent.
    fn no_parent() -> Option<FiberId> {
        None
    }

    fn clear_dirty_queue() {
        PENDING_DIRTY.with(|q| q.borrow_mut().clear());
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
        fn build(self: Box<Self>, _cx: &mut BuildCx) -> View {
            View::new(TestView(self.0.clone(), vec![]), self.1, None)
        }
        fn intrinsic_size(&self, c: BoxConstraints) -> Size {
            c.constrain(Size::new(10.0, 10.0))
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
        fn build(self: Box<Self>, _cx: &mut BuildCx) -> View {
            View::new(OtherView, vec![], None)
        }
        fn intrinsic_size(&self, c: BoxConstraints) -> Size {
            c.constrain(Size::new(20.0, 20.0))
        }
    }

    fn other_view() -> View {
        View::new(OtherView, vec![], None)
    }

    #[derive(Clone)]
    struct KeyedTestView {
        name: String,
        key: Key,
    }

    impl AnyView for KeyedTestView {
        fn key(&self) -> Option<&Key> {
            Some(&self.key)
        }
        fn widget_type(&self) -> TypeId {
            TypeId::of::<Self>()
        }
        fn build(self: Box<Self>, _cx: &mut BuildCx) -> View {
            View::new(
                KeyedTestView {
                    name: self.name.clone(),
                    key: self.key.clone(),
                },
                vec![],
                None,
            )
        }
        fn intrinsic_size(&self, c: BoxConstraints) -> Size {
            c.constrain(Size::new(20.0, 20.0))
        }
    }

    fn keyed_test_view(name: &str, key: &str) -> View {
        View::new(
            KeyedTestView {
                name: name.to_string(),
                key: Key::new(key),
            },
            vec![],
            None,
        )
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
        let dirty = PENDING_DIRTY.with(|q| q.borrow().clone());
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

        let items = paint_fiber(&arena, fiber_id, 0);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].paint_order, 0);
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

        let items = paint_fiber(&arena, fiber_id, 0);
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

        let items = paint_fiber(&arena, fiber_id, 0);
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

        let items = paint_fiber(&arena, fiber_id, 0);
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

        let items = paint_fiber(&arena, fiber_id, 0);
        // Stack background + 2 children = 3
        assert_eq!(items.len(), 3);
        // Paint order: stack bg (0), first child (1), second child (2)
        assert_eq!(items[0].paint_order, 0);
        assert_eq!(items[1].paint_order, 1);
        assert_eq!(items[2].paint_order, 2);
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

        let items = paint_fiber(&arena, id, 0);
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
        let items = paint_fiber(&arena, fiber_id, 10);
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].paint_order, 10,
            "paint order should start at base_order"
        );
    }
}
