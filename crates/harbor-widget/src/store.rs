//! Reusable Host-to-widget state publication and widget-to-Host action transport.
//!
//! [`Store`] combines a UI-thread [`Signal`] with a private FIFO action inbox. It does not run a
//! reducer or execute effects: the Host publishes state and drains actions at its chosen event-turn
//! boundary. [`Dispatcher::dispatch`] only enqueues an action and does not wake a Runtime, so
//! background producers must use the Host's existing external invalidation mechanism as well.

use crate::signal::Signal;
use crate::view::BuildCx;
use std::cell::Ref;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};

/// A reusable boundary for Host-published widget state and widget-submitted actions.
///
/// The state signal follows the Runtime's UI-thread, pull-based dirty-fiber model. Actions remain
/// separate one-shot values and are returned in FIFO order when the Host calls [`Self::drain_actions`].
/// This type intentionally provides no reducer, effect executor, or automatic Runtime wake.
pub struct Store<S, A> {
    state: Signal<S>,
    actions: Arc<Mutex<VecDeque<A>>>,
}

impl<S, A> Clone for Store<S, A> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            actions: Arc::clone(&self.actions),
        }
    }
}

impl<S, A> Store<S, A> {
    /// Creates a Store with the initial published state and an empty action inbox.
    pub fn new(initial: S) -> Self {
        Self {
            state: Signal::new(initial),
            actions: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// Returns the shared state signal without subscribing the caller.
    pub fn state(&self) -> &Signal<S> {
        &self.state
    }

    /// Subscribes the current Fiber and borrows the latest state.
    pub fn watch<'a>(&'a self, cx: &mut BuildCx) -> Ref<'a, S>
    where
        S: 'static,
    {
        cx.track(&self.state);
        self.state.read()
    }

    /// Publishes new state through the existing Signal dirty-fiber mechanism.
    pub fn set_state(&self, next: S) {
        self.state.set(next);
    }

    /// Returns a cloneable, write-only capability for submitting actions.
    pub fn dispatcher(&self) -> Dispatcher<A> {
        Dispatcher {
            actions: Arc::clone(&self.actions),
        }
    }

    /// Drains all currently queued actions in FIFO order.
    ///
    /// A poisoned inbox is recovered with `PoisonError::into_inner`, preserving queued and future
    /// actions rather than silently discarding user intent.
    pub fn drain_actions(&self) -> Vec<A> {
        lock_recover(&self.actions).drain(..).collect()
    }
}

/// A cloneable, write-only capability for submitting one-shot actions to a [`Store`].
///
/// Dispatch preserves FIFO order but performs no reduction, effects, or Runtime wake. The Host is
/// responsible for draining actions at an appropriate event-turn boundary.
pub struct Dispatcher<A> {
    actions: Arc<Mutex<VecDeque<A>>>,
}

impl<A> Clone for Dispatcher<A> {
    fn clone(&self) -> Self {
        Self {
            actions: Arc::clone(&self.actions),
        }
    }
}

impl<A> Dispatcher<A> {
    /// Enqueues an action for the Host to consume later.
    ///
    /// A poisoned inbox is recovered so dispatch remains deterministic after a panic while the
    /// queue lock was held.
    pub fn dispatch(&self, action: A) {
        lock_recover(&self.actions).push_back(action);
    }
}

fn lock_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Size;
    use crate::runtime::Runtime;
    use crate::view::{Component, View};
    use crate::widgets::sized_box::SizedBox;
    use std::cell::Cell;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::rc::Rc;
    use std::time::Instant;

    struct NonCloneState(&'static str);

    struct NonCloneAction(u32);

    #[test]
    fn state_is_shared_and_set_advances_signal_version() {
        let store = Store::<_, ()>::new(NonCloneState("initial"));
        let clone = store.clone();
        assert_eq!(store.state().read().0, "initial");
        let version = store.state().version();

        clone.set_state(NonCloneState("updated"));

        assert_eq!(store.state().read().0, "updated");
        assert!(store.state().version() > version);
    }

    #[test]
    fn cloned_dispatchers_accept_non_clone_actions_in_fifo_order() {
        let store = Store::<(), NonCloneAction>::new(());
        let first = store.dispatcher();
        let second = first.clone();

        first.dispatch(NonCloneAction(1));
        second.dispatch(NonCloneAction(2));
        first.dispatch(NonCloneAction(3));

        let values = store
            .drain_actions()
            .into_iter()
            .map(|action| action.0)
            .collect::<Vec<_>>();
        assert_eq!(values, vec![1, 2, 3]);
        assert!(store.drain_actions().is_empty());
    }

    #[test]
    fn empty_and_repeated_drains_do_not_replay_actions() {
        let store = Store::<(), u32>::new(());
        assert!(store.drain_actions().is_empty());

        store.dispatcher().dispatch(7);
        assert_eq!(store.drain_actions(), vec![7]);
        assert!(store.drain_actions().is_empty());
    }

    #[test]
    fn poisoned_action_inbox_recovers_queued_and_future_actions() {
        let store = Store::<(), u32>::new(());
        store.dispatcher().dispatch(1);
        let actions = Arc::clone(&store.actions);
        let panic = catch_unwind(AssertUnwindSafe(|| {
            let mut queue = actions.lock().unwrap();
            queue.push_back(2);
            panic!("poison action inbox");
        }));
        assert!(panic.is_err());

        store.dispatcher().dispatch(3);

        assert_eq!(store.drain_actions(), vec![1, 2, 3]);
        assert!(store.drain_actions().is_empty());
    }

    #[derive(Clone)]
    struct WatchingComponent {
        store: Store<u32, ()>,
        builds: Rc<Cell<usize>>,
        observed: Rc<Cell<u32>>,
    }

    impl Component for WatchingComponent {
        fn build(&self, cx: &mut BuildCx) -> View {
            self.builds.set(self.builds.get() + 1);
            self.observed.set(*self.store.watch(cx));
            SizedBox::new(Size::new(1.0, 1.0)).build(cx)
        }
    }

    #[test]
    fn watch_rebuilds_subscribed_fiber_through_signal_updates() {
        let store = Store::<u32, ()>::new(0);
        let builds = Rc::new(Cell::new(0));
        let observed = Rc::new(Cell::new(u32::MAX));
        let mut runtime = Runtime::new();
        runtime.set_root(WatchingComponent {
            store: store.clone(),
            builds: Rc::clone(&builds),
            observed: Rc::clone(&observed),
        });
        runtime.update(Instant::now());
        assert_eq!((builds.get(), observed.get()), (1, 0));

        store.set_state(1);
        store.set_state(2);
        runtime.update(Instant::now());

        assert_eq!((builds.get(), observed.get()), (2, 2));
    }
}
