use harbor_widget::layout::{Alignment, Size};
use harbor_widget::widgets::{align::Align, padding::Padding, sized_box::SizedBox};
use harbor_widget::{Column, ComponentExt, view};
fn leaf() -> SizedBox {
    SizedBox::new(Size::ZERO)
}
fn main() {
    let _ = view! { Padding::all(1.0) => { leaf(); leaf(); } };
    let _ = view! { Column::new() => {
        Align::new(Alignment::Center).keyed("align") => { leaf(); leaf(); }
    }};
    let enabled = std::env::args().len() > 1;
    let _ = view! { Padding::all(1.0) => {
        if enabled { leaf(); leaf(); } else { leaf(); }
    }};
    let _ = view! { Padding::all(1.0) => {
        leaf(); for _ in 0..usize::from(enabled) { leaf(); } leaf();
    }};
}
