use harbor_widget::widgets::column::Column;
use harbor_widget::view;

fn main() {
    let _ = view! {
        Column::new() => {
            { let _value = 42usize; };
        }
    };
}
