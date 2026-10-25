use harbor_widget::scene::primitive::Color;
use harbor_widget::widgets::text_label::TextLabel;
use harbor_widget::{
    Button, Column, ControlStyle, EditableText, Icon, IconButton, InteractiveRegion, LucideIcon,
    Separator, SizedBox, TextEditingController, TextField, Theme, ThemeProvider, VerticalDirection,
    view,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let theme = Theme::default();
    theme.validate()?;
    let style: ControlStyle = theme.button.clone();
    style.validate()?;
    let _ = InteractiveRegion::new()
        .style(style.clone())?
        .disabled(false);
    let _ = Button::new()
        .style(style)?
        .child(TextLabel::new("child").foreground(Color::WHITE)?);
    let button = IconButton::new(LucideIcon::Plus, "Plus")
        .style(theme.icon_button.clone())?
        .icon_size(0.0)?
        .foreground_with(|_| Color::RED);
    assert_eq!(button.get_label(), "Plus");
    let _ = ThemeProvider::new(theme)?.child(button);
    let _ = SizedBox::width(20.0)?.background(Color::WHITE)?;
    let _ = Separator::horizontal().thickness(0.0)?.color(Color::RED)?;
    let _ = Icon::new(LucideIcon::X)
        .size(20.0)?
        .foreground(Color::RED)?;
    let _ = harbor_widget::Align::new(harbor_widget::Alignment2D::CENTER).background(Color::RED)?;
    let _ = harbor_widget::Padding::all(0.0)?.background(Color::RED)?;
    let _ = harbor_widget::Row::new().background(Color::RED)?;
    let _ = Column::new().background(Color::RED)?;
    let _ = harbor_widget::Flex::new(harbor_widget::Axis::Horizontal).background(Color::RED)?;
    let _ = harbor_widget::widgets::stack::Stack::new().background(Color::RED)?;
    let c = TextEditingController::new("");
    let _ = EditableText::new(c.clone())
        .foreground(Color::WHITE)?
        .on_change(|_| {})
        .on_submit(|_| {})
        .on_vertical_navigation(|direction| match direction {
            VerticalDirection::Up | VerticalDirection::Down => {}
        });
    let _ = TextField::new(c)
        .padding(0.0)?
        .foreground(Color::WHITE)?
        .on_change(|_| {})
        .on_submit(|_| {})
        .on_vertical_navigation(|_: VerticalDirection| {});
    let _ = view! { Column::new() => {
        ThemeProvider::new(Theme::default()) => { Button::text("ok"); }
        Icon::new(LucideIcon::X).foreground(Color::RED);
        SizedBox::width(20.0).unwrap().background(Color::RED).expect("finite background");
        Button::text("styled").style(Theme::default().button);
    }};
    Ok(())
}
