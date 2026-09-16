#![allow(unused_parens)]

use harbor_widget::layout::Size;
use harbor_widget::view::{BuildCx, Component, View};
use harbor_widget::widgets::column::Column;
use harbor_widget::widgets::sized_box::SizedBox;
use harbor_widget::view;

#[derive(Clone)]
struct StructLeaf {
    size: Size,
}

impl Component for StructLeaf {
    fn build(&self, cx: &mut BuildCx) -> View {
        SizedBox::new(self.size).build(cx)
    }
}

fn main() {
    let mut cx = BuildCx::stub();
    let enabled = true;
    let choice = Some(Size::new(3.0, 4.0));

    let _ = view! {
        &mut cx;
        Column::new() => {
            SizedBox::new(Size::new(1.0, 2.0)).color(harbor_widget::scene::primitive::Color::RED);
            StructLeaf { size: Size::new(2.0, 2.0) };
            {
                let size = Size::new(2.0, 3.0);
                SizedBox::new(size)
            };
            (if enabled {
                SizedBox::new(Size::new(4.0, 5.0))
            } else {
                SizedBox::new(Size::new(6.0, 7.0))
            });
            ({
                match choice {
                    Some(size) => SizedBox::new(size),
                    None => SizedBox::new(Size::ZERO),
                }
            });
            Column::new() => {}
        }
    };

    let _ = view! { &mut cx; SizedBox::new(Size::new(8.0, 9.0)); };
}
