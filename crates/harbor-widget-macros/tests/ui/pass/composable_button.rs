use harbor_widget::view::{BuildCx, Component};
use harbor_widget::widgets::text_label::TextLabel;
use harbor_widget::{Button, Column, IntoChildView, LucideIcon, view};
fn main() {
    let _ = Button::new();
    let _ = Button::text("Save");
    let _ = Button::icon(LucideIcon::Plus, "New");
    let _ = Button::new().child(TextLabel::new("Custom").into_child_view());
    let _ = Button::text("Old")
        .child_with(|cx: &mut BuildCx, state| {
            TextLabel::new(if state.pressed { "Pressed" } else { "Ready" }).build(cx)
        })
        .child(TextLabel::new("Replacement"));
    let _ = view! { Column::new() => {
        Button::text("Old") => { TextLabel::new("Replacement"); }
        Button::icon(LucideIcon::Plus, "Cleared") => {}
        Button::new().child_with(|cx, _| TextLabel::new("Cleared").build(cx)) => {}
    }};
}
