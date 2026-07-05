use super::Screen;
use super::screen::AltScreenAction;

/// Maximum allowed value for a single CSI numeric parameter.
///
/// Parameters exceeding this value are treated as malformed and the entire CSI
/// sequence is ignored with a warning. This catches stray huge numbers or
/// corrupted byte streams without silently executing unintended operations.
const MAX_CSI_PARAM: usize = 65535;

/// High-level ANSI/VT parser state.
///
/// The parser advances one byte at a time so incomplete escape sequences can span multiple PTY
/// reads without rendering their bytes as visible characters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ParserState {
    /// Normal text/control-byte processing.
    Ground,
    /// Saw `ESC`; the next byte selects an escape sequence or starts CSI/OSC.
    Escape,
    /// Inside `ESC [` control sequence introducer parameters until a final byte arrives.
    Csi,
    /// Inside `ESC ]` operating-system command text until BEL or ST terminates it.
    Osc,
    /// Saw `ESC` inside OSC; `\` terminates the OSC string.
    OscEscape,
}

/// Accumulator for CSI numeric parameters.
///
/// Supported sequences need at most two parameters, but a fixed buffer avoids heap allocation and
/// safely consumes longer sequences by ignoring parameters after the buffer fills.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct CsiState {
    /// Parsed parameters, preserving empty slots like `CSI ; 3 H` as `None`.
    params: [Option<usize>; 16],
    /// Number of populated entries in `params`.
    len: usize,
    /// Digits being accumulated for the current parameter.
    current: Option<usize>,
    /// Whether this is a private CSI sequence such as `CSI ? 25 l`.
    private: bool,
    /// Set when a parameter or intermediate byte violates expected CSI syntax.
    /// When set, the final byte logs a warning and skips dispatch.
    malformed: bool,
    /// SP (0x20) intermediate byte for sequences like CSI Ps SP q (DECSCUSR).
    intermediate: Option<u8>,
}

impl CsiState {
    fn reset(&mut self) {
        *self = Self::default();
    }

    /// Finishes the current CSI parameter and stores it if there is capacity.
    ///
    /// Empty parameters are represented as `None` so dispatch can apply sequence-specific
    /// defaults.
    fn push_current(&mut self) {
        if self.len < self.params.len() {
            self.params[self.len] = self.current;
            self.len += 1;
        } else {
            tracing::warn!(
                "CSI parameter buffer full (max {}), dropping extra parameters",
                self.params.len(),
            );
        }
        self.current = None;
    }
}

/// Pending UTF-8 bytes for a possibly split multi-byte character.
///
/// PTY reads can cut a character between bytes. Keeping this state prevents valid split
/// characters from becoming replacement glyphs.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Utf8State {
    /// Buffered UTF-8 bytes. Four bytes is the maximum length of one Unicode scalar value.
    bytes: [u8; 4],
    /// Number of valid bytes currently stored in `bytes`.
    len: usize,
}

impl Utf8State {
    fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Streaming terminal parser.
///
/// `TerminalParser` owns only parser state. It mutates a supplied `Screen`, which keeps the
/// renderable grid separate from byte-stream parsing.
#[derive(Debug)]
pub(super) struct TerminalParser {
    /// Current parser mode.
    state: ParserState,
    /// CSI parameter accumulator used while `state == ParserState::Csi`.
    csi: CsiState,
    /// Pending UTF-8 bytes carried across calls to `put_bytes`.
    utf8: Utf8State,
}

impl Default for TerminalParser {
    fn default() -> Self {
        Self {
            state: ParserState::Ground,
            csi: CsiState::default(),
            utf8: Utf8State::default(),
        }
    }
}

/// Result of feeding bytes through the parser.
pub(super) struct PutResult {
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
    pub(super) fn put_bytes(&mut self, screen: &mut Screen, bytes: &[u8]) -> PutResult {
        for (consumed, &byte) in bytes.iter().enumerate() {
            self.put_byte(screen, byte);
            let request = screen.alt_request();
            if request.is_some() {
                return PutResult {
                    consumed: consumed + 1,
                    alt_request: screen.take_alt_request(),
                };
            }
        }
        PutResult {
            consumed: bytes.len(),
            alt_request: None,
        }
    }

