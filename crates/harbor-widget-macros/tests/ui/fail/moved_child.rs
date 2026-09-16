use harbor_widget::layout::Size;
use harbor_widget::view::{BuildCx, Component, View};
use harbor_widget::widgets::column::Column;
use harbor_widget::widgets::sized_box::SizedBox;
use harbor_widget::view;

#[derive(Clone)]
struct Leaf;

impl Component for Leaf {
    fn build(&self, cx: &mut BuildCx) -> View {
        SizedBox::new(Size::new(1.0, 1.0)).build(cx)
    }
}

fn main() {
    let mut cx = BuildCx::stub();
    let leaf = Leaf;
    let _ = view! { &mut cx; Column::new() => { leaf; } };
    let _used_after_move = leaf;
}
