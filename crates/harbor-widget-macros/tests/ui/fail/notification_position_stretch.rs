use harbor_widget::widgets::notification_card::NotificationCardPlacement;
use harbor_widget::{CrossAxisAlignment, NotificationCard, PositionAlignment};
fn main() {
    let _ = NotificationCardPlacement {
        horizontal: CrossAxisAlignment::Stretch,
        vertical: PositionAlignment::Start,
        inset: 0.0,
    };
    let _ = NotificationCardPlacement {
        horizontal: PositionAlignment::Start,
        vertical: CrossAxisAlignment::Stretch,
        inset: 0.0,
    };
    let _ = NotificationCard::new("", || {}).close_alignment(CrossAxisAlignment::Stretch);
    let _ = NotificationCard::new("", || {}).close_alignment(CrossAxisAlignment::Stretch.into());
    let _ = PositionAlignment::Stretch;
}
