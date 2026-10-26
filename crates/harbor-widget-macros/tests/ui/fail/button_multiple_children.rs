use harbor_widget::{Button, view};
use harbor_widget::widgets::text_label::TextLabel;
fn main() {
    let _ = view! { Button::new() => { TextLabel::new("A"); TextLabel::new("B"); } };
}
