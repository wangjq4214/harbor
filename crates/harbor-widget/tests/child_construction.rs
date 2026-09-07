use harbor_widget::layout::Size;
use harbor_widget::runtime::Runtime;
use harbor_widget::signal::Signal;
use harbor_widget::view::{BuildCx, Component, Key, View};
use harbor_widget::widgets::button::Button;
use harbor_widget::widgets::column::Column;
use harbor_widget::widgets::padding::Padding;
use harbor_widget::widgets::sized_box::SizedBox;
use harbor_widget::{
    ChildCardinality, Children, ComponentExt, IntoChildView, IntoChildren, Keyed, WithChildren,
};
use std::any::TypeId;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

/// This component can be implemented by an external crate because it only
/// builds with public Components and never names harbor-widget's `AnyView`.
#[derive(Clone)]
struct PublicComponent;

impl Component for PublicComponent {
    fn build(&self, cx: &mut BuildCx) -> View {
        SizedBox::new(Size::new(10.0, 10.0)).build(cx)
    }
}

#[test]
fn keyed_public_component_is_deferred_with_a_visible_key() {
    let view = PublicComponent.keyed("public-component").into_child_view();

    assert_eq!(view.key(), Some(&Key::new("public-component")));
    assert_eq!(
        view.widget_type(),
        TypeId::of::<Keyed<PublicComponent>>(),
        "the key wrapper is the deferred Fiber identity",
    );
}

#[test]
fn children_accept_components_and_existing_views_in_source_order() {
    let mut build_cx = BuildCx::stub();
    let existing_view = SizedBox::new(Size::new(20.0, 10.0)).build(&mut build_cx);
    let mut children = Children::one(PublicComponent.keyed("first"));
    children.push(existing_view);
    children.extend([PublicComponent.keyed("third")]);

    let mut runtime = Runtime::new();
    runtime.set_root(Column::new().with_children(children).unwrap());
    runtime.update(Instant::now());

    let root = runtime.arena().get(runtime.root_id().unwrap()).unwrap();
    let child_ids = root.children();
    assert_eq!(child_ids.len(), 3);
    assert_eq!(
        runtime.arena().get(child_ids[0]).unwrap().widget_type(),
        TypeId::of::<Keyed<PublicComponent>>()
    );
    assert_eq!(
        runtime.arena().get(child_ids[1]).unwrap().widget_type(),
        TypeId::of::<SizedBox>()
    );
    assert_eq!(
        runtime.arena().get(child_ids[2]).unwrap().widget_type(),
        TypeId::of::<Keyed<PublicComponent>>()
    );
}

#[test]
fn into_children_normalizes_single_values_and_collections_in_order() {
    let mut build_cx = BuildCx::stub();
    let existing_view = SizedBox::new(Size::new(20.0, 10.0)).build(&mut build_cx);
    let mut children = Children::new();

    IntoChildren::append_to(PublicComponent.keyed("first"), &mut children);
    IntoChildren::append_to(existing_view, &mut children);
    IntoChildren::append_to(Children::new(), &mut children);
    IntoChildren::append_to(Children::one(PublicComponent.keyed("third")), &mut children);

    let mut runtime = Runtime::new();
    runtime.set_root(Column::new().with_children(children).unwrap());
    runtime.update(Instant::now());

    let root = runtime.arena().get(runtime.root_id().unwrap()).unwrap();
    let child_ids = root.children();
    assert_eq!(child_ids.len(), 3);
    assert_eq!(
        runtime.arena().get(child_ids[0]).unwrap().widget_type(),
        TypeId::of::<Keyed<PublicComponent>>()
    );
    assert_eq!(
        runtime.arena().get(child_ids[1]).unwrap().widget_type(),
        TypeId::of::<SizedBox>()
    );
    assert_eq!(
        runtime.arena().get(child_ids[2]).unwrap().widget_type(),
        TypeId::of::<Keyed<PublicComponent>>()
    );
}

#[test]
fn with_children_reports_single_and_leaf_cardinality_violations() {
    let single_error = match Padding::all(1.0)
        .child(SizedBox::new(Size::new(1.0, 1.0)))
        .with_children(Children::one(SizedBox::new(Size::new(1.0, 1.0))))
    {
        Err(error) => error,
        Ok(_) => panic!("single-child attachment should reject a second child"),
    };
    assert_eq!(single_error.widget, "Padding");
    assert_eq!(single_error.cardinality, ChildCardinality::Single);
    assert_eq!(single_error.received, 2);

    let leaf_error =
        match SizedBox::new(Size::new(1.0, 1.0)).with_children(Children::one(PublicComponent)) {
            Err(error) => error,
            Ok(_) => panic!("leaf attachment should reject children"),
        };
    assert_eq!(leaf_error.widget, "SizedBox");
    assert_eq!(leaf_error.cardinality, ChildCardinality::Leaf);
    assert_eq!(leaf_error.received, 1);
}

#[derive(Clone)]
struct StatefulButton {
    label: &'static str,
    initial: u32,
    signal_out: Rc<RefCell<Option<Signal<u32>>>>,
    observed: Rc<RefCell<Vec<u32>>>,
}

