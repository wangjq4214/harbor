//! Optional adapter-owned widget hot-reload observation and generation lifecycle.

use crate::view::{BuildCx, Component, View};
use std::fmt;

/// A root component erased at the reusable host boundary.
pub(crate) struct WidgetHmrRoot(pub(crate) Box<dyn Component>);

impl Component for WidgetHmrRoot {
    fn build(&self, cx: &mut BuildCx) -> View {
        self.0.build(cx)
    }
}

/// Opaque UI-thread work produced by the widget HMR observer.
///
/// Applications may transport this value, but generation and barrier policy stay
/// private to the winit adapter.
pub struct WidgetHmrWork {
    kind: WidgetHmrWorkKind,
}

#[cfg_attr(
    not(any(all(target_os = "windows", debug_assertions), test)),
    allow(dead_code)
)]
pub(crate) enum WidgetHmrWorkKind {
    Prepare {
        generation: u64,
        barrier: Box<dyn Send>,
    },
    Activate {
        generation: u64,
    },
}

impl WidgetHmrWork {
    #[cfg(any(all(target_os = "windows", debug_assertions), test))]
    fn prepare(generation: u64, barrier: impl Send + 'static) -> Self {
        Self {
            kind: WidgetHmrWorkKind::Prepare {
                generation,
                barrier: Box::new(barrier),
            },
        }
    }

    #[cfg(any(all(target_os = "windows", debug_assertions), test))]
    fn activate(generation: u64) -> Self {
        Self {
            kind: WidgetHmrWorkKind::Activate { generation },
        }
    }

    pub(crate) fn into_kind(self) -> WidgetHmrWorkKind {
        self.kind
    }
}

impl fmt::Debug for WidgetHmrWork {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("WidgetHmrWork(..)")
    }
}

/// Application-supplied reload observation, UI wake transport, and root factory.
///
/// The observer and wake closure are moved to one background worker. The root
/// factory remains on the UI thread inside the owning [`super::WinitWindowHost`].
pub struct WidgetHmrConfig {
    observer: hot_lib_reloader::LibReloadObserver,
    wake: Box<dyn Fn(WidgetHmrWork) -> bool + Send>,
    root_factory: Box<dyn Fn() -> Box<dyn Component>>,
}

impl WidgetHmrConfig {
    pub fn new<W, F>(
        observer: hot_lib_reloader::LibReloadObserver,
        wake: W,
        root_factory: F,
    ) -> Self
    where
        W: Fn(WidgetHmrWork) -> bool + Send + 'static,
        F: Fn() -> Box<dyn Component> + 'static,
    {
        Self {
            observer,
            wake: Box::new(wake),
            root_factory: Box::new(root_factory),
        }
    }

    pub(crate) fn start(self) -> std::io::Result<Option<WidgetHmrState>> {
        let Self {
            observer,
            wake,
            root_factory,
        } = self;

        #[cfg(all(target_os = "windows", debug_assertions))]
        {
            std::thread::Builder::new()
                .name("harbor-widget-hot-reload".to_owned())
                .spawn(move || observer_loop(observer, wake))?;
            Ok(Some(WidgetHmrState {
                lifecycle: WidgetHmrLifecycle::new(),
                root_factory,
            }))
        }

        #[cfg(not(all(target_os = "windows", debug_assertions)))]
        {
            drop(observer);
            drop(wake);
            drop(root_factory);
            Ok(None)
        }
    }
}

#[cfg(all(target_os = "windows", debug_assertions))]
fn observer_loop(
    observer: hot_lib_reloader::LibReloadObserver,
    wake: Box<dyn Fn(WidgetHmrWork) -> bool + Send>,
) {
    let mut generation = 0_u64;
    loop {
        let barrier = observer.wait_for_about_to_reload();
        let Some(next_generation) = generation.checked_add(1) else {
            drop(barrier);
            return;
        };
        generation = next_generation;
        if !wake(WidgetHmrWork::prepare(generation, barrier)) {
            return;
        }
        observer.wait_for_reload();
        if !wake(WidgetHmrWork::activate(generation)) {
            return;
        }
    }
}

pub(crate) struct WidgetHmrState {
    pub(crate) lifecycle: WidgetHmrLifecycle,
    pub(crate) root_factory: Box<dyn Fn() -> Box<dyn Component>>,
}

/// UI-thread generation state machine. A generation may prepare and attempt
/// activation once; only a successful attempt restores widget input routing.
pub(crate) struct WidgetHmrLifecycle {
    newest_generation: u64,
    awaiting_generation: Option<u64>,
    attempted_generation: Option<u64>,
    active: bool,
}

impl WidgetHmrLifecycle {
    #[cfg(any(all(target_os = "windows", debug_assertions), test))]
    pub(crate) const fn new() -> Self {
        Self {
            newest_generation: 0,
            awaiting_generation: None,
            attempted_generation: None,
            active: true,
        }
    }

    pub(crate) const fn accepts_widget_input(&self) -> bool {
        self.active
    }

    pub(crate) fn begin_prepare(&mut self, generation: u64) -> bool {
        if generation == 0 || generation <= self.newest_generation {
            return false;
        }
        self.newest_generation = generation;
        self.awaiting_generation = Some(generation);
        self.attempted_generation = None;
        self.active = false;
        true
    }

    pub(crate) fn begin_activate(&mut self, generation: u64) -> bool {
        if self.awaiting_generation != Some(generation)
            || self.attempted_generation == Some(generation)
        {
            return false;
        }
        self.attempted_generation = Some(generation);
        true
    }

    pub(crate) fn finish_activate(&mut self, generation: u64) {
        debug_assert_eq!(self.awaiting_generation, Some(generation));
        debug_assert_eq!(self.attempted_generation, Some(generation));
        self.awaiting_generation = None;
        self.active = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    struct DropProbe(Arc<AtomicUsize>);

    impl Drop for DropProbe {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn dropped_prepare_work_releases_its_barrier() {
        let drops = Arc::new(AtomicUsize::new(0));
        drop(WidgetHmrWork::prepare(1, DropProbe(Arc::clone(&drops))));
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn lifecycle_rejects_stale_duplicate_and_out_of_order_work() {
        let mut lifecycle = WidgetHmrLifecycle::new();
        assert!(lifecycle.accepts_widget_input());
        assert!(lifecycle.begin_prepare(2));
        assert!(!lifecycle.accepts_widget_input());
        assert!(!lifecycle.begin_prepare(2));
        assert!(!lifecycle.begin_activate(1));
        assert!(lifecycle.begin_activate(2));
        assert!(!lifecycle.begin_activate(2));
        lifecycle.finish_activate(2);
        assert!(lifecycle.accepts_widget_input());
        assert!(!lifecycle.begin_prepare(1));
    }

    #[test]
    fn failed_activation_can_recover_only_through_a_new_generation() {
        let mut lifecycle = WidgetHmrLifecycle::new();
        assert!(lifecycle.begin_prepare(1));
        assert!(lifecycle.begin_activate(1));
        assert!(!lifecycle.begin_activate(1));
        assert!(!lifecycle.accepts_widget_input());
        assert!(lifecycle.begin_prepare(2));
        assert!(lifecycle.begin_activate(2));
        lifecycle.finish_activate(2);
        assert!(lifecycle.accepts_widget_input());
    }
}
