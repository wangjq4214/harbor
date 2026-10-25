//! Subscription ownership is exercised only through external public build paths.
use harbor_widget::{
    SizedBox, Store,
    layout::Size,
    runtime::Runtime,
    signal::Signal,
    view::{BuildCx, Component},
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Instant,
};

fn update(runtime: &mut Runtime) {
    runtime.update(Instant::now());
}

#[test]
fn rebuilding_replaces_dependencies_retains_hooks_and_unmount_cleans_up() {
    let select_second = Signal::new(false);
    let first = Signal::new(1u32);
    let second = Signal::new(2u32);
    let builds = Rc::new(Cell::new(0));
    let observed = Rc::new(Cell::new(0));
    let hook = Rc::new(RefCell::new(None::<Signal<u32>>));
    let mut runtime = Runtime::new();
    runtime.set_root({
        let select_second = select_second.clone();
        let first = first.clone();
        let second = second.clone();
        let builds = builds.clone();
        let observed = observed.clone();
        let hook = hook.clone();
        move |cx: &mut BuildCx| {
            cx.track(&select_second);
            let source = if *select_second.read() {
                &second
            } else {
                &first
            };
            cx.track(source);
            let state = cx.use_state(|| 10u32);
            observed.set(*source.read() + *state.read());
            *hook.borrow_mut() = Some(state);
            builds.set(builds.get() + 1);
            SizedBox::new(Size::new(10.0, 10.0)).unwrap().build(cx)
        }
    });
    update(&mut runtime);
    let root = runtime.root_id();
    assert_eq!((builds.get(), observed.get()), (1, 11));

    // Multiple writes are coalesced by the existing dirty-fiber mechanism.
    first.set(3);
    first.set(4);
    update(&mut runtime);
    assert_eq!((builds.get(), observed.get()), (2, 14));
    let state = hook.borrow().as_ref().unwrap().clone();
    state.set(20);
    update(&mut runtime);
    assert_eq!((builds.get(), observed.get()), (3, 24));

    select_second.set(true);
    update(&mut runtime);
    assert_eq!(runtime.root_id(), root);
    assert_eq!((builds.get(), observed.get()), (4, 22));
    first.set(100);
    assert!(!runtime.update(Instant::now()).request_redraw);
    assert_eq!(builds.get(), 4, "the old dependency must be removed");
    second.set(5);
    update(&mut runtime);
    assert_eq!((builds.get(), observed.get()), (5, 25));

    runtime.set_root(SizedBox::new(Size::new(10.0, 10.0)).unwrap());
    update(&mut runtime);
    select_second.set(false);
    first.set(101);
    second.set(6);
    state.set(30);
    assert!(!runtime.update(Instant::now()).request_redraw);
    assert_eq!(builds.get(), 5, "removed consumers cannot rebuild");
}

#[test]
fn shared_signal_and_store_subscriptions_are_isolated_and_survive_other_runtime_cleanup() {
    let shared = Signal::new(1u32);
    let store = Store::<u32, ()>::new(10);
    let first_builds = Rc::new(Cell::new(0));
    let second_builds = Rc::new(Cell::new(0));
    let mut first = Runtime::new();
    let mut second = Runtime::new();
    for (runtime, builds) in [
        (&mut first, first_builds.clone()),
        (&mut second, second_builds.clone()),
    ] {
        let shared = shared.clone();
        let store = store.clone();
        runtime.set_root(move |cx: &mut BuildCx| {
            cx.track(&shared);
            let total = *shared.read() + *store.watch(cx);
            builds.set(builds.get() + 1);
            SizedBox::new(Size::new(total as f32, 10.0))
                .unwrap()
                .build(cx)
        });
        update(runtime);
    }
    assert_eq!((first_builds.get(), second_builds.get()), (1, 1));
    // These equal arena-local IDs must not collapse the Runtime-owned subscriptions.
    assert_eq!(first.root_id(), second.root_id());
    shared.set(2);
    update(&mut first);
    assert_eq!((first_builds.get(), second_builds.get()), (2, 1));
    update(&mut second);
    assert_eq!((first_builds.get(), second_builds.get()), (2, 2));
    store.set_state(20);
    update(&mut second);
    assert_eq!((first_builds.get(), second_builds.get()), (2, 3));
    update(&mut first);
    assert_eq!((first_builds.get(), second_builds.get()), (3, 3));

    first.set_root(SizedBox::new(Size::ZERO).unwrap());
    update(&mut first);
    shared.set(3);
    assert!(!first.update(Instant::now()).request_redraw);
    update(&mut second);
    assert_eq!((first_builds.get(), second_builds.get()), (3, 4));
    drop(first);
    store.set_state(30);
    update(&mut second);
    assert_eq!(second_builds.get(), 5);

    second.set_root(SizedBox::new(Size::ZERO).unwrap());
    update(&mut second);
    shared.set(4);
    store.set_state(40);
    assert!(!second.update(Instant::now()).request_redraw);
    assert_eq!(second_builds.get(), 5);
}

#[test]
fn plain_reads_do_not_create_implicit_build_subscriptions() {
    let signal = Signal::new(1u32);
    let store = Store::<u32, ()>::new(2);
    let builds = Rc::new(Cell::new(0));
    let mut runtime = Runtime::new();
    runtime.set_root({
        let signal = signal.clone();
        let store = store.clone();
        let builds = builds.clone();
        move |cx: &mut BuildCx| {
            let _ = signal.read();
            let _ = store.state().read();
            builds.set(builds.get() + 1);
            SizedBox::new(Size::ZERO).unwrap().build(cx)
        }
    });
    update(&mut runtime);
    signal.set(3);
    store.set_state(4);
    assert!(!runtime.update(Instant::now()).request_redraw);
    assert_eq!(builds.get(), 1);
}
