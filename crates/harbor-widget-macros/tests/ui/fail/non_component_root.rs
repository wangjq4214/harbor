use harbor_widget::view;

fn main() {
    let not_a_component = 42u8;
    let _ = view! { not_a_component; };
}
