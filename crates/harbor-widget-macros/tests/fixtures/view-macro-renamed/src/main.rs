use widget_alias::layout::Size;
use widget_alias::view::BuildCx;
use widget_alias::widgets::column::Column;
use widget_alias::widgets::sized_box::SizedBox;
use widget_alias::view;

fn main() {
    let mut cx = BuildCx::stub();
    let _ = view!(&mut cx, Column::new() => {
        SizedBox::new(Size::new(10.0, 10.0)) => {}
    });
}
