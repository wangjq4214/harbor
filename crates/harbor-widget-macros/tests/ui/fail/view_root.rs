use harbor_widget::layout::Size;
use harbor_widget::view::{BuildCx, Component};
use harbor_widget::widgets::sized_box::SizedBox;
use harbor_widget::view;

fn main() {
    let mut cx = BuildCx::stub();
    let existing = SizedBox::new(Size::new(1.0, 1.0)).build(&mut cx);
    let _ = view! { &mut cx; existing; };
}
