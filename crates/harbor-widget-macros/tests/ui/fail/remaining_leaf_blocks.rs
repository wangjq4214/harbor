use harbor_widget::{
    EditableText, Notification, Separator, Spacer, TextEditingController, TextField, view,
};

fn main() {
    let _ = view! { EditableText::new(TextEditingController::new("")) => {} };
    let _ = view! { TextField::new(TextEditingController::new("")) => {} };
    let _ = view! { Notification::new("message", || {}) => {} };
    let _ = view! { Separator::horizontal() => {} };
    let _ = view! { Spacer::new() => {} };
}
