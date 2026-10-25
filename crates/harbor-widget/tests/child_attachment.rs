//! Public replacement and append contracts, including real Align slot ownership.
use harbor_widget::layout::Size;
use harbor_widget::runtime::Runtime;
use harbor_widget::view::{Key, View};
use harbor_widget::widgets::{align::Align, stack::Stack};
use harbor_widget::{Children, Column, ComponentExt, IntoChildView, Row, WithChildren, view};
use std::time::Instant;

fn child(key: &'static str) -> impl IntoChildView {
    harbor_widget::widgets::sized_box::SizedBox::new(Size::new(10.0, 10.0))
        .unwrap()
        .keyed(key)
}

fn mounted_keys(tree: View) -> Vec<Key> {
    let mut runtime = Runtime::new();
    runtime.set_root(tree);
    runtime.update(Instant::now());
    runtime
        .arena()
        .get(runtime.root_id().unwrap())
        .unwrap()
        .children()
        .iter()
        .map(|&id| runtime.arena().get(id).unwrap().key().unwrap().clone())
        .collect()
}

#[test]
fn multi_child_setters_and_blocks_replace_while_explicit_appends_preserve_order() {
    macro_rules! verify {
        ($parent:expr) => {{
            let staged = || $parent.push_child(child("a")).push_child(child("b"));
            let incoming = || Children::from_iter([child("c"), child("d")]);
            let expected = vec![Key::new("c"), Key::new("d")];
            assert_eq!(
                mounted_keys(staged().children(incoming()).into_child_view()),
                expected
            );
            assert_eq!(
                mounted_keys(view! { staged() => { child("c"); child("d"); } }),
                expected
            );
            assert_eq!(
                mounted_keys(
                    staged()
                        .with_children(incoming())
                        .unwrap()
                        .into_child_view()
                ),
                expected
            );
            assert_eq!(
                mounted_keys(staged().extend_children(incoming()).into_child_view()),
                vec![Key::new("a"), Key::new("b"), Key::new("c"), Key::new("d")]
            );
            assert_eq!(
                mounted_keys(staged().extend_children(Children::new()).into_child_view()),
                vec![Key::new("a"), Key::new("b")]
            );
            assert!(mounted_keys(staged().children(Children::new()).into_child_view()).is_empty());
            assert!(mounted_keys(view! { staged() => {} }).is_empty());
            assert_eq!(
                mounted_keys(view! { staged(); }),
                vec![Key::new("a"), Key::new("b")]
            );
        }};
    }
    verify!(Row::new());
    verify!(Column::new());
    verify!(Stack::new());
    verify!(harbor_widget::Flex::new(harbor_widget::Axis::Horizontal));
}

fn verify_single_child_contract<C>(parent: C)
where
    C: harbor_widget::view::Component
        + WithChildren<Capability = harbor_widget::SingleChild>
        + Clone
        + 'static,
{
    let staged = || {
        parent
            .clone()
            .with_children(Children::one(child("a")))
            .unwrap()
    };
    assert!(mounted_keys(parent.clone().into_child_view()).is_empty());
    assert_eq!(mounted_keys(view! { staged(); }), vec![Key::new("a")]);
    assert_eq!(
        mounted_keys(view! { staged() => { child("b"); } }),
        vec![Key::new("b")]
    );
    assert!(mounted_keys(view! { staged() => {} }).is_empty());

    // Key wrappers must preserve both the capability and the parent's identity.
    let mut runtime = Runtime::new();
    runtime.set_root(view! { staged().keyed("parent") => { child("b"); } });
    runtime.update(Instant::now());
    assert_eq!(
        runtime
            .arena()
            .get(runtime.root_id().unwrap())
            .unwrap()
            .key(),
        Some(&Key::new("parent"))
    );

    for has_staged_child in [false, true] {
        for count in 0..=2 {
            let make_parent = || {
                if has_staged_child {
                    staged()
                } else {
                    parent.clone()
                }
            };
            let incoming = (0..count).map(|_| child("b")).collect();
            match make_parent().with_children(incoming) {
                Ok(parent) => {
                    assert!(count <= 1);
                    assert_eq!(
                        mounted_keys(parent.into_child_view()),
                        if count == 0 {
                            vec![]
                        } else {
                            vec![Key::new("b")]
                        }
                    );
                }
                Err(error) => {
                    assert_eq!(count, 2);
                    assert_eq!(error.cardinality, harbor_widget::ChildCardinality::Single);
                    assert_eq!(error.received, 2);
                    assert!(!error.widget.is_empty());
                }
            }
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                view! { make_parent() => { for _ in 0..count { child("b"); } } }
            }));
            if count == 2 {
                assert!(outcome.is_err());
            } else {
                assert_eq!(
                    mounted_keys(outcome.unwrap()),
                    if count == 0 {
                        vec![]
                    } else {
                        vec![Key::new("b")]
                    }
                );
            }
        }
    }
}

// Keep this inventory aligned with every built-in WithChildren implementation.
// MultiChild widgets are covered above; leaves are compile-fail macro fixtures.
macro_rules! single_child_contract {
    ($name:ident, $parent:expr) => {
        #[test]
        fn $name() {
            let parent = $parent;
            verify_single_child_contract(parent.clone());
            assert_eq!(
                mounted_keys(parent.child(child("a")).child(child("b")).into_child_view()),
                vec![Key::new("b")]
            );
        }
    };
}

single_child_contract!(align, Align::new(harbor_widget::Alignment2D::CENTER));
single_child_contract!(padding, harbor_widget::Padding::all(1.0).unwrap());
single_child_contract!(
    sized_box,
    harbor_widget::SizedBox::new(Size::new(20.0, 20.0)).unwrap()
);
single_child_contract!(
    backdrop_filter,
    harbor_widget::BackdropFilter::new(2.0).unwrap()
);
single_child_contract!(button, harbor_widget::Button::new());
single_child_contract!(constrained_box, harbor_widget::ConstrainedBox::new());
single_child_contract!(
    custom_paint,
    harbor_widget::widgets::custom_paint::CustomPaint::new(1)
);
single_child_contract!(
    decorated_box,
    harbor_widget::DecoratedBox::new(Default::default())
);
single_child_contract!(flexible, harbor_widget::Flexible::new());
single_child_contract!(expanded, harbor_widget::Expanded::new());
single_child_contract!(focus_scope, harbor_widget::FocusScope::new());
single_child_contract!(interactive_region, harbor_widget::InteractiveRegion::new());
single_child_contract!(layout_observer, harbor_widget::LayoutObserver::new(|_| {}));
single_child_contract!(mouse_region, harbor_widget::MouseRegion::new());
single_child_contract!(portal, harbor_widget::Portal::new());
single_child_contract!(scroll_view, harbor_widget::ScrollView::new());
single_child_contract!(
    theme_provider,
    harbor_widget::ThemeProvider::new(Default::default()).expect("valid theme")
);
single_child_contract!(focus, harbor_widget::Focus::new());
single_child_contract!(shortcut_map, harbor_widget::ShortcutMap::<()>::new());
single_child_contract!(
    action_handler,
    harbor_widget::ActionHandler::<()>::new(|_| harbor_widget::ActionOutcome::Consumed)
);
