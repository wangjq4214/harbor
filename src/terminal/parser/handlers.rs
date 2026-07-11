//! Screen-backed `Perform` handler — all current execute/dispatch behavior.

use super::params::Params;
use super::perform::Perform;
use crate::terminal::Screen;

/// Applies recognized VT actions to a `Screen`.
pub(crate) struct ScreenHandler<'a> {
    pub screen: &'a mut Screen,
}

impl ScreenHandler<'_> {
    /// Returns a CSI parameter or the caller-specified default for missing/empty parameters.
    fn param(params: &Params, index: usize, default: usize) -> usize {
        params.get(index).unwrap_or(default)
    }
}

impl Perform for ScreenHandler<'_> {
    fn print(&mut self, ch: char) {
        self.screen.write_char(ch);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x07 => {}
            0x08 => self.screen.backspace(),
            0x09 => self.screen.horizontal_tab(),
            0x0a..=0x0c => self.screen.newline(),
            0x0d => self.screen.carriage_return(),
            _ => {}
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &Params,
        intermediates: &[u8],
        ignore: bool,
        private: bool,
        action: u8,
    ) {
        if ignore {
            tracing::warn!(
                "ignored CSI sequence: params={:?} final=0x{action:02x}",
                params.as_slice(),
            );
            return;
        }

        if private {
            match action {
                b'h' if params.as_slice() == [Some(1049)] => {
                    self.screen.request_alt_enter();
                }
                b'l' if params.as_slice() == [Some(1049)] => {
                    self.screen.request_alt_exit();
                }
                _ => {
                    tracing::warn!(
                        "unsupported private CSI sequence: params={:?} final=0x{:02x}",
                        params.as_slice(),
                        action,
                    );
                }
            }
            return;
        }

        // Handle SP intermediate: currently only CSI Ps SP q (DECSCUSR).
        if intermediates == [b' '] {
            if action == b'q' {
                self.screen.set_cursor_style(Self::param(params, 0, 1));
            } else {
                tracing::warn!(
                    "unrecognized CSI sequence with SP intermediate: params={:?} final=0x{:02x}",
                    params.as_slice(),
                    action,
                );
            }
            return;
        }

        if !intermediates.is_empty() {
            tracing::warn!(
                "unsupported CSI intermediates {:?}: params={:?} final=0x{:02x}",
                intermediates,
                params.as_slice(),
                action,
            );
            return;
        }

        match action {
            b'A' => self.screen.cursor_up(Self::param(params, 0, 1)),
            b'B' => self.screen.cursor_down(Self::param(params, 0, 1)),
            b'C' => self.screen.cursor_right(Self::param(params, 0, 1)),
            b'D' => self.screen.cursor_left(Self::param(params, 0, 1)),
            b'E' => {
                let n = Self::param(params, 0, 1);
                self.screen.cursor_down(n);
                self.screen.carriage_return();
            }
            b'F' => {
                let n = Self::param(params, 0, 1);
                self.screen.cursor_up(n);
                self.screen.carriage_return();
            }
            b'G' => {
                // CHA: cursor horizontal absolute (1-based → 0-based).
                let col = Self::param(params, 0, 1)
                    .saturating_sub(1)
                    .min(self.screen.cols() - 1);
                self.screen.set_cursor(self.screen.cursor_y() + 1, col + 1);
            }
            b'H' | b'f' => self
                .screen
                .set_cursor(Self::param(params, 0, 1), Self::param(params, 1, 1)),
            b'J' => self.screen.erase_display(Self::param(params, 0, 0)),
            b'K' => self.screen.erase_line(Self::param(params, 0, 0)),
            b'd' => {
                // VPA: vertical position absolute (1-based → 0-based).
                let row = Self::param(params, 0, 1)
                    .saturating_sub(1)
                    .min(self.screen.rows() - 1);
                self.screen.set_cursor(row + 1, self.screen.cursor_x() + 1);
            }
            b'm' => self.screen.set_sgr(params.as_slice()),
            b'X' => self.screen.erase_chars(Self::param(params, 0, 1)),
            b'r' => self
                .screen
                .set_scroll_region(Self::param(params, 0, 0), Self::param(params, 1, 0)),
            b's' => self.screen.save_cursor(),
            b'u' => self.screen.restore_cursor(),
            b'@' => self.screen.insert_chars(Self::param(params, 0, 1)),
            b'P' => self.screen.delete_chars(Self::param(params, 0, 1)),
            b'L' => self.screen.insert_lines(Self::param(params, 0, 1)),
            b'M' => self.screen.delete_lines(Self::param(params, 0, 1)),
            b'S' => self.screen.scroll_up_region(Self::param(params, 0, 1)),
            b'T' => self.screen.scroll_down_region(Self::param(params, 0, 1)),
            _ => {
                tracing::warn!(
                    "unsupported CSI sequence: params={:?} final=0x{:02x}",
                    params.as_slice(),
                    action,
                );
            }
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        if ignore || !intermediates.is_empty() {
            tracing::warn!(
                "unsupported escape sequence: ESC intermediates={intermediates:?} 0x{byte:02x}"
            );
            return;
        }

        match byte {
            b'c' => {
                self.screen.reset_display();
            }
            b'D' => {
                self.screen.newline();
            }
            b'E' => {
                self.screen.newline();
                self.screen.carriage_return();
            }
            b'M' => {
                self.screen.reverse_index();
            }
            b'7' => {
                self.screen.save_cursor();
            }
            b'8' => {
                self.screen.restore_cursor();
            }
            _ => {
                tracing::warn!("unsupported escape sequence: ESC 0x{byte:02x}");
            }
        }
    }

    fn osc_dispatch(&mut self, _params: &[&[u8]], bell_terminated: bool) {
        if bell_terminated {
            tracing::warn!("unsupported OSC sequence (terminated by BEL)");
        } else {
            tracing::warn!("unsupported OSC sequence (terminated by ST)");
        }
    }

    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: u8) {
        tracing::trace!("DCS hook (no-op)");
    }

    fn put(&mut self, _byte: u8) {
        // Consume-only: payload never reaches the screen.
    }

    fn unhook(&mut self) {
        tracing::trace!("DCS/string unhook (no-op)");
    }

    fn start_string(&mut self, kind: u8) {
        tracing::trace!("start string family 0x{kind:02x} (no-op)");
    }
}
