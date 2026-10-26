use harbor_widget::{EditableText, TextEditingController, TextField};

fn main() {
    let _ = TextField::new(TextEditingController::new("")).enabled(true);
    let _ = EditableText::new(TextEditingController::new("")).enabled(true);
}
