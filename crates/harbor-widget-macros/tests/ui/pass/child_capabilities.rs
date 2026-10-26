#[path = "../support/capabilities.rs"]
mod capabilities;
use capabilities::{ExternalMulti, ExternalSingle};
use harbor_widget::layout::Size;
use harbor_widget::widgets::{column::Column, padding::Padding, sized_box::SizedBox};
use harbor_widget::{Children, ComponentExt, WithChildren, view};
fn leaf() -> SizedBox {
    SizedBox::new(Size::ZERO).unwrap()
}
fn main() {
    let enabled = std::env::args().len() > 1;
    let _ = view! { ExternalSingle(Padding::all(1.0).unwrap()).keyed("single") => {
        if enabled { leaf(); } else if !enabled {} else { leaf(); }
    }};
    let _ = view! { ExternalSingle(Padding::all(1.0).unwrap()) => {
        match enabled { true => { leaf(); }, false => {} }
    }};
    let _ = view! { ExternalSingle(Padding::all(1.0).unwrap()) => {} };
    let _ = view! { ExternalMulti(Column::new()).keyed("multi") => {
        leaf();
        ExternalSingle(Padding::all(1.0).unwrap()) => { leaf(); }
        leaf();
    }};
    let _ = view! { ExternalSingle(Padding::all(1.0).unwrap()) => {
        for _ in 0..usize::from(enabled) { leaf(); }
    }};
    let _ = ExternalSingle(Padding::all(1.0).unwrap())
        .with_children(Children::one(leaf()))
        .unwrap();
}
