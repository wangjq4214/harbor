#[path = "../support/capabilities.rs"]
mod capabilities;
use capabilities::{ExternalMulti, ExternalSingle};
use harbor_widget::view::{BuildCx, Component, View};
use harbor_widget::widgets::text_label::TextLabel;
use harbor_widget::{
    Children, ComponentExt, FlexOptions, Padding, Row, SizedBox, WithChildren, view,
};

fn main() {
    let value: View = view! { SizedBox::width(440.0); };
    let _ = view! { Ok::<_, &str>(value); };
    let existing = TextLabel::new("existing")
        .keyed("existing")
        .build(&mut BuildCx::stub());
    let _ = view! { Row::with_options(FlexOptions::default()) => {
        Ok::<_, &str>(existing);
        SizedBox::height(20.0) => { TextLabel::new("child"); }
        Ok::<_, &str>(ExternalSingle(Padding::all(0.0).unwrap())) => {}
        Ok::<_, &str>(ExternalMulti(harbor_widget::Column::new())) => {
            for i in 0..2 { SizedBox::width(i as f32); }
        }
    }};
    let enabled = std::env::args().len() > 1;
    let _ = view! { Padding::all(0.0) => {
        if enabled { SizedBox::width(2.0); } else { SizedBox::height(3.0); }
    }};
    let _ = view! { SizedBox::width(10.0) => {
        match enabled { true => { TextLabel::new("yes"); }, false => {} }
    }};
    let staged = SizedBox::width(10.0).unwrap().child(TextLabel::new("old"));
    let _ = view! { staged => {} };
    let _ = view! { (if enabled { SizedBox::width(1.0) } else { SizedBox::height(1.0) }); };
    let _ = SizedBox::width(10.0)
        .unwrap()
        .with_children(Children::new())
        .unwrap();
}
