use crate::layout::Rect;
use crate::signal::Hook;
use crate::view::{AnyView, Key};
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
    pub(crate) id: Option<FiberId>,
    pub(crate) key: Option<Key>,
    pub(crate) widget_type: TypeId,
    #[allow(private_interfaces)]
    pub(crate) hooks: Vec<Box<dyn Hook>>,
    pub(crate) children: Vec<FiberId>,
    pub(crate) parent: Option<FiberId>,
    pub(crate) flags: DirtyFlags,
    pub(crate) layout_rect: Option<Rect>,
    /// The type-erased widget data for layout and rebuild.
    pub(crate) view: Option<Arc<dyn AnyView>>,
    /// Stable scene identities for this fiber's local primitive slots.
    pub(crate) scene_item_ids: Vec<u64>,
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
            view,
            scene_item_ids: Vec::new(),
        }
    }

    /// Type id of the widget stored on this fiber.
    pub fn widget_type(&self) -> TypeId {
        self.widget_type
    }

    /// Layout rectangle in tree coordinates, if laid out.
    pub fn layout_rect(&self) -> Option<Rect> {
        self.layout_rect
    }

    /// Child fiber ids in paint order.
    pub fn children(&self) -> &[FiberId] {
        &self.children
    }

    /// Whether the view reports itself as focusable.
    pub fn is_focusable(&self) -> bool {
        self.view.as_ref().is_some_and(|v| v.is_focusable())
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
