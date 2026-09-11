use harbor_widget::view::BuildCx;
use harbor_widget::widgets::column::Column;
use harbor_widget::{view};

fn main() {
    let mut cx = BuildCx::stub();
    let _ = view!(&mut cx, Column::new() => { { 42usize } });
}
