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
    /// Negative/non-finite padding is normalized to zero.
    pub fn padding(mut self, padding: f32) -> Self {
        self.padding = if padding.is_finite() {
            padding.max(0.0)
        } else {
            0.0
        };
        self
    }
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.editor = self.editor.enabled(enabled);
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
    pub fn color(mut self, color: Color) -> Self {
        self.editor = self.editor.color(color);
        self
    }
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.editor = self.editor.placeholder(text);
        self
    }
    pub fn on_changed(mut self, callback: impl Fn(&str) + 'static) -> Self {
        self.editor = self.editor.on_changed(callback);
        self
    }
    pub fn on_submitted(mut self, callback: impl Fn(Modifiers) + 'static) -> Self {
        self.editor = self.editor.on_submitted(callback);
        self
    }
    pub fn on_escape(mut self, callback: impl Fn() + 'static) -> Self {
        self.editor = self.editor.on_escape(callback);
        self
    }
    /// Observe vertical navigation after the editor's IME handling.
    pub fn on_vertical_navigation(mut self, callback: impl Fn(bool) + 'static) -> Self {
        self.editor = self.editor.on_vertical_navigation(callback);
        self
    }
}

impl Component for TextField {
    fn build(&self, cx: &mut BuildCx) -> View {
        let editor = self.editor.build(cx);
        let padded = Padding::all(self.padding).child(editor).build(cx);
        DecoratedBox::new(self.decoration.clone())
            .child(padded)
            .build(cx)
    }
}

impl crate::WithChildren for TextField {
    fn with_children(
        self,
        children: crate::Children,
    ) -> Result<Self, crate::ChildConstructionError> {
        children.ensure_leaf("TextField")?;
        Ok(self)
    }
}
