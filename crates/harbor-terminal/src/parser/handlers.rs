//! Screen-backed `Perform` handler — all current execute/dispatch behavior.

use crate::screen::Screen;
use harbor_parser::Perform;
use harbor_parser::params::Params;

/// Applies recognized VT actions to a `Screen`.
pub struct ScreenHandler<'a> {
    pub screen: &'a mut Screen,
}

impl Perform for ScreenHandler<'_> {
    fn print(&mut self, ch: char) {
        self.screen.write_char(ch);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x05 | 0x07 => {}
            0x08 => self.screen.backspace(),
            0x09 => self.screen.horizontal_tab(),
            0x0a..=0x0c => self.screen.line_feed(),
            0x0d => self.screen.carriage_return(),
            0x0e => self.screen.set_active_charset(1),
            0x0f => self.screen.set_active_charset(0),
            _ => {}
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &Params,
        intermediates: &[u8],
        private_marker: Option<u8>,
        action: u8,
    ) {
        if let Some(private_marker) = private_marker {
            match (private_marker, action) {
                (b'?', b'h' | b'l') => {
                    let enabled = action == b'h';
                    for param in params.iter_flat().flatten() {
                        self.screen.set_private_mode(param, enabled);
                    }
                }
                (b'?', b'J') => {
                    self.screen.selective_erase_display(params.get_or(0, 0));
                }
                (b'?', b'K') => {
                    self.screen.selective_erase_line(params.get_or(0, 0));
                }
                _ => {
                    tracing::warn!(
                        "unsupported private CSI sequence: marker=0x{private_marker:02x} params={:?} final=0x{action:02x}",
                        params.iter_flat().collect::<Vec<_>>(),
                    );
                }
            }
            return;
        }

        if !intermediates.is_empty() {
            if intermediates == [b' '] && action == b'q' {
                self.screen.set_cursor_style(params.get_or(0, 1));
            } else if intermediates == [b'!'] && action == b'p' {
                self.screen.soft_reset();
            } else if intermediates == [b'"'] && action == b'q' {
                self.screen.set_character_protection(params.get_or(0, 0));
            } else if intermediates == [b'$'] {
                match action {
                    b'z' => self.screen.decera(params),
                    b'{' => self.screen.decsera(params),
                    b'x' => self.screen.decfra(params),
                    b'v' => self.screen.deccra(params),
                    b'r' => self.screen.deccara(params),
                    b't' => self.screen.decrara(params),
                    _ => {
                        tracing::warn!(
                            "unsupported CSI intermediates {:?}: params={:?} final=0x{:02x}",
                            intermediates,
                            params.iter_flat().collect::<Vec<_>>(),
                            action,
                        );
                    }
                }
            } else {
                tracing::warn!(
                    "unsupported CSI intermediates {:?}: params={:?} final=0x{:02x}",
                    intermediates,
                    params.iter_flat().collect::<Vec<_>>(),
                    action,
                );
            }
            return;
        }

        match action {
            b'A' => self.screen.cursor_up(params.get_or(0, 1)),
            b'B' => self.screen.cursor_down(params.get_or(0, 1)),
            b'C' => self.screen.cursor_right(params.get_or(0, 1)),
            b'D' => self.screen.cursor_left(params.get_or(0, 1)),
            b'E' => {
                let n = params.get_or(0, 1);
                self.screen.cursor_down(n);
                self.screen.carriage_return();
            }
            b'F' => {
                let n = params.get_or(0, 1);
                self.screen.cursor_up(n);
                self.screen.carriage_return();
            }
            b'G' => {
                self.screen.set_cursor_col(params.get_or(0, 1));
            }
            b'H' | b'f' => {
                self.screen
                    .set_cursor_position(params.get_or(0, 1), params.get_or(1, 1));
            }
            b'J' => self.screen.erase_display(params.get_or(0, 0)),
            b'K' => self.screen.erase_line(params.get_or(0, 0)),
            b'd' => {
                self.screen.set_cursor_row(params.get_or(0, 1));
            }
            b'g' => {
                self.screen.clear_tab_stops(params.get_or(0, 0));
            }
            b'b' => {
                self.screen.repeat_char(params.get_or(0, 1));
            }
            b'm' => self.screen.set_sgr(params),
            b'X' => self.screen.erase_chars(params.get_or(0, 1)),
            b'r' => self
                .screen
                .set_scroll_region(params.get_or(0, 0), params.get_or(1, 0)),
            b's' => {
                if self.screen.margin_mode() {
                    self.screen
                        .set_left_right_margins(params.get_or(0, 0), params.get_or(1, 0));
                } else {
                    self.screen.save_cursor();
                }
            }
            b'u' => self.screen.restore_cursor(),
            b'@' => self.screen.insert_chars(params.get_or(0, 1)),
            b'P' => self.screen.delete_chars(params.get_or(0, 1)),
            b'L' => self.screen.insert_lines(params.get_or(0, 1)),
            b'M' => self.screen.delete_lines(params.get_or(0, 1)),
            b'S' => self.screen.scroll_up_region(params.get_or(0, 1)),
            b'T' => self.screen.scroll_down_region(params.get_or(0, 1)),
            b'h' | b'l' => {
                let enabled = action == b'h';
                for param in params.iter_flat().flatten() {
                    self.screen.set_standard_mode(param, enabled);
                }
            }
            _ => {
                tracing::warn!(
                    "unsupported CSI sequence: params={:?} final=0x{:02x}",
                    params.iter_flat().collect::<Vec<_>>(),
                    action,
                );
            }
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        if ignore {
            return;
        }
        if !intermediates.is_empty() {
            if intermediates == [b'('] {
                self.screen.designate_g0(byte);
            } else if intermediates == [b')'] {
                self.screen.designate_g1(byte);
            } else {
                tracing::warn!(
                    "unsupported escape sequence: ESC intermediates={intermediates:?} 0x{byte:02x}"
                );
            }
            return;
        }

        match byte {
            b'c' => {
                self.screen.reset_display();
            }
            b'D' => {
                self.screen.index();
            }
            b'E' => {
                self.screen.newline();
            }
            b'M' => {
                self.screen.reverse_index();
            }
            b'H' => {
                self.screen.set_tab_stop();
            }
            b'7' => {
                self.screen.save_cursor();
            }
            b'8' => {
                self.screen.restore_cursor();
            }
            b'=' => {
                self.screen.set_application_keypad(true);
            }
            b'>' => {
                self.screen.set_application_keypad(false);
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
