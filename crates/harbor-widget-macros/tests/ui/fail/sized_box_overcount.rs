use harbor_widget::widgets::text_label::TextLabel;
use harbor_widget::{Column, SizedBox, view};
fn main() {
    let _ = view! { SizedBox::width(10.0) => {
        TextLabel::new("a"); TextLabel::new("b");
    }};
    let _ = view! { Column::new() => {
        Ok::<_, &str>(SizedBox::height(20.0).unwrap()) => {
            TextLabel::new("a"); TextLabel::new("b");
        }
    }};
}
