use harbor_widget::{Axis, Column, DecoratedBox, Flex, Padding, Row, ScrollController, SizedBox};
fn main() {
    let _ = Padding::all(1.0).unwrap().insets();
    let _ = DecoratedBox::new(Default::default()).decoration();
    let _ = DecoratedBox::new(Default::default()).clip_behavior_value();
    let _ = Row::new().options();
    let _ = Column::new().options();
    let _ = Flex::new(Axis::Vertical).options();
    let _ = SizedBox::width(1.0).unwrap().requested_width();
    let _ = SizedBox::height(1.0).unwrap().requested_height();
    let _ = ScrollController::new().offset();
}
