#[path = "../support/capabilities.rs"]
mod capabilities;
use capabilities::ExternalSingle;
use harbor_widget::layout::Size;
use harbor_widget::widgets::{padding::Padding, sized_box::SizedBox};
use harbor_widget::{ComponentExt, view};
fn main() {
    let _ = view! { ExternalSingle(Padding::all(1.0)).keyed("external") => {
        SizedBox::new(Size::ZERO);
        SizedBox::new(Size::ZERO);
    }};
}
