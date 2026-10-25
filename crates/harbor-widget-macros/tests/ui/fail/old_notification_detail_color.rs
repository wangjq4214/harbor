use harbor_widget::{NotificationCard, scene::primitive::Color};
fn main() {
    let _ = NotificationCard::new("detail", || {}).detail_color(Color::WHITE);
}
