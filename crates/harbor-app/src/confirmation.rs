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