    fn put_byte(&mut self, screen: &mut Screen, byte: u8) {
        match self.state {
            ParserState::Ground => self.put_ground_byte(screen, byte),
            ParserState::Escape => self.put_escape_byte(screen, byte),
            ParserState::Csi => self.put_csi_byte(screen, byte),
            ParserState::Osc => self.put_osc_byte(byte),
            ParserState::OscEscape => self.put_osc_escape_byte(byte),
        }
    }

    /// Handles ground-state bytes: printable ASCII, C0 controls, UTF-8, or `ESC`.
    fn put_ground_byte(&mut self, screen: &mut Screen, byte: u8) {
        match byte {
            0x1b => self.state = ParserState::Escape,
            0x00..=0x1f => self.execute_control(screen, byte),
            0x7f => {} // DEL: explicitly ignored (C1 control, zero-width no-op)
            0x20..=0x7e => {
                if self.utf8.len > 0 {
                    self.write_replacement(screen);
                }
                screen.write_char(byte as char);
            }
            _ => self.put_utf8_byte(screen, byte),
        }
    }

    /// Handles one byte after `ESC`.
    ///
    /// C0 controls execute immediately but leave the parser in escape state; unrecognized escape
    /// sequences are consumed and ignored so they never appear in the grid.
    fn put_escape_byte(&mut self, screen: &mut Screen, byte: u8) {
        match byte {
            b'[' => {
                self.csi.reset();
                self.state = ParserState::Csi;
            }
            b']' => {
                self.utf8.reset();
                self.state = ParserState::Osc;
            }
            b'c' => {
                screen.reset_display();
                self.csi.reset();
                self.utf8.reset();
                self.state = ParserState::Ground;
            }
            b'D' => {
                screen.newline();
                self.state = ParserState::Ground;
            }
            b'E' => {
                screen.newline();
                screen.carriage_return();
                self.state = ParserState::Ground;
            }
            b'M' => {
                screen.reverse_index();
                self.state = ParserState::Ground;
            }
            b'7' => {
                screen.save_cursor();
                self.state = ParserState::Ground;
            }
            b'8' => {
                screen.restore_cursor();
                self.state = ParserState::Ground;
            }
            0x18 | 0x1a => self.state = ParserState::Ground,
            0x1b => self.state = ParserState::Escape,
            0x00..=0x1f => self.execute_control(screen, byte),
            _ => {
                tracing::warn!("unsupported escape sequence: ESC 0x{byte:02x}");
                self.state = ParserState::Ground;
            }
        }
    }

    /// Accumulates CSI parameters until a final byte dispatches the sequence.
    ///
    /// Final bytes are `0x40..=0x7e`. CAN/SUB cancel the sequence, and a nested ESC restarts
    /// escape parsing.
    fn put_csi_byte(&mut self, screen: &mut Screen, byte: u8) {
        match byte {
            b'?' => self.csi.private = true,
            b'0'..=b'9' => {
                let digit = usize::from(byte - b'0');
                let current = self.csi.current.unwrap_or(0);
                let value = current.saturating_mul(10).saturating_add(digit);
                if value > MAX_CSI_PARAM {
                    self.csi.malformed = true;
                }
                self.csi.current = Some(value);
            }
            // Intermediate bytes (0x20-0x2f: space through `/`) and unhandled
            // parameter bytes (0x3a `:`, 0x3c `<`, 0x3d `=`, 0x3e `>`). SP (0x20)
            // is a valid intermediate for sequences like CSI Ps SP q (DECSCUSR).
            0x20 => {
                self.csi.intermediate = Some(byte);
            }
            0x21..=0x2f | 0x3a | 0x3c..=0x3e => {
                self.csi.malformed = true;
            }
            b';' => self.csi.push_current(),
            0x40..=0x7e => {
                if self.csi.current.is_some() || self.csi.len == 0 {
                    self.csi.push_current();
                }
                if self.csi.malformed {
                    tracing::warn!(
                        "malformed CSI sequence: params={:?} final=0x{final_byte:02x} — ignored",
                        &self.csi.params[..self.csi.len],
                        final_byte = byte,
                    );
                } else {
                    self.dispatch_csi(screen, byte);
                }
                self.csi.reset();
                self.state = ParserState::Ground;
            }
            0x18 | 0x1a => {
                self.csi.reset();
                self.state = ParserState::Ground;
            }
            0x1b => {
                self.csi.reset();
                self.state = ParserState::Escape;
            }
            _ => {}
        }
    }

