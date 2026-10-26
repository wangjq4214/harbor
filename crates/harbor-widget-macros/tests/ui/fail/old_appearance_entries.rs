use harbor_widget::scene::primitive::Color;
use harbor_widget::widgets::text_label::TextLabel;
use harbor_widget::{
    EditableText, Icon, IconButton, LucideIcon, SizedBox, TextEditingController, TextField,
};

fn main() {
    let _ = SizedBox::width(10.0).unwrap().color(Color::RED);
    let _ = TextLabel::new("x").color(Color::RED);
    let _ = Icon::new(LucideIcon::X).color(Color::RED);
    let c = TextEditingController::new("");
    let _ = EditableText::new(c.clone()).color(Color::RED);
    let _ = TextField::new(c.clone()).color(Color::RED);
    let _ = EditableText::new(c.clone()).on_changed(|_| {});
    let _ = TextField::new(c.clone()).on_changed(|_| {});
    let _ = EditableText::new(c.clone()).on_submitted(|_| {});
    let _ = TextField::new(c).on_submitted(|_| {});
    let _ = IconButton::new(LucideIcon::X, "x").color_with(|_| Color::RED);
    let _ = IconButton::new(LucideIcon::X, "x").label();
}
