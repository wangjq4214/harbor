use harbor_widget::layout::Size;
use harbor_widget::view::{BuildCx, Component, View};
use harbor_widget::widgets::column::Column;
use harbor_widget::widgets::sized_box::SizedBox;
use harbor_widget::view;

#[derive(Clone)]
struct Content;

impl Component for Content {
    fn build(&self, cx: &mut BuildCx) -> View {
        SizedBox::new(Size::new(1.0, 1.0)).build(cx)
    }
}

fn main() {
    let mut cx = BuildCx::stub();

    // An already-built view is accepted both as a child and as the root;
    // both positions pass it through unchanged.
    let existing = SizedBox::new(Size::new(2.0, 2.0)).build(&mut cx);
    let as_root = view! { existing; };
    let _: View = as_root;

    let child = SizedBox::new(Size::new(3.0, 3.0)).build(&mut cx);
    let wrapped = view! {
        Column::new() => {
            child;
            Content;
        }
    };
    let _: View = wrapped;
}
