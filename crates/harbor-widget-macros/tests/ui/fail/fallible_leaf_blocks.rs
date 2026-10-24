use harbor_widget::widgets::text_label::TextLabel;
use harbor_widget::{Column, view};
fn main() {
    let _ = view! { Ok::<_, &str>(TextLabel::new("leaf")) => {} };
    let _ = view! { Column::new() => {
        Ok::<_, &str>(TextLabel::new("leaf")) => { TextLabel::new("child"); }
    }};
}
