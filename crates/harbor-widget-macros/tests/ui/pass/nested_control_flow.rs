use harbor_widget::layout::Size;
use harbor_widget::view::{BuildCx, Component};
use harbor_widget::widgets::column::Column;
use harbor_widget::widgets::sized_box::SizedBox;
use harbor_widget::{view, ComponentExt};

fn main() {
    let mut cx = BuildCx::stub();
    let existing = SizedBox::new(Size::new(3.0, 4.0)).build(&mut cx);
    let sizes = [Size::new(5.0, 6.0), Size::new(7.0, 8.0)];
    let visible = true;
    let choice = Some(Size::new(9.0, 10.0));

    let _root = view!(&mut cx, Column::new() => {
        SizedBox::new(Size::new(1.0, 2.0)) => {}
        { existing }
        for size in sizes {
            SizedBox::new(size).keyed(size.width.to_string()) => {}
        }
        if visible {
            SizedBox::new(Size::new(11.0, 12.0)) => {}
        } else {
            SizedBox::new(Size::new(13.0, 14.0)) => {}
        }
        match choice {
            Some(size) if size.width > 0.0 => { SizedBox::new(size) => {} },
            None => {}
            Some(_) => {}
        }
    });
}
