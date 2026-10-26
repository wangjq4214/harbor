//! U1: replacement/clearing must remove mounted resources, not just descriptions.
use harbor_widget::{
    Children, ComponentExt, FocusScope, IntoChildView, Portal, WithChildren,
    fiber::FiberId,
    runtime::Runtime,
    scene::primitive::ExternalScheduleDemand,
    signal::Signal,
    view::{BuildCx, Component, View},
    widgets::external_surface::ExternalSurface,
};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    sync::Arc,
    time::Instant,
};

#[derive(Clone, Default)]
struct Resources {
    callbacks: Rc<RefCell<Vec<Weak<()>>>>,
    unmounts: Rc<Cell<usize>>,
    schedule_calls: Rc<Cell<usize>>,
    ime_calls: Rc<Cell<usize>>,
}

impl Resources {
    #[expect(
        clippy::arc_with_non_send_sync,
        reason = "exercise UI-local provider ownership and release through the public Arc API"
    )]
    fn surface(&self, draw_id: u64) -> ExternalSurface {
        // No strong token lives in the test/host. Only the actual callbacks own it.
        let draw = Rc::new(());
        let schedule = Rc::new(());
        let ime = Rc::new(());
        *self.callbacks.borrow_mut() = vec![
            Rc::downgrade(&draw),
            Rc::downgrade(&schedule),
            Rc::downgrade(&ime),
        ];
        let unmounts = self.unmounts.clone();
        let schedule_calls = self.schedule_calls.clone();
        let ime_calls = self.ime_calls.clone();
        ExternalSurface::new(draw_id)
            .on_draw(Arc::new(move |_, _, _, _, _| {
                let _keep_alive = &draw;
            }))
            .schedule_with(Arc::new(move |_, _| {
                let _keep_alive = &schedule;
                schedule_calls.set(schedule_calls.get() + 1);
                ExternalScheduleDemand::empty()
            }))
            .ime_with(Arc::new(move |_, _| {
                let _keep_alive = &ime;
                ime_calls.set(ime_calls.get() + 1);
                None
            }))
            .on_unmount(Arc::new(move || unmounts.set(unmounts.get() + 1)))
    }

    fn assert_live(&self, live: bool) {
        let callbacks = self.callbacks.borrow();
        assert_eq!(callbacks.len(), 3);
        for callback in callbacks.iter() {
            assert_eq!(callback.upgrade().is_some(), live);
        }
    }
}

#[derive(Clone)]
struct TrackedChild {
    draw_id: u64,
    source: Signal<u32>,
    state: Rc<RefCell<Option<Signal<u32>>>>,
    hook_resource: Rc<RefCell<Weak<()>>>,
    builds: Rc<Cell<usize>>,
    observed: Rc<Cell<u32>>,
    resources: Resources,
}

impl TrackedChild {
    fn new(draw_id: u64) -> Self {
        Self {
            draw_id,
            source: Signal::new(1),
            state: Default::default(),
            hook_resource: Default::default(),
            builds: Default::default(),
            observed: Default::default(),
            resources: Default::default(),
        }
    }
}

impl Component for TrackedChild {
    fn build(&self, cx: &mut BuildCx) -> View {
        cx.track(&self.source);
        let state = cx.use_state(|| 10u32);
        *self.state.borrow_mut() = Some(state.clone());
        let resource = cx.use_state(|| Rc::new(()));
        *self.hook_resource.borrow_mut() = Rc::downgrade(&resource.read());
        self.builds.set(self.builds.get() + 1);
        self.observed.set(*state.read() + *self.source.read());
        self.resources.surface(self.draw_id).build(cx)
    }
}

#[derive(Clone, Copy, Debug)]
enum Parent {
    FocusScope,
    ExternalSurface,
    Portal,
}

#[derive(Clone, Copy, Debug)]
enum Attachment {
    Setter,
    Checked,
    Block,
}

fn description(
    parent: Parent,
    attachment: Attachment,
    mode: u8,
    a: TrackedChild,
    b: TrackedChild,
    parent_resources: &Resources,
) -> View {
    macro_rules! attach {
        ($parent:expr) => {{
            let staged = $parent.child(a.keyed("a"));
            match (mode, attachment) {
                (0, _) => staged.into_child_view(),
                (1, Attachment::Setter) => staged.child(b.keyed("b")).into_child_view(),
                (1, Attachment::Checked) => staged
                    .with_children(Children::one(b.keyed("b")))
                    .unwrap()
                    .into_child_view(),
                (1, Attachment::Block) => harbor_widget::view! { staged => { b.keyed("b"); } },
                (_, Attachment::Block) => harbor_widget::view! { staged => {} },
                // Single-child builders have no separate empty-child setter.
                _ => staged
                    .with_children(Children::new())
                    .unwrap()
                    .into_child_view(),
            }
        }};
    }
    match parent {
        Parent::FocusScope => attach!(FocusScope::new()),
        Parent::ExternalSurface => attach!(parent_resources.surface(100)),
        Parent::Portal => attach!(Portal::new()),
    }
}

fn update(runtime: &mut Runtime) {
    runtime.update(Instant::now());
}

fn only_child(runtime: &Runtime) -> FiberId {
    let root = runtime.arena().get(runtime.root_id().unwrap()).unwrap();
    assert_eq!(root.children().len(), 1);
    root.children()[0]
}

