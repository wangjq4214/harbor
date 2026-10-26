use harbor_widget::{ExternalSurface, SizedBox, layout::Size, view};

fn main() {
    let _ = view! { ExternalSurface::new(1) => {
        SizedBox::new(Size::ZERO);
        SizedBox::new(Size::ZERO);
    }};
}
