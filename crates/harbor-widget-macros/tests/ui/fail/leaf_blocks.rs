use harbor_widget::layout::Size;
use harbor_widget::widgets::sized_box::SizedBox;
use harbor_widget::{Button, Column, ComponentExt, Icon, IconButton, LucideIcon, view};
fn leaf() -> SizedBox {
    SizedBox::new(Size::ZERO)
}
fn main() {
    let _ = view! { leaf() => {} };
    let _ = view! { Button::new("leaf") => { leaf(); } };
    let _ = view! { Icon::new(LucideIcon::X) => { for _ in 0..2 { leaf(); } } };
    let _ =
        view! { Column::new() => { IconButton::new(LucideIcon::X, "Close").keyed("leaf") => {} } };
}