#[test]
fn all_three_wrappers_teardown_replaced_and_cleared_subtrees_without_unmounting_parent() {
    for parent in [Parent::FocusScope, Parent::ExternalSurface, Parent::Portal] {
        for attachment in [Attachment::Setter, Attachment::Checked, Attachment::Block] {
            let a = TrackedChild::new(101);
            let b = TrackedChild::new(102);
            let parent_resources = Resources::default();
            let mode = Signal::new(0u8);
            let mut runtime = Runtime::new();
            runtime.set_viewport(harbor_widget::renderer::Viewport::new(300, 200, 1.0));
            runtime.set_root({
                let a = a.clone();
                let b = b.clone();
                let mode = mode.clone();
                let parent_resources = parent_resources.clone();
                move |cx: &mut BuildCx| {
                    cx.track(&mode);
                    description(
                        parent,
                        attachment,
                        *mode.read(),
                        a.clone(),
                        b.clone(),
                        &parent_resources,
                    )
                }
            });
            update(&mut runtime);
            let root = runtime.root_id().unwrap();
            let old_a = only_child(&runtime);
            assert_eq!(a.observed.get(), 11);
            assert_eq!(b.builds.get(), 0, "unselected content must remain deferred");
            assert!(a.hook_resource.borrow().upgrade().is_some());
            a.resources.assert_live(true);
            assert!(runtime.has_external_draws());

            // A same-key, same-type assignment preserves the mounted Fiber and hooks.
            let old_state = a.state.borrow().as_ref().unwrap().clone();
            old_state.set(20);
            update(&mut runtime);
            mode.set(0);
            update(&mut runtime);
            assert_eq!(only_child(&runtime), old_a);
            assert_eq!(a.observed.get(), 21);
            assert_eq!(a.resources.unmounts.get(), 0);

            mode.set(1);
            update(&mut runtime);
            let old_b = only_child(&runtime);
            assert_ne!(old_b, old_a);
            assert!(!runtime.arena().contains(old_a));
            assert!(a.hook_resource.borrow().upgrade().is_none());
            a.resources.assert_live(false);
            assert_eq!(a.resources.unmounts.get(), 1);
            assert_eq!(b.observed.get(), 11);
            b.resources.assert_live(true);
            let builds = a.builds.get();
            old_state.set(99);
            a.source.set(5);
            assert!(
                !runtime.update(Instant::now()).request_redraw,
                "removed subscriptions must not schedule tree work"
            );
            assert_eq!(
                a.builds.get(),
                builds,
                "removed subscriptions cannot rebuild a"
            );

            mode.set(2);
            update(&mut runtime);
            assert_eq!(runtime.root_id(), Some(root));
            assert!(runtime.arena().get(root).unwrap().children().is_empty());
            assert!(!runtime.arena().contains(old_b));
            assert!(b.hook_resource.borrow().upgrade().is_none());
            b.resources.assert_live(false);
            assert_eq!(b.resources.unmounts.get(), 1);
            let builds = b.builds.get();
            b.source.set(5);
            b.state.borrow().as_ref().unwrap().set(99);
            assert!(
                !runtime.update(Instant::now()).request_redraw,
                "cleared subscriptions must not schedule tree work"
            );
            assert_eq!(b.builds.get(), builds);
            assert_eq!(parent_resources.unmounts.get(), 0);
            if matches!(parent, Parent::ExternalSurface) {
                assert!(
                    runtime.has_external_draws(),
                    "the parent's draw registration stays live"
                );
                parent_resources.assert_live(true);
                let schedules = parent_resources.schedule_calls.get();
                let ime_queries = parent_resources.ime_calls.get();
                runtime.set_focus(root);
                update(&mut runtime);
                assert!(parent_resources.schedule_calls.get() > schedules);
                assert!(parent_resources.ime_calls.get() > ime_queries);
            } else {
                assert!(!runtime.has_external_draws());
            }

            // Reselecting removed content mounts a fresh hook sequence, not old state.
            mode.set(0);
            update(&mut runtime);
            assert_ne!(only_child(&runtime), old_a);
            assert_eq!(a.observed.get(), 15);
            a.resources.assert_live(true);
            runtime
                .set_root(harbor_widget::SizedBox::new(harbor_widget::layout::Size::ZERO).unwrap());
            update(&mut runtime);
            a.resources.assert_live(false);
            assert_eq!(a.resources.unmounts.get(), 2);
            if matches!(parent, Parent::ExternalSurface) {
                parent_resources.assert_live(false);
                assert_eq!(parent_resources.unmounts.get(), 1);
            }
        }
    }
}

#[test]
fn overwritten_unmounted_descriptions_never_build_or_call_unmount() {
    for parent in [Parent::FocusScope, Parent::ExternalSurface, Parent::Portal] {
        for attachment in [Attachment::Setter, Attachment::Checked, Attachment::Block] {
            let a = TrackedChild::new(101);
            let b = TrackedChild::new(102);
            let mut runtime = Runtime::new();
            let tree = description(
                parent,
                attachment,
                1,
                a.clone(),
                b.clone(),
                &Resources::default(),
            );
            assert_eq!(a.builds.get(), 0);
            assert_eq!(b.builds.get(), 0);
            runtime.set_root(tree);
            update(&mut runtime);
            assert_eq!(a.builds.get(), 0);
            assert_eq!(b.builds.get(), 1);
            runtime
                .set_root(harbor_widget::SizedBox::new(harbor_widget::layout::Size::ZERO).unwrap());
            update(&mut runtime);
            assert_eq!(a.resources.unmounts.get(), 0);
            assert_eq!(b.resources.unmounts.get(), 1);
        }
    }
}
