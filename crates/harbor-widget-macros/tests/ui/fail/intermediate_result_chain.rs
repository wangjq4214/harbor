use harbor_widget::widgets::text_label::TextLabel;
use harbor_widget::{SizedBox, view};
fn main() {
    // Only a final Result is resolved, not an intermediate receiver.
    let _ = view! { SizedBox::width(10.0).child(TextLabel::new("child")); };
}
