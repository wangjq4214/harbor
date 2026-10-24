use harbor_widget::layout::Size;
use harbor_widget::view;
use harbor_widget::view::{BuildCx, Component, View};
use harbor_widget::widgets::column::Column;
use harbor_widget::widgets::sized_box::SizedBox;

#[derive(Clone)]
struct Leaf;

impl Component for Leaf {
    fn build(&self, cx: &mut BuildCx) -> View {
        SizedBox::new(Size::new(1.0, 1.0)).unwrap().build(cx)
    }
}

fn main() {
    let leaf = Leaf;
    let _ = view! {  Column::new() => { Ok::<_, &str>(leaf); } };
    let _used_after_move = leaf;
}