impl StatefulButton {
    fn new(
        label: &'static str,
        initial: u32,
        signal_out: Rc<RefCell<Option<Signal<u32>>>>,
        observed: Rc<RefCell<Vec<u32>>>,
    ) -> Self {
        Self {
            label,
            initial,
            signal_out,
            observed,
        }
    }
}

impl Component for StatefulButton {
    fn build(&self, cx: &mut BuildCx) -> View {
        let state = cx.use_state(|| self.initial);
        *self.signal_out.borrow_mut() = Some(state.clone());
        self.observed.borrow_mut().push(*state.read());
        Button::new(self.label).build(cx)
    }
}

type TabItems = Vec<(&'static str, StatefulButton)>;
type TabItemsSignal = Rc<RefCell<Option<Signal<TabItems>>>>;

#[derive(Clone)]
struct TabList {
    items: TabItemsSignal,
    initial: TabItems,
}

impl TabList {
    fn new(initial: TabItems) -> (Self, TabItemsSignal) {
        let holder = Rc::new(RefCell::new(None));
        (
            Self {
                items: holder.clone(),
                initial,
            },
            holder,
        )
    }
}

impl Component for TabList {
    fn build(&self, cx: &mut BuildCx) -> View {
        let state = cx.use_state(|| self.initial.clone());
        *self.items.borrow_mut() = Some(state.clone());
        let current_items = state.read();
        let mut children = Children::new();
        for (key, child) in current_items.iter() {
            children.push(child.clone().keyed(*key));
        }
        Column::new().with_children(children).unwrap().build(cx)
    }
}

#[test]
fn keyed_children_middle_removal_preserves_later_fiber_hook_and_focus() {
    let sig_a = Rc::new(RefCell::new(None));
    let obs_a = Rc::new(RefCell::new(Vec::new()));
    let child_a = StatefulButton::new("A", 10, sig_a, obs_a);

    let sig_b = Rc::new(RefCell::new(None));
    let obs_b = Rc::new(RefCell::new(Vec::new()));
    let child_b = StatefulButton::new("B", 20, sig_b, obs_b);

    let sig_c = Rc::new(RefCell::new(None));
    let obs_c = Rc::new(RefCell::new(Vec::new()));
    let child_c = StatefulButton::new("C", 30, sig_c.clone(), obs_c.clone());

    let (tabs, tabs_signal) = TabList::new(vec![
        ("a", child_a.clone()),
        ("b", child_b),
        ("c", child_c.clone()),
    ]);

    let mut rt = Runtime::new();
    rt.set_root(tabs);
    rt.update(Instant::now());

    let root_id = rt.root_id().unwrap();
    let initial_child_ids = rt.arena().get(root_id).unwrap().children().to_vec();
    assert_eq!(initial_child_ids.len(), 3);
    let (fiber_a, fiber_b, fiber_c) = (
        initial_child_ids[0],
        initial_child_ids[1],
        initial_child_ids[2],
    );

    assert!(rt.arena().get(fiber_c).unwrap().is_focusable());
    rt.set_focus(fiber_c);
    assert_eq!(rt.input().focused(), Some(fiber_c));

    // Mutate state of child C
    sig_c.borrow().as_ref().unwrap().set(35);
    rt.update(Instant::now());
    assert_eq!(obs_c.borrow().as_slice(), &[30, 35]);

    // Remove middle child B: ["a", "c"]
    tabs_signal
        .borrow()
        .as_ref()
        .unwrap()
        .set(vec![("a", child_a), ("c", child_c)]);
    rt.update(Instant::now());

    let updated_child_ids = rt.arena().get(root_id).unwrap().children().to_vec();
    assert_eq!(updated_child_ids, vec![fiber_a, fiber_c]);
    assert!(rt.arena().contains(fiber_a), "fiber A must be retained");
    assert!(
        !rt.arena().contains(fiber_b),
        "fiber B must be unmounted from arena"
    );
    assert!(
        rt.arena().contains(fiber_c),
        "fiber C must be retained across position shift"
    );
    assert_eq!(
        *obs_c.borrow().last().unwrap(),
        35,
        "fiber C's hook state must be preserved across middle removal"
    );
    assert_eq!(
        rt.input().focused(),
        Some(fiber_c),
        "focus target on fiber C must be preserved across middle removal"
    );
}

#[test]
fn closing_focused_keyed_child_clears_focus_deterministically() {
    let sig_a = Rc::new(RefCell::new(None));
    let obs_a = Rc::new(RefCell::new(Vec::new()));
    let child_a = StatefulButton::new("A", 10, sig_a, obs_a);

    let sig_b = Rc::new(RefCell::new(None));
    let obs_b = Rc::new(RefCell::new(Vec::new()));
    let child_b = StatefulButton::new("B", 20, sig_b, obs_b);

    let sig_c = Rc::new(RefCell::new(None));
    let obs_c = Rc::new(RefCell::new(Vec::new()));
    let child_c = StatefulButton::new("C", 30, sig_c, obs_c);

    let (tabs, tabs_signal) = TabList::new(vec![
        ("a", child_a.clone()),
        ("b", child_b),
        ("c", child_c.clone()),
    ]);

    let mut rt = Runtime::new();
    rt.set_root(tabs);
    rt.update(Instant::now());

    let root_id = rt.root_id().unwrap();
    let initial_child_ids = rt.arena().get(root_id).unwrap().children().to_vec();
    let fiber_b = initial_child_ids[1];

    rt.set_focus(fiber_b);
    assert_eq!(rt.input().focused(), Some(fiber_b));

    // Remove focused child B: ["a", "c"]
    tabs_signal
        .borrow()
        .as_ref()
        .unwrap()
        .set(vec![("a", child_a), ("c", child_c)]);
    rt.update(Instant::now());

    assert!(
        !rt.arena().contains(fiber_b),
        "unmounted fiber B must not exist in arena"
    );
    assert_eq!(
        rt.input().focused(),
        None,
        "focus pointing to unmounted fiber B must be cleared, not routed to sibling"
    );
}

#[test]
fn keyed_children_reorder_preserves_state_per_key_without_migration() {
    let sig_a = Rc::new(RefCell::new(None));
    let obs_a = Rc::new(RefCell::new(Vec::new()));
    let child_a = StatefulButton::new("A", 100, sig_a.clone(), obs_a.clone());

    let sig_b = Rc::new(RefCell::new(None));
    let obs_b = Rc::new(RefCell::new(Vec::new()));
    let child_b = StatefulButton::new("B", 200, sig_b.clone(), obs_b.clone());

    let sig_c = Rc::new(RefCell::new(None));
    let obs_c = Rc::new(RefCell::new(Vec::new()));
    let child_c = StatefulButton::new("C", 300, sig_c.clone(), obs_c.clone());

    let (tabs, tabs_signal) = TabList::new(vec![
        ("a", child_a.clone()),
        ("b", child_b.clone()),
        ("c", child_c.clone()),
    ]);

    let mut rt = Runtime::new();
    rt.set_root(tabs);
    rt.update(Instant::now());

    let root_id = rt.root_id().unwrap();
    let old_ids = rt.arena().get(root_id).unwrap().children().to_vec();
    let (fiber_a, fiber_b, fiber_c) = (old_ids[0], old_ids[1], old_ids[2]);

    // Mutate state for each child: A -> 101, B -> 202, C -> 303
    sig_a.borrow().as_ref().unwrap().set(101);
    sig_b.borrow().as_ref().unwrap().set(202);
    sig_c.borrow().as_ref().unwrap().set(303);
    rt.update(Instant::now());

    // Reorder from [A, B, C] to [C, A, B]
    tabs_signal.borrow().as_ref().unwrap().set(vec![
        ("c", child_c),
        ("a", child_a),
        ("b", child_b),
    ]);
    rt.update(Instant::now());

    let new_ids = rt.arena().get(root_id).unwrap().children().to_vec();
    assert_eq!(
        new_ids,
        vec![fiber_c, fiber_a, fiber_b],
        "fibers must be reordered according to key source order"
    );

    // Assert states remained with their respective keys and did not migrate by position
    assert_eq!(*obs_a.borrow().last().unwrap(), 101);
    assert_eq!(*obs_b.borrow().last().unwrap(), 202);
    assert_eq!(*obs_c.borrow().last().unwrap(), 303);
}

#[test]
fn macro_ready_traits_produce_identical_view_tree_to_fluent_builder() {
    let fluent_tree = Column::new()
        .child(Padding::all(4.0).child(SizedBox::new(Size::new(10.0, 20.0))))
        .child(Button::new("Click"));

    let macro_tree = Column::new()
        .with_children(Children::from_iter([
            Padding::all(4.0)
                .with_children(Children::one(SizedBox::new(Size::new(10.0, 20.0))))
                .unwrap()
                .into_child_view(),
            Button::new("Click").into_child_view(),
        ]))
        .unwrap();

    let mut rt_fluent = Runtime::new();
    rt_fluent.set_root(fluent_tree);
    rt_fluent.update(Instant::now());

    let mut rt_macro = Runtime::new();
    rt_macro.set_root(macro_tree);
    rt_macro.update(Instant::now());

    let fluent_root = rt_fluent.arena().get(rt_fluent.root_id().unwrap()).unwrap();
    let macro_root = rt_macro.arena().get(rt_macro.root_id().unwrap()).unwrap();

    assert_eq!(fluent_root.children().len(), macro_root.children().len());
    assert_eq!(fluent_root.layout_rect(), macro_root.layout_rect());

    for (&f_id, &m_id) in fluent_root
        .children()
        .iter()
        .zip(macro_root.children().iter())
    {
        let f_child = rt_fluent.arena().get(f_id).unwrap();
        let m_child = rt_macro.arena().get(m_id).unwrap();
        assert_eq!(f_child.widget_type(), m_child.widget_type());
        assert_eq!(f_child.layout_rect(), m_child.layout_rect());
    }
}
