use harbor_widget::layout::Size;
use harbor_widget::widgets::column::Column;
use harbor_widget::widgets::sized_box::SizedBox;
use harbor_widget::view;

fn main() {
    let _ = view! {
        Column::new() => SizedBox::new(Size::new(1.0, 1.0));
    };
}
