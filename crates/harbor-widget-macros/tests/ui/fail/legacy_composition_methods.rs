use harbor_widget::{
    ActionHandler, ActionOutcome, Button, Component, Focus, FocusHandle, InteractiveRegion,
    LayoutObserver, ShortcutMap, TextLabel,
};
fn main() {
    let _ = Focus::empty();
    let _ = Focus::new(TextLabel::new("old"));
    let _ = Focus::new().handle(FocusHandle::new());
    let _ = Focus::new().order(1);
    let _ = Focus::new().enabled(false);
    let _ = ShortcutMap::<u8>::empty();
    let _ = ShortcutMap::<u8>::new(TextLabel::new("old"));
    let _ = ActionHandler::handler(|_: u8| ActionOutcome::Consumed);
    let _ = ActionHandler::new(TextLabel::new("old"), |_: u8| ActionOutcome::Consumed);
    let _ = Button::new().content(|cx, _| TextLabel::new("old").build(cx));
    let _ = InteractiveRegion::new().content(|cx, _| TextLabel::new("old").build(cx));
    let callback: harbor_widget::LayoutChangedCallback = std::sync::Arc::new(|_| {});
    let _ = LayoutObserver::from_callback(callback);
}
