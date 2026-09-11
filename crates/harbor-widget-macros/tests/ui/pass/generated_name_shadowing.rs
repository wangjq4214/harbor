use harbor_widget::layout::Size;
use harbor_widget::view::BuildCx;
use harbor_widget::widgets::column::Column;
use harbor_widget::widgets::sized_box::SizedBox;
use harbor_widget::view;

fn main() {
    let mut cx = BuildCx::stub();
    let _ = view!(&mut cx, Column::new() => {
        for __harbor_view_children_0 in 0..1 {
            SizedBox::new(Size::new(1.0, 1.0)) => {}
        }
    });
}
