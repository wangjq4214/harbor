//! Application configuration-notice state and Harbor's floating presentation.

use harbor_widget::{
    Alignment, Notification, Portal,
    view::{BuildCx, Component},
};

/// Harbor's root escape, top-right placement and product presentation preset.
pub(crate) fn configuration_notification(
    message: String,
    top_offset: f32,
    on_dismiss: impl Fn() + Send + Sync + 'static,
) -> impl Component {
    let on_dismiss = std::sync::Arc::new(on_dismiss);
    move |cx: &mut BuildCx| {
        let mut background = cx.theme().button.normal.background;
        background.a = 1.0;
        let card = Notification::new(message.clone(), {
            let on_dismiss = on_dismiss.clone();
            move || on_dismiss()
        });
        Portal::new()
            .child(
                harbor_widget::widgets::align::Align::new(Alignment::End)
                    .vertical_alignment(Alignment::Start)
                    .insets(top_offset + 12.0, 12.0, 12.0, 12.0)
                    .child(
                        card.style(harbor_widget::widgets::notification::NotificationStyle {
                            foreground: Some(cx.theme().button.normal.foreground),
                            background: Some(background),
                            max_width: 520.0,
                            corner_radius: 6.0,
                            horizontal_padding: 12.0,
                            header_height: 32.0,
                            bottom_padding: 12.0,
                            close_width: 36.0,
                        }),
                    )
                    .build(cx),
            )
            .build(cx)
    }
}

/// Independent feedback sources: recovery of reload must not hide observation
/// or operation failures. This is presentation state, not configuration state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigurationNoticeSource {
    Startup,
    Reload,
    Observation,
    Resource,
    Operation,
    Command,
}

#[derive(Clone, Default)]
pub(crate) struct ConfigurationNotices {
    entries: Vec<(ConfigurationNoticeSource, String, bool)>,
    pub(crate) revision: u64,
}

impl ConfigurationNotices {
    pub(crate) fn publish(&mut self, source: ConfigurationNoticeSource, message: Option<String>) {
        let existing = self.entries.iter().position(|entry| entry.0 == source);
        if let Some(index) = existing {
            if message.as_ref() == Some(&self.entries[index].1) {
                return;
            }
            self.entries.remove(index);
        } else if message.is_none() {
            return;
        }
        if let Some(message) = message {
            self.entries.push((source, message, false));
        }
        self.revision += 1;
    }

    pub(crate) fn message(&self) -> Option<String> {
        let messages = self
            .entries
            .iter()
            .filter(|entry| !entry.2)
            .map(|entry| entry.1.as_str())
            .collect::<Vec<_>>();
        (!messages.is_empty()).then(|| messages.join("\n\n"))
    }

    pub(crate) fn dismiss(&mut self, revision: u64) {
        if revision == self.revision {
            for entry in &mut self.entries {
                entry.2 = true;
            }
        }
    }
}
