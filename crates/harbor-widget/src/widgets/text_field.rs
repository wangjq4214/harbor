use crate::{
    Border, BoxDecoration, TextEditingController,
    input::event::Modifiers,
    scene::primitive::Color,
    view::{BuildCx, Component, View},
    widgets::{
        FocusHandle, decorated_box::DecoratedBox, editable_text::EditableText, padding::Padding,
    },
};

/// Decorated single-line field, sharing all editing behavior with EditableText.
#[derive(Clone)]
pub struct TextField {
    editor: EditableText,
    decoration: BoxDecoration,
    padding: f32,
}

impl TextField {
    pub fn new(controller: TextEditingController) -> Self {
        Self {
            editor: EditableText::new(controller),
            decoration: BoxDecoration::new().border(
                Border::all(
                    Color {
                        r: 0.4,
                        g: 0.4,
                        b: 0.4,
                        a: 1.0,
                    },
                    1.0,
                )
                .expect("valid border"),
            ),
            padding: 4.0,
        }
    }
    pub fn decoration(mut self, decoration: BoxDecoration) -> Self {
        self.decoration = decoration;
        self
    }
    /// Sets finite, nonnegative padding. Zero is legal.
    pub fn padding(mut self, padding: f32) -> Result<Self, crate::LayoutConfigError> {
        crate::layout::configuration::validate_scalar("TextField.padding", padding)?;
        self.padding = padding;
        Ok(self)
    }
    /// Disables editing and focus eligibility. Enabled by default.
    /// Read-only is separate and still permits selection and copy.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.editor = self.editor.disabled(disabled);
        self
    }
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.editor = self.editor.read_only(read_only);
        self
    }
    pub fn focus_handle(mut self, handle: FocusHandle) -> Self {
        self.editor = self.editor.focus_handle(handle);
        self
    }
    pub fn foreground(mut self, color: Color) -> Result<Self, crate::theme::ColorConfigError> {
        crate::theme::validate_color("TextField.foreground", color)?;
        self.editor = self.editor.foreground(color)?;
        Ok(self)
    }
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.editor = self.editor.placeholder(text);
        self
    }
    pub fn on_change(mut self, callback: impl Fn(&str) + 'static) -> Self {
        self.editor = self.editor.on_change(callback);
        self
    }
    pub fn on_submit(mut self, callback: impl Fn(Modifiers) + 'static) -> Self {
        self.editor = self.editor.on_submit(callback);
        self
    }
    pub fn on_escape(mut self, callback: impl Fn() + 'static) -> Self {
        self.editor = self.editor.on_escape(callback);
        self
    }
    /// Observe vertical navigation after the editor's IME handling.
    pub fn on_vertical_navigation(
        mut self,
        callback: impl Fn(crate::VerticalDirection) + 'static,
    ) -> Self {
        self.editor = self.editor.on_vertical_navigation(callback);
        self
    }
}

impl Component for TextField {
    fn build(&self, cx: &mut BuildCx) -> View {
        let editor = self.editor.build(cx);
        let padded = Padding::all(self.padding).unwrap().child(editor).build(cx);
        DecoratedBox::new(self.decoration.clone())
            .child(padded)
            .build(cx)
    }
}