    /// Consumes OSC payload bytes such as `ESC ] 0 ; title BEL`.
    ///
    /// The renderer does not expose window-title state, so OSC strings are ignored rather than
    /// painted into the visible grid.
    fn put_osc_byte(&mut self, byte: u8) {
        match byte {
            0x07 | 0x18 | 0x1a => {
                tracing::warn!("unsupported OSC sequence (terminated by 0x{byte:02x})");
                self.state = ParserState::Ground;
            }
            0x1b => self.state = ParserState::OscEscape,
            _ => {}
        }
    }

    /// Handles `ESC` seen inside OSC. `ESC \` is the standard ST terminator.
    fn put_osc_escape_byte(&mut self, byte: u8) {
        match byte {
            b'\\' => {
                tracing::warn!("unsupported OSC sequence (terminated by ST)");
                self.state = ParserState::Ground;
            }
            0x18 | 0x1a => {
                tracing::warn!("unsupported OSC sequence (terminated by 0x{byte:02x})");
                self.state = ParserState::Ground;
            }
            0x1b => self.state = ParserState::OscEscape,
            _ => self.state = ParserState::Osc,
        }
    }

    /// Executes the subset of C0 controls that affects the visible grid.
    fn execute_control(&mut self, screen: &mut Screen, byte: u8) {
        match byte {
            0x07 => {}
            0x08 => screen.backspace(),
            0x09 => screen.horizontal_tab(),
            0x0a..=0x0c => screen.newline(),
            0x0d => screen.carriage_return(),
            _ => {}
        }
    }

    /// Accumulates and validates one UTF-8 character from one or more bytes.
    ///
    /// Invalid byte sequences emit U+FFFD. Valid but incomplete prefixes stay buffered until a
    /// later `put_bytes` call supplies the remaining bytes.
    fn put_utf8_byte(&mut self, screen: &mut Screen, byte: u8) {
        if self.utf8.len == self.utf8.bytes.len() {
            self.write_replacement(screen);
        }
        self.utf8.bytes[self.utf8.len] = byte;
        self.utf8.len += 1;

        match std::str::from_utf8(&self.utf8.bytes[..self.utf8.len]) {
            Ok(text) => {
                if let Some(ch) = text.chars().next() {
                    screen.write_char(ch);
                    self.utf8.reset();
                }
            }
            Err(error) if error.error_len().is_some() => self.write_replacement(screen),
            Err(_) if self.utf8.len == self.utf8.bytes.len() => self.write_replacement(screen),
            Err(_) => {}
        }
    }

    fn write_replacement(&mut self, screen: &mut Screen) {
        self.utf8.reset();
        screen.write_char('\u{fffd}');
    }

    /// Returns a CSI parameter or the caller-specified default for missing/empty parameters.
    fn csi_param(&self, index: usize, default: usize) -> usize {
        self.csi
            .params
            .get(index)
            .copied()
            .flatten()
            .unwrap_or(default)
    }

