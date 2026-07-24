use crate::fiber::FiberId;
use hashbrown::HashSet;
use std::cell::{Ref, RefCell};
use std::rc::Rc;

// ── Dirty Queue (shared with Runtime) ────────────────────────────────────────

thread_local! {
    /// Global set of fibers that have been marked dirty by Signal writes.
    /// Uses hashbrown::HashSet for O(1) deduplication.
    ///
    /// NOTE: This is thread-local, not Runtime-scoped. It assumes a single
    /// Runtime per thread. If multiple Runtime instances exist on the same
    /// thread, Signal writes from one will be processed by the other's update().
    pub(crate) static PENDING_DIRTY: RefCell<HashSet<FiberId>> =
        RefCell::new(HashSet::new());
}

/// Inserts a FiberId into the dirty set (idempotent, O(1)).
pub(crate) fn mark_dirty(id: FiberId) {
    PENDING_DIRTY.with(|q| {
        q.borrow_mut().insert(id);
    });
}

// ── Hook Trait ───────────────────────────────────────────────────────────────

/// Per-hook type-erased interface for subscription management.
///
/// Each concrete hook type (e.g., `Signal<T>`) implements this trait
/// so it can be stored in the fiber's type-erased hook list.
#[allow(dead_code)]
pub(crate) trait Hook: 'static {
    fn unsubscribe_all(&self, id: FiberId);
    fn as_any_ref(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

// ── Signal ───────────────────────────────────────────────────────────────────

/// Internal shared data for a Signal.
struct SignalData<T> {
    value: T,
    version: u64,
    subscribers: Vec<FiberId>,
}

/// A fine-grained pull-based reactive state cell.
///
/// Cloning a `Signal` creates a new handle to the same underlying shared state.
/// Uses interior mutability so that `set()` can be called on a shared reference.
pub struct Signal<T> {
    data: Rc<RefCell<SignalData<T>>>,
}

impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        Signal {
            data: Rc::clone(&self.data),
        }
    }
}

impl<T: 'static> Hook for Signal<T> {
    fn unsubscribe_all(&self, id: FiberId) {
        self.unsubscribe(id);
    }

    fn as_any_ref(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl<T> Signal<T> {
    pub fn new(value: T) -> Self {
        Signal {
            data: Rc::new(RefCell::new(SignalData {
                value,
                version: 0,
                subscribers: Vec::new(),
            })),
        }
    }

    /// Reads the current value, returning a borrowed reference.
    ///
    /// Panics if the signal is already mutably borrowed.
    pub fn read(&self) -> Ref<'_, T> {
        Ref::map(self.data.borrow(), |d| &d.value)
    }

    /// Updates the value, increments the version, and marks subscribers dirty.
    pub fn set(&self, value: T) {
        let mut data = self.data.borrow_mut();
        data.value = value;
        data.version += 1;
        for &id in &data.subscribers {
            mark_dirty(id);
        }
    }

    /// Returns the current version number.
    pub fn version(&self) -> u64 {
        self.data.borrow().version
    }

    /// Subscribes a fiber to this signal (idempotent).
    pub fn subscribe(&self, id: FiberId) {
        let mut data = self.data.borrow_mut();
        if !data.subscribers.contains(&id) {
            data.subscribers.push(id);
        }
    }

    /// Removes a fiber's subscription.
    pub fn unsubscribe(&self, id: FiberId) {
        let mut data = self.data.borrow_mut();
        data.subscribers.retain(|&x| x != id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_fiber_id() -> FiberId {
        // Create a temporary arena and fiber to get a valid FiberId
        let mut arena = crate::fiber::FiberArena::new();
        let fiber = crate::fiber::Fiber::new(None, std::any::TypeId::of::<()>(), None);
        arena.insert(fiber)
    }

    fn clear_dirty_queue() {
        PENDING_DIRTY.with(|q| q.borrow_mut().clear());
    }

    #[test]
    fn initial_value_readable() {
        let signal = Signal::new(42u32);
        assert_eq!(*signal.read(), 42);
    }

    #[test]
    fn set_updates_value_and_version() {
        let signal = Signal::new(0u32);
        let v1 = signal.version();
        signal.set(10);
        assert_eq!(*signal.read(), 10);
        assert!(signal.version() > v1);
    }

    #[test]
    fn multiple_subscribers() {
        let signal = Signal::new(0u32);
        let f1 = dummy_fiber_id();
        let f2 = dummy_fiber_id();
        signal.subscribe(f1);
        signal.subscribe(f2);
        signal.set(42);
        assert_eq!(*signal.read(), 42);
    }

    #[test]
    fn unsubscribe_removes_subscriber() {
        let signal = Signal::new(0u32);
        let f1 = dummy_fiber_id();
        let f2 = dummy_fiber_id();
        signal.subscribe(f1);
        signal.subscribe(f2);
        signal.unsubscribe(f1);
        signal.set(99);
        assert_eq!(*signal.read(), 99);
    }

    #[test]
    fn duplicate_subscribe_idempotent() {
        let signal = Signal::new(0u32);
        let fid = dummy_fiber_id();
        signal.subscribe(fid);
        signal.subscribe(fid);
        signal.set(55);
        assert_eq!(*signal.read(), 55);
    }

    #[test]
    fn set_dedupes_dirty_queue_for_duplicate_subscriber() {
        clear_dirty_queue();

        let signal = Signal::new(0u32);
        let fid = dummy_fiber_id();
        signal.subscribe(fid);
        signal.subscribe(fid);
        signal.set(1);

        let dirty = PENDING_DIRTY.with(|q| q.borrow().clone());
        assert!(dirty.contains(&fid));
        assert_eq!(dirty.len(), 1);
        clear_dirty_queue();
    }

    #[test]
    fn set_with_no_subscribers_no_panic() {
        let signal = Signal::new(0u32);
        signal.set(100);
        assert_eq!(*signal.read(), 100);
    }

    #[test]
    fn clone_shares_state() {
        let s1 = Signal::new(10u32);
        let s2 = s1.clone();
        assert_eq!(*s1.read(), 10);
        assert_eq!(*s2.read(), 10);
        s2.set(20);
        assert_eq!(*s1.read(), 20);
        assert_eq!(*s2.read(), 20);
    }

    #[test]
    fn hook_trait_unsubscribe_all() {
        let signal = Signal::new(0u32);
        let fid = dummy_fiber_id();
        signal.subscribe(fid);

        let hook: &dyn Hook = &signal;
        hook.unsubscribe_all(fid);
        // Unsubscribing again is a no-op
        hook.unsubscribe_all(fid);
    }
}
