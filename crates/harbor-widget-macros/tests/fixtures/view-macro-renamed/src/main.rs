use widget_alias::layout::Size;
use widget_alias::{Column, SizedBox, Stack, TextLabel, view};

fn main() {
    let _ = view! {
        Column::new() => {
            SizedBox::new(Size::new(10.0, 10.0));
        }
    };
    use widget_alias::Padding;
    use widget_alias::{BuildCx, Component, View};
    use widget_alias::{ChildConstructionError, Children, ComponentExt, SingleChild, WithChildren};
    struct External(Padding);
    impl WithChildren for External {
        type Capability = SingleChild;
        fn with_children(self, children: Children) -> Result<Self, ChildConstructionError> {
            Ok(Self(self.0.with_children(children)?))
        }
    }
    impl Component for External {
        fn build(&self, cx: &mut BuildCx) -> View {
            self.0.build(cx)
        }
    }
    let _ = view! { Stack::new() => { TextLabel::new("root exports"); } };
    let enabled = std::env::args().len() > 1;
    let _ = view! { External(Padding::all(1.0).unwrap()).keyed("external") => {
        if enabled { SizedBox::new(Size::ZERO); } else {}
    }};
    let _ = view! { Ok::<_, &str>(External(Padding::all(1.0).unwrap())) => {
        for _ in 0..usize::from(enabled) { SizedBox::new(Size::ZERO); }
    }};
}