    /// Applies supported CSI final bytes to the screen.
    fn dispatch_csi(&mut self, screen: &mut Screen, final_byte: u8) {
        if self.csi.private {
            match final_byte {
                b'h' if self.csi.params[..self.csi.len] == [Some(1049)] => {
                    screen.request_alt_enter();
                }
                b'l' if self.csi.params[..self.csi.len] == [Some(1049)] => {
                    screen.request_alt_exit();
                }
                _ => {
                    tracing::warn!(
                        "unsupported private CSI sequence: params={:?} final=0x{:02x}",
                        &self.csi.params[..self.csi.len],
                        final_byte,
                    );
                }
            }
            return;
        }

        // Handle SP intermediate: currently only CSI Ps SP q (DECSCUSR) is recognized.
        if self.csi.intermediate == Some(b' ') {
            if final_byte == b'q' {
                screen.set_cursor_style(self.csi_param(0, 1));
            } else {
                tracing::warn!(
                    "unrecognized CSI sequence with SP intermediate: params={:?} final=0x{:02x}",
                    &self.csi.params[..self.csi.len],
                    final_byte,
                );
            }
            return;
        }

        match final_byte {
            b'A' => screen.cursor_up(self.csi_param(0, 1)),
            b'B' => screen.cursor_down(self.csi_param(0, 1)),
            b'C' => screen.cursor_right(self.csi_param(0, 1)),
            b'D' => screen.cursor_left(self.csi_param(0, 1)),
            b'E' => {
                let n = self.csi_param(0, 1);
                screen.cursor_down(n);
                screen.carriage_return();
            }
            b'F' => {
                let n = self.csi_param(0, 1);
                screen.cursor_up(n);
                screen.carriage_return();
            }
            b'G' => {
                // CHA: cursor horizontal absolute (1-based → 0-based).
                let col = self
                    .csi_param(0, 1)
                    .saturating_sub(1)
                    .min(screen.cols() - 1);
                screen.set_cursor(screen.cursor_y() + 1, col + 1);
            }
            b'H' | b'f' => screen.set_cursor(self.csi_param(0, 1), self.csi_param(1, 1)),
            b'J' => screen.erase_display(self.csi_param(0, 0)),
            b'K' => screen.erase_line(self.csi_param(0, 0)),
            b'd' => {
                // VPA: vertical position absolute (1-based → 0-based).
                let row = self
                    .csi_param(0, 1)
                    .saturating_sub(1)
                    .min(screen.rows() - 1);

                screen.set_cursor(row + 1, screen.cursor_x() + 1);
            }
            b'm' => screen.set_sgr(&self.csi.params[..self.csi.len]),
            b'X' => screen.erase_chars(self.csi_param(0, 1)),
            b'r' => screen.set_scroll_region(self.csi_param(0, 0), self.csi_param(1, 0)),
            b's' => screen.save_cursor(),
            b'u' => screen.restore_cursor(),
            b'@' => screen.insert_chars(self.csi_param(0, 1)),
            b'P' => screen.delete_chars(self.csi_param(0, 1)),
            b'L' => screen.insert_lines(self.csi_param(0, 1)),
            b'M' => screen.delete_lines(self.csi_param(0, 1)),
            b'S' => screen.scroll_up_region(self.csi_param(0, 1)),
            b'T' => screen.scroll_down_region(self.csi_param(0, 1)),
            _ => {
                tracing::warn!(
                    "unsupported CSI sequence: params={:?} final=0x{:02x}",
                    &self.csi.params[..self.csi.len],
                    final_byte,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{CursorShape, Screen};
    use super::*;

    fn feed(parser: &mut TerminalParser, screen: &mut Screen, seq: &[u8]) {
        parser.put_bytes(screen, seq);
    }

    /// Move cursor to (row, col) 1-based via `CSI row;col H`.
    fn move_to(parser: &mut TerminalParser, screen: &mut Screen, row: usize, col: usize) {
        feed(parser, screen, format!("\x1b[{row};{col}H").as_bytes());
    }

    #[test]
    fn oversized_param_skips_dispatch() {
        let mut screen = Screen::new(10, 10);
        let mut parser = TerminalParser::default();
        move_to(&mut parser, &mut screen, 4, 4);
        assert_eq!(screen.cursor_y(), 3);
        feed(&mut parser, &mut screen, b"\x1b[999999A");
        assert_eq!(screen.cursor_y(), 3, "oversized param should skip dispatch");
    }

    #[test]
    fn normal_param_still_dispatches() {
        let mut screen = Screen::new(10, 10);
        let mut parser = TerminalParser::default();
        feed(&mut parser, &mut screen, b"\x1b[5B");
        assert_eq!(screen.cursor_y(), 5);
    }

    #[test]
    fn max_valid_param_dispatches_and_clamps() {
        let mut screen = Screen::new(100, 100);
        let mut parser = TerminalParser::default();
        feed(&mut parser, &mut screen, b"\x1b[65535B");
        assert_eq!(
            screen.cursor_y(),
            screen.rows() - 1,
            "valid param at MAX should dispatch and clamp"
        );
    }

    #[test]
    fn saturated_oversized_param_still_rejected() {
        let mut screen = Screen::new(10, 10);
        let mut parser = TerminalParser::default();
        move_to(&mut parser, &mut screen, 4, 4);
        feed(&mut parser, &mut screen, b"\x1b[99999999999999999999A");
        assert_eq!(
            screen.cursor_y(),
            3,
            "saturated oversized param should skip"
        );
    }

    #[test]
    fn intermediate_byte_cancels_sequence() {
        let mut screen = Screen::new(10, 10);
        let mut parser = TerminalParser::default();
        move_to(&mut parser, &mut screen, 4, 4);
        feed(&mut parser, &mut screen, b"\x1b[!A");
        assert_eq!(screen.cursor_y(), 3, "intermediate byte should cancel CSI");
    }

    #[test]
    fn unsupported_param_byte_cancels_sequence() {
        for &byte in b":<>" {
            let mut screen = Screen::new(10, 10);
            let mut parser = TerminalParser::default();
            move_to(&mut parser, &mut screen, 4, 4);
            let seq = [b'\x1b', b'[', byte, b'A'];
            feed(&mut parser, &mut screen, &seq);
            assert_eq!(
                screen.cursor_y(),
                3,
                "byte 0x{:02x} should cancel CSI",
                byte
            );
        }
    }

    #[test]
    fn many_empty_params_does_not_panic() {
        // 17 semicolons → 17 push_current calls → 16 fit, 17th triggers warn.
        let mut screen = Screen::new(5, 5);
        let mut parser = TerminalParser::default();
        move_to(&mut parser, &mut screen, 3, 3);
        feed(&mut parser, &mut screen, b"\x1b[;;;;;;;;;;;;;;;;;H");
        // Sequence still dispatches (empty params → defaults → cursor home).
        assert_eq!(screen.cursor_y(), 0, "overflow should not panic");
        assert_eq!(screen.cursor_x(), 0);
    }

    #[test]
    fn empty_params_use_defaults() {
        let mut screen = Screen::new(10, 10);
        let mut parser = TerminalParser::default();
        move_to(&mut parser, &mut screen, 5, 5);
        feed(&mut parser, &mut screen, b"\x1b[;;;;H");
        assert_eq!(screen.cursor_y(), 0, "empty params should use defaults");
        assert_eq!(screen.cursor_x(), 0);
    }

    #[test]
    fn decscusr_ps5_is_blinking_bar() {
        let mut screen = Screen::new(10, 10);
        let mut parser = TerminalParser::default();
        feed(&mut parser, &mut screen, b"\x1b[5 q");
        assert_eq!(screen.cursor_shape(), CursorShape::Bar);
        assert!(screen.cursor_blink());
    }

    #[test]
    fn decscusr_ps2_is_steady_block() {
        let mut screen = Screen::new(10, 10);
        let mut parser = TerminalParser::default();
        feed(&mut parser, &mut screen, b"\x1b[2 q");
        assert_eq!(screen.cursor_shape(), CursorShape::Block);
        assert!(!screen.cursor_blink());
    }

    #[test]
    fn decscusr_ps0_is_blinking_bar() {
        let mut screen = Screen::new(10, 10);
        let mut parser = TerminalParser::default();
        feed(&mut parser, &mut screen, b"\x1b[0 q");
        assert_eq!(screen.cursor_shape(), CursorShape::Bar);
        assert!(screen.cursor_blink());
    }

    #[test]
    fn decscusr_ps1_is_blinking_block() {
        let mut screen = Screen::new(10, 10);
        let mut parser = TerminalParser::default();
        feed(&mut parser, &mut screen, b"\x1b[1 q");
        assert_eq!(screen.cursor_shape(), CursorShape::Block);
        assert!(screen.cursor_blink());
    }

    #[test]
    fn initial_cursor_shape_is_bar() {
        let screen = Screen::new(10, 10);
        assert_eq!(screen.cursor_shape(), CursorShape::Bar);
        assert!(screen.cursor_blink());
    }

    // ── CHA (CSI n G) ─────────────────────────────────────────────

    #[test]
    fn cha_sets_column_keeps_row() {
        let mut screen = Screen::new(5, 20);
        let mut parser = TerminalParser::default();
        move_to(&mut parser, &mut screen, 3, 5);
        feed(&mut parser, &mut screen, b"\x1b[12G");
        assert_eq!(screen.cursor_y(), 2, "CHA should keep row unchanged");
        assert_eq!(screen.cursor_x(), 11, "CHA should set column (0-based)");
    }

    #[test]
    fn cha_default_param_is_one() {
        let mut screen = Screen::new(5, 20);
        let mut parser = TerminalParser::default();
        move_to(&mut parser, &mut screen, 2, 8);
        feed(&mut parser, &mut screen, b"\x1b[G");
        assert_eq!(screen.cursor_x(), 0, "default CHA param = 1 → col 0");
    }

    #[test]
    fn cha_clamps_to_cols() {
        let mut screen = Screen::new(5, 10);
        let mut parser = TerminalParser::default();
        feed(&mut parser, &mut screen, b"\x1b[999G");
        assert_eq!(screen.cursor_x(), 9, "CHA clamps to cols-1");
    }

    // ── VPA (CSI n d) ─────────────────────────────────────────────

    #[test]
    fn vpa_sets_row_keeps_col() {
        let mut screen = Screen::new(10, 20);
        let mut parser = TerminalParser::default();
        move_to(&mut parser, &mut screen, 5, 7);
        feed(&mut parser, &mut screen, b"\x1b[3d");
        assert_eq!(screen.cursor_y(), 2, "VPA should set row (0-based)");
        assert_eq!(screen.cursor_x(), 6, "VPA should keep col unchanged");
    }

    #[test]
    fn vpa_default_param_is_one() {
        let mut screen = Screen::new(10, 20);
        let mut parser = TerminalParser::default();
        move_to(&mut parser, &mut screen, 5, 5);
        feed(&mut parser, &mut screen, b"\x1b[d");
        assert_eq!(screen.cursor_y(), 0, "default VPA param = 1 → row 0");
    }

    #[test]
    fn vpa_clamps_to_rows() {
        let mut screen = Screen::new(5, 10);
        let mut parser = TerminalParser::default();
        feed(&mut parser, &mut screen, b"\x1b[999d");
        assert_eq!(screen.cursor_y(), 4, "VPA clamps to rows-1");
    }

    // ── CNL (CSI n E) / CPL (CSI n F) ────────────────────────────

    #[test]
    fn cnl_moves_down_and_cr() {
        let mut screen = Screen::new(10, 20);
        let mut parser = TerminalParser::default();
        move_to(&mut parser, &mut screen, 3, 8);
        feed(&mut parser, &mut screen, b"\x1b[2E");
        assert_eq!(screen.cursor_y(), 4, "CNL 2 from row 3 → row 5 (0-based 4)");
        assert_eq!(screen.cursor_x(), 0, "CNL resets column to 0");
    }

    #[test]
    fn cpl_moves_up_and_cr() {
        let mut screen = Screen::new(10, 20);
        let mut parser = TerminalParser::default();
        move_to(&mut parser, &mut screen, 6, 5);
        feed(&mut parser, &mut screen, b"\x1b[3F");
        assert_eq!(screen.cursor_y(), 2, "CPL 3 from row 6 → row 3 (0-based 2)");
        assert_eq!(screen.cursor_x(), 0, "CPL resets column to 0");
    }

    // ── SCP / RCP (CSI s / CSI u) ─────────────────────────────────

    #[test]
    fn csi_save_restore_cursor() {
        let mut screen = Screen::new(10, 20);
        let mut parser = TerminalParser::default();
        move_to(&mut parser, &mut screen, 4, 6);
        feed(&mut parser, &mut screen, b"\x1b[s");
        move_to(&mut parser, &mut screen, 8, 12);
        feed(&mut parser, &mut screen, b"\x1b[u");
        assert_eq!(screen.cursor_y(), 3, "RCP should restore row");
        assert_eq!(screen.cursor_x(), 5, "RCP should restore col");
    }
}
