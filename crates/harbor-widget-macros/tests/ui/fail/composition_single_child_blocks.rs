use harbor_widget::{ActionHandler, ActionOutcome, Focus, ShortcutMap, TextLabel, view};
fn main() {
    let _ = view! { Focus::new() => { TextLabel::new("a"); TextLabel::new("b"); } };
    let _ = view! { ShortcutMap::<u8>::new() => { TextLabel::new("a"); TextLabel::new("b"); } };
    let _ = view! { ActionHandler::new(|_: u8| ActionOutcome::Consumed) => { TextLabel::new("a"); TextLabel::new("b"); } };
}
