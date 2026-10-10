//! Streaming VT parser: bridge between `harbor_parser` core and `Screen`.

mod device_attributes;
mod handlers;
mod mode_query;
mod osc;
mod osc133;
mod osc52;
mod osc7;
mod osc8;
mod osc_color;
mod osc_palette;
mod osc_title;
mod status_strings;
mod xtgettcap;

#[cfg(test)]
mod incremental_tests;
#[cfg(test)]
mod osc52_tests;
#[cfg(test)]
mod osc_action_tests;
#[cfg(test)]
mod osc_benchmark;
#[cfg(test)]
mod tests;

use crate::TerminalOutputEvent;
use crate::model::AltScreenAction;
use crate::screen::Screen;
use handlers::ScreenHandler;
use harbor_parser::Parser;
use status_strings::DecrqssRequest;
use xtgettcap::XtgettcapRequest;

/// Streaming terminal parser.
///
/// `TerminalParser` owns only parser state. It mutates a supplied `Screen`, which keeps the
/// renderable grid separate from byte-stream parsing.
#[derive(Debug, Default)]
pub struct TerminalParser {
    inner: Parser,
    decrqss: DecrqssRequest,
    xtgettcap: XtgettcapRequest,
    output_events: Vec<TerminalOutputEvent>,
    clipboard_delivery: crate::ClipboardDelivery,
}

/// Result of feeding bytes through the parser.
pub struct PutResult {
    /// Number of bytes consumed. Less than input length when a screen switch
    /// was triggered mid-batch; caller should re-feed remaining bytes after
    /// handling the switch.
    pub consumed: usize,
    /// Non-None when the parser dispatched a screen-switch sequence.
    pub alt_request: Option<AltScreenAction>,
}

impl TerminalParser {
    /// Consumes a PTY byte slice incrementally, preserving parser state for the next call.
    /// Returns a `PutResult` indicating how many bytes were consumed and whether an
    /// alternate-screen switch was triggered.
    pub fn put_bytes(&mut self, screen: &mut Screen, bytes: &[u8]) -> PutResult {
        for (i, &byte) in bytes.iter().enumerate() {
            self.inner.advance(
                &mut ScreenHandler {
                    screen,
                    decrqss: &mut self.decrqss,
                    xtgettcap: &mut self.xtgettcap,
                    output_events: &mut self.output_events,
                    clipboard_delivery: self.clipboard_delivery,
                },
                byte,
            );
            if let Some(alt_request) = screen.take_alt_request() {
                return PutResult {
                    consumed: i + 1,
                    alt_request: Some(alt_request),
                };
            }
        }
        PutResult {
            consumed: bytes.len(),
            alt_request: None,
        }
    }

    pub fn set_c1_enabled(&mut self, enabled: bool) {
        self.inner.set_c1_enabled(enabled);
    }

    /// Configure bounded pending clipboard delivery, independently of host authorization.
    pub fn set_clipboard_delivery(&mut self, delivery: crate::ClipboardDelivery) {
        self.clipboard_delivery = delivery;
        if delivery == crate::ClipboardDelivery::Discard {
            self.output_events
                .retain(|event| !matches!(event, TerminalOutputEvent::ClipboardWrite(_)));
        }
    }

    pub(crate) fn close_session(&mut self) {
        self.set_clipboard_delivery(crate::ClipboardDelivery::Discard);
        // A closed tab may stay visible; release partial wire data and its capacity too.
        self.inner = Parser::default();
        self.decrqss.cancel();
        self.xtgettcap.cancel();
    }

    pub(crate) fn clipboard_delivery(&self) -> crate::ClipboardDelivery {
        self.clipboard_delivery
    }

    pub(crate) fn drain_output_events(&mut self) -> Vec<TerminalOutputEvent> {
        std::mem::take(&mut self.output_events)
    }
}
