use harbor_widget::scene::primitive::Color;
use harbor_widget::signal::Signal;
use harbor_widget::{
    Button, EditableText, Focus, FocusHandle, IconButton, LucideIcon, MouseRegion,
    NotificationCard, TextEditingController, TextField,
};
use std::cell::Cell;
use std::rc::Rc;

fn main() {
    let local = Signal::new(0u32);
    let _ = Button::text("Local")
        .focus_handle(FocusHandle::new())
        .on_click(move |_| local.set(1));
    let local = Rc::new(Cell::new(0));
    let click = local.clone();
    let color = local.clone();
    let enter = local.clone();
    let exit = local.clone();
    let _ = IconButton::new(LucideIcon::Plus, "Local")
        .focus_handle(FocusHandle::new())
        .on_click(move |_| click.set(1))
        .foreground_with(move |_| {
            color.set(2);
            Color::RED
        });
    let _ = MouseRegion::new()
        .on_enter(move |_| enter.set(3))
        .on_exit(move |_| exit.set(4));
    let _ = NotificationCard::new("Local", move || local.set(5));
    let _ = TextField::new(TextEditingController::new(""))
        .disabled(true)
        .disabled(false);
    let _ = EditableText::new(TextEditingController::new(""))
        .disabled(true)
        .disabled(false);
    let _ = Focus::new().focusable(true); // unrelated positive API remains
}
