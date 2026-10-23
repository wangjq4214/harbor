use harbor_widget::widgets::column::Column;
use harbor_widget::view;

fn main() {
    let _ = view! {
        Column::new() => {
            match Some(1) {
                Some(value) => value,
                None => {}
            }
        }
    };
}
