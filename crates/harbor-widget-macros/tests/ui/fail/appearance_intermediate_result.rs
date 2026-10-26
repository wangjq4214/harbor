use harbor_widget::scene::primitive::Color;
use harbor_widget::{
    Icon, LucideIcon, TextEditingController, TextField, Theme, ThemeProvider, view,
};

fn main() {
    let _ = view! { Icon::new(LucideIcon::X).size(20.0).foreground(Color::RED); };
    let _ = ThemeProvider::new(Theme::default()).child(Icon::new(LucideIcon::X));
    let _ = TextField::new(TextEditingController::new("")).on_vertical_navigation(|_: bool| {});
}
