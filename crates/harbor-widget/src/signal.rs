use crate::fiber::FiberId;
use hashbrown::{HashMap, HashSet};
use std::cell::{Cell, Ref, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

// ── Runtime-scoped dirty queues ─────────────────────────────────────────────

/// Identifies one Runtime's signal subscriptions and pending work.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct RuntimeId(u64);

impl RuntimeId {
    pub(crate) fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        Self(NEXT_ID.fetch_add(1, Ordering::Relaxed))
    }
}

pub(crate) const DEFAULT_RUNTIME_ID: RuntimeId = RuntimeId(0);

thread_local! {
    /// Pending fibers grouped by their owning Runtime.
    pub(crate) static PENDING_DIRTY: RefCell<HashMap<RuntimeId, HashSet<FiberId>>> =
        RefCell::new(HashMap::new());

    static ACTIVE_RUNTIME: Cell<Option<RuntimeId>> = const { Cell::new(None) };
}

/// Temporarily associates Signal subscriptions made during a Runtime build
/// with that Runtime. The previous scope is restored when this is dropped.
pub(crate) struct RuntimeScope {
    previous: Option<RuntimeId>,
}

impl RuntimeScope {
    pub(crate) fn enter(id: RuntimeId) -> Self {
        let previous = ACTIVE_RUNTIME.with(|active| active.replace(Some(id)));
        Self { previous }
    }
}

impl Drop for RuntimeScope {
    fn drop(&mut self) {
        ACTIVE_RUNTIME.with(|active| active.set(self.previous));
    }
}

fn active_runtime() -> RuntimeId {
    ACTIVE_RUNTIME.with(Cell::get).unwrap_or(DEFAULT_RUNTIME_ID)
}

/// Inserts a FiberId into a specific Runtime's dirty set.
pub(crate) fn mark_dirty_for(runtime_id: RuntimeId, id: FiberId) {
    PENDING_DIRTY.with(|queues| {
        queues
            .borrow_mut()
            .entry(runtime_id)
            .or_default()
            .insert(id);
    });
}

/// Takes only the pending work belonging to one Runtime.
pub(crate) fn take_dirty(runtime_id: RuntimeId) -> HashSet<FiberId> {
    PENDING_DIRTY.with(|queues| queues.borrow_mut().remove(&runtime_id).unwrap_or_default())
}

/// Removes all pending work for a Runtime that is being destroyed.
pub(crate) fn remove_runtime(runtime_id: RuntimeId) {
    PENDING_DIRTY.with(|queues| {
        queues.borrow_mut().remove(&runtime_id);
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
#[derive(Clone, Copy)]
struct Subscriber {
    runtime_id: RuntimeId,
    fiber_id: FiberId,
}

struct SignalData<T> {
    value: T,
    version: u64,
    subscribers: Vec<Subscriber>,
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
        for subscriber in &data.subscribers {
            mark_dirty_for(subscriber.runtime_id, subscriber.fiber_id);
        }
    }

    /// Returns the current version number.
    pub fn version(&self) -> u64 {
        self.data.borrow().version
    }

    /// Subscribes a fiber to this signal (idempotent).
    pub fn subscribe(&self, id: FiberId) {
        let runtime_id = active_runtime();
        let mut data = self.data.borrow_mut();
        if !data
            .subscribers
            .iter()
            .any(|subscriber| subscriber.runtime_id == runtime_id && subscriber.fiber_id == id)
        {
            data.subscribers.push(Subscriber {
                runtime_id,
                fiber_id: id,
            });
        }
    }

    /// Removes a fiber's subscription in the active Runtime scope.
    pub fn unsubscribe(&self, id: FiberId) {
        let runtime_id = active_runtime();
        let mut data = self.data.borrow_mut();
        data.subscribers
            .retain(|subscriber| subscriber.runtime_id != runtime_id || subscriber.fiber_id != id);
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

        let dirty = PENDING_DIRTY.with(|q| {
            q.borrow()
                .get(&DEFAULT_RUNTIME_ID)
                .cloned()
                .unwrap_or_default()
        });
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
