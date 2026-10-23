//! Shared application confirmation composition for paste and clipboard-write requests.

use harbor_widget::{
    Button, Column, FocusScope, Row,
    layout::Size,
    view::{BuildCx, Component, View},
    widgets::{padding::Padding, sized_box::SizedBox, text_label::TextLabel},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

pub mod preview;
use preview::PreviewPane;
pub const CONFIRMATION_PREVIEW_VISIBLE_LINES: usize = 12;
#[derive(Clone)]
struct ConfirmationDialogProps {
    header_text: String,
    wrapped_lines: Vec<String>,
    scroll_offset: Arc<AtomicUsize>,
    cancelled: Arc<AtomicBool>,
    confirmed: Arc<AtomicBool>,
    line_height: f32,
    deny_label: &'static str,
    allow_label: &'static str,
}

impl Component for ConfirmationDialogProps {
    fn build(&self, _cx: &mut BuildCx) -> View {
        let cancelled = Arc::clone(&self.cancelled);
        let confirmed = Arc::clone(&self.confirmed);
        harbor_widget::view! { FocusScope::new() => {
            Padding::new(24.0, 16.0, 24.0, 16.0) => {
                Column::new() => {
                    TextLabel::new(self.header_text.clone());
                    SizedBox::new(Size::new(0.0, 8.0));
                    PreviewPane::new(
                        self.wrapped_lines.clone(),
                        Arc::clone(&self.scroll_offset),
                        self.line_height,
                        CONFIRMATION_PREVIEW_VISIBLE_LINES,
                    );
                    SizedBox::new(Size::new(0.0, 12.0));
                    Row::new() => {
                        Button::new(self.deny_label).on_click(move |_| {
                            cancelled.store(true, Ordering::SeqCst);
                        });
                        SizedBox::new(Size::new(12.0, 0.0));
                        Button::new(self.allow_label).on_click(move |_| {
                            confirmed.store(true, Ordering::SeqCst);
                        });
                    }
                }
            }
        } }
    }
}

pub fn build_confirmation_root(
    line_count: usize,
    wrapped_lines: Vec<String>,
    scroll_offset: Arc<AtomicUsize>,
    cancelled: Arc<AtomicBool>,
    confirmed: Arc<AtomicBool>,
    line_height: f32,
) -> impl Component {
    ConfirmationDialogProps {
        header_text: format!("Paste {line_count} lines?"),
        deny_label: "Cancel",
        allow_label: "Paste",
        wrapped_lines,
        scroll_offset,
        cancelled,
        confirmed,
        line_height,
    }
}
/// Independent, per-request clipboard-write confirmation; never grants paste permission.
pub fn build_clipboard_confirmation_root(
    header_text: String,
    wrapped_lines: Vec<String>,
    scroll_offset: Arc<AtomicUsize>,
    cancelled: Arc<AtomicBool>,
    confirmed: Arc<AtomicBool>,
    line_height: f32,
) -> impl Component {
    ConfirmationDialogProps {
        header_text,
        deny_label: "Deny this request",
        allow_label: "Allow this request",
        wrapped_lines,
        scroll_offset,
        cancelled,
        confirmed,
        line_height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harbor_widget::{
        input::event::{Key, KeyboardEvent, Modifiers, UiEvent},
        runtime::Runtime,
    };
    use std::time::Instant;

    #[test]
    fn confirmation_buttons_preserve_host_coordination_and_independent_permissions() {
        for clipboard_write in [false, true] {
            let cancelled = Arc::new(AtomicBool::new(false));
            let confirmed = Arc::new(AtomicBool::new(false));
            let scroll = Arc::new(AtomicUsize::new(0));
            let mut runtime = Runtime::new();
            if clipboard_write {
                runtime.set_root(build_clipboard_confirmation_root(
                    "Clipboard request".into(),
                    vec!["preview".into()],
                    scroll,
                    cancelled.clone(),
                    confirmed.clone(),
                    18.0,
                ));
            } else {
                runtime.set_root(build_confirmation_root(
                    1,
                    vec!["preview".into()],
                    scroll,
                    cancelled.clone(),
                    confirmed.clone(),
                    18.0,
                ));
            }
            runtime.update(Instant::now());
            assert!(runtime.focus_first_focusable());
            let press = |runtime: &mut Runtime, key| {
                runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
                    key,
                    modifiers: Modifiers::default(),
                }));
                runtime.update(Instant::now());
            };
            press(&mut runtime, Key::Enter);
            assert!(cancelled.load(Ordering::SeqCst));
            assert!(
                !confirmed.load(Ordering::SeqCst),
                "deny never grants permission"
            );
            cancelled.store(false, Ordering::SeqCst);
            press(&mut runtime, Key::Tab);
            press(&mut runtime, Key::Space);
            assert!(confirmed.load(Ordering::SeqCst));
            assert!(!cancelled.load(Ordering::SeqCst));
            drop(runtime);
            assert_eq!(Arc::strong_count(&confirmed), 1);
            assert_eq!(Arc::strong_count(&cancelled), 1);
        }
    }
}
