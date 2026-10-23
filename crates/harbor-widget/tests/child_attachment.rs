//! Public replacement and append contracts, including real Align slot ownership.
use harbor_widget::layout::{Alignment, Size};
use harbor_widget::runtime::Runtime;
use harbor_widget::view::{Key, View};
use harbor_widget::widgets::{align::Align, stack::Stack};
use harbor_widget::{Children, Column, ComponentExt, IntoChildView, Row, WithChildren, view};
use std::time::Instant;

fn child(key: &'static str) -> impl IntoChildView {
    harbor_widget::widgets::sized_box::SizedBox::new(Size::new(10.0, 10.0)).keyed(key)
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

#[test]
fn align_owns_one_replaceable_slot_and_rejects_dynamic_overcount() {
    let staged = || Align::new(Alignment::Center).child(child("a"));
    assert_eq!(
        mounted_keys(staged().child(child("b")).into_child_view()),
        vec![Key::new("b")]
    );
    assert_eq!(
        mounted_keys(view! { staged() => { child("b"); } }),
        vec![Key::new("b")]
    );
    assert!(mounted_keys(view! { staged() => {} }).is_empty());
    for count in 0..=2 {
        let incoming = (0..count).map(|_| child("b")).collect();
        match staged().with_children(incoming) {
            Ok(parent) => assert_eq!(mounted_keys(parent.into_child_view()).len(), count),
            Err(error) => {
                assert_eq!(count, 2);
                assert_eq!(error.widget, "Align");
                assert_eq!(error.received, 2);
            }
        }
        let outcome = std::panic::catch_unwind(|| {
            view! {
                staged() => { for _ in 0..count { child("b"); } }
            }
        });
        assert_eq!(outcome.is_err(), count == 2);
    }
}
