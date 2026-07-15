//! Streaming VT parser: bridge between `harbor_parser` core and `Screen`.

mod handlers;

#[cfg(test)]
mod incremental_tests;
#[cfg(test)]
mod tests;

use crate::screen::Screen;
use handlers::ScreenHandler;
use harbor_parser::Parser;
use harbor_types::AltScreenAction;

/// Streaming terminal parser.
///
/// `TerminalParser` owns only parser state. It mutates a supplied `Screen`, which keeps the
/// renderable grid separate from byte-stream parsing.
#[derive(Debug, Default)]
pub struct TerminalParser {
    inner: Parser,
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
            {
                let mut handler = ScreenHandler { screen };
                self.inner.advance(&mut handler, byte);
            }
            if screen.alt_request().is_some() {
                return PutResult {
                    consumed: i + 1,
                    alt_request: screen.take_alt_request(),
                };
            }
        }
        PutResult {
            consumed: bytes.len(),
            alt_request: None,
        }
    }
}
