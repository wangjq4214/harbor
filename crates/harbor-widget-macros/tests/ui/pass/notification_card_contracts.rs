use harbor_widget::widgets::notification_card::{NotificationCardPlacement, NotificationCardStyle};
use harbor_widget::{
    Alignment2D, CrossAxisAlignment, NotificationCard, PositionAlignment, Row, TextLabel, view,
};

fn main() {
    let _ = Row::new()
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .push_child(TextLabel::new("stretch"));
    let position = Alignment2D::BOTTOM_RIGHT;
    let card = NotificationCard::new("detail", || {})
        .detail_foreground(harbor_widget::scene::primitive::Color::WHITE)
        .unwrap()
        .close_alignment(PositionAlignment::Start)
        .placement(NotificationCardPlacement {
            horizontal: position.horizontal,
            vertical: position.vertical,
            inset: 12.0,
        })
        .unwrap();
    let _ = view! { card.style(NotificationCardStyle::default()); };
}
