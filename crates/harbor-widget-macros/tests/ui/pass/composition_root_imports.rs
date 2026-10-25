use harbor_widget::layout::Size;
use harbor_widget::{
    ActionHandler, ActionOutcome, Align, Alignment2D, Axis, BoxDecoration, BuildCx, Button,
    Children, ClipBehavior, Column, Component, ComponentExt, DecoratedBox, Flex, Focus,
    FocusHandle, Insets, InteractiveRegion, IntoChildView, LayoutObserver, Padding, Row,
    ScrollController, ShortcutMap, SizedBox, Stack, TextLabel, View, WithChildren, view,
};

#[derive(Clone)]
struct Root;
impl Component for Root {
    fn build(&self, _cx: &mut BuildCx) -> View {
        view! {
            ActionHandler::new(|_: u8| ActionOutcome::Consumed) => {
                ShortcutMap::<u8>::new() => {
                    Focus::new().focus_handle(FocusHandle::new()).tab_order(2).focusable(true) => {
                        Stack::new() => { TextLabel::new("root").keyed("label"); }
                    }
                }
            }
        }
    }
}
fn main() {
    let _ = Root.into_child_view();
    let _ = ActionHandler::new(|_: u8| ActionOutcome::Consumed)
        .child(TextLabel::new("old"))
        .child(TextLabel::new("new"))
        .with_children(Children::new())
        .unwrap();
    let _ = Focus::new().child(TextLabel::new("focus"));
    let _ = ShortcutMap::<u8>::new().child(TextLabel::new("map"));
    #[derive(Clone)]
    struct NoDefault;
    let _ = ShortcutMap::<NoDefault>::default(); // no A: Default requirement
    assert_eq!(Padding::all(3.0).unwrap().get_insets(), Insets::all(3.0));
    assert_eq!(
        Align::new(Alignment2D::CENTER).get_insets(),
        Insets::all(0.0)
    );
    assert_eq!(
        DecoratedBox::new(BoxDecoration::default()).get_decoration(),
        &BoxDecoration::default()
    );
    assert_eq!(
        DecoratedBox::new(BoxDecoration::default()).get_clip_behavior(),
        ClipBehavior::None
    );
    let _ = (
        Row::new().get_options(),
        Column::new().get_options(),
        Flex::new(Axis::Vertical).get_options(),
    );
    let sized = SizedBox::new(Size::new(2.0, 3.0)).unwrap();
    assert_eq!(sized.get_requested_width(), Some(2.0));
    assert_eq!(sized.get_requested_height(), Some(3.0));
    let _ = Button::new().child_with(|cx, _| TextLabel::new("button").build(cx));
    let _ = InteractiveRegion::new().child_with(|cx, _| TextLabel::new("region").build(cx));
    let callback: harbor_widget::LayoutChangedCallback = std::sync::Arc::new(|_| {});
    let _ = LayoutObserver::from_shared_callback(callback);
    let controller = ScrollController::new();
    assert_eq!(controller.requested_offset(), controller.metrics().offset());
}
