use harbor_widget::scene::primitive::Color;
use harbor_widget::widgets::stack::Stack;
use harbor_widget::{Align, Alignment2D, Axis, Flex, Padding, SizedBox};

fn main() {
    SizedBox::width(1.0).unwrap().background = Some(Color::RED);
    Align::new(Alignment2D::CENTER).background = Some(Color::RED);
    Padding::all(0.0).unwrap().background = Some(Color::RED);
    Flex::new(Axis::Horizontal).background = Some(Color::RED);
    Stack::new().background = Some(Color::RED);
}
