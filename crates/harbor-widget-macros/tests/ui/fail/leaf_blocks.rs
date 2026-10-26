use harbor_widget::widgets::text_label::TextLabel;
use harbor_widget::{Column, ComponentExt, Icon, IconButton, LucideIcon, view};
fn leaf() -> TextLabel {
    TextLabel::new("leaf")
}
fn main() {
    let _ = view! { leaf() => {} };
    let _ = view! { leaf() => { leaf(); } };
    let _ = view! { Icon::new(LucideIcon::X) => { for _ in 0..2 { leaf(); } } };
    let _ =
        view! { Column::new() => { IconButton::new(LucideIcon::X, "Close").keyed("leaf") => {} } };
}
