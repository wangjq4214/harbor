//! Screen-backed `VtHandler` adapter — all current execute/dispatch behavior.

use super::device_attributes::{PRIMARY_REPLY, SECONDARY_REPLY, accepts_default_query};
use super::mode_query;
use super::osc_color;
use super::osc7;
use super::osc8;
use super::osc133;
use super::status_strings::DecrqssRequest;
use super::xtgettcap::XtgettcapRequest;
use crate::model::{CharacterProtection, CursorStyleArg};
use crate::screen::Screen;

use crate::TerminalOutputEvent;
use harbor_parser::{Params, VtHandler};

/// Applies recognized VT actions to a `Screen`.
pub struct ScreenHandler<'a> {
    pub screen: &'a mut Screen,
    pub decrqss: &'a mut DecrqssRequest,
    pub xtgettcap: &'a mut XtgettcapRequest,
    pub output_events: &'a mut Vec<TerminalOutputEvent>,
}

impl VtHandler for ScreenHandler<'_> {
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
        action: u8,
        private_marker: Option<u8>,
    ) {
        if intermediates == b"$" && action == b'p' && matches!(private_marker, None | Some(b'?')) {
            if let Some(param) = mode_query::param(params) {
                let private = private_marker == Some(b'?');
                let reply =
                    mode_query::reply(param, self.screen.mode_status(private, param), private);
                self.screen.push_reply(&reply);
            }
            return;
        }

        match private_marker {
            Some(b'?') => {
                match action {
                    b'h' | b'l' => {
                        let enabled = action == b'h';
                        for param in params.iter_flat().flatten() {
                            self.screen.set_private_mode(param, enabled);
                        }
                    }
                    b'J' => self.screen.selective_erase_display(params.get_or(0, 0)),
                    b'K' => self.screen.selective_erase_line(params.get_or(0, 0)),
                    b'n' => {
                        let mode = params.get_or(0, 0);
                        if mode == 6 {
                            // Private CPR (Cursor Position Report)
                            let (row, col) = self.screen.cpr_coordinates();
                            let reply = format!("\x1b[?{};{}R", row, col);
                            self.screen.push_reply(reply.as_bytes());
                        }
                    }
                    _ => {
                        tracing::warn!(
                            "unsupported private CSI sequence: params={:?} final=0x{action:02x}",
                            params.iter_flat().collect::<Vec<_>>(),
                        );
                    }
                }
                return;
            }
            Some(b'>') => {
                if intermediates.is_empty() && action == b'c' && accepts_default_query(params) {
                    self.screen.push_reply(SECONDARY_REPLY);
                }
                return;
            }
            Some(b'=') => return,
            Some(marker) => {
                tracing::warn!(
                    "unsupported CSI private marker 0x{marker:02x}: params={:?} final=0x{action:02x}",
                    params.iter_flat().collect::<Vec<_>>(),
                );
                return;
            }
            None => {}
        }

        if !intermediates.is_empty() {
            if intermediates == *b" " && action == b'q' {
                self.screen
                    .set_cursor_style(CursorStyleArg::from_param(params.get_or(0, 1)));
            } else if intermediates == *b"!" && action == b'p' {
                self.screen.soft_reset();
            } else if intermediates == *b"\"" && action == b'q' {
                self.screen
                    .set_character_protection(CharacterProtection::from_param(params.get_or(0, 0)));
            } else if intermediates == *b"$" {
                match action {
                    b'z' => self.screen.decera(params),
                    b'{' => self.screen.decsera(params),
                    b'x' => self.screen.decfra(params),
                    b'v' => self.screen.deccra(params),
                    b'r' => self.screen.deccara(params),
                    b't' => self.screen.decrara(params),
                    _ => tracing::warn!(
                        "unsupported CSI intermediates {intermediates:?}: params={:?} final=0x{action:02x}",
                        params.iter_flat().collect::<Vec<_>>(),
                    ),
                }
            } else {
                tracing::warn!(
                    "unsupported CSI intermediates {intermediates:?}: params={:?} final=0x{action:02x}",
                    params.iter_flat().collect::<Vec<_>>(),
                );
            }
            return;
        }

        match action {
            b'c' => {
                if accepts_default_query(params) {
                    self.screen.push_reply(PRIMARY_REPLY);
                }
            }
            b'A' => self.screen.cursor_up(params.get_or(0, 1)),
            // VPR (`e`) is an alias of CUD (`B`).
            b'B' | b'e' => self.screen.cursor_down(params.get_or(0, 1)),
            // HPR (`a`) is an alias of CUF (`C`).
            b'C' | b'a' => self.screen.cursor_right(params.get_or(0, 1)),
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
            // HPA (`` ` ``) is an alias of CHA (`G`).
            b'G' | b'`' => self.screen.set_cursor_col(params.get_or(0, 1)),
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
            b'I' => self.screen.forward_tab(params.get_or(0, 1).max(1)),
            b'Z' => self.screen.backward_tab(params.get_or(0, 1).max(1)),
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
            b'n' => {
                let mode = params.get_or(0, 0);
                match mode {
                    5 => {
                        // Device Status Report: status OK
                        self.screen.push_reply(b"\x1b[0n");
                    }
                    6 => {
                        // Standard CPR
                        let (row, col) = self.screen.cpr_coordinates();
                        let reply = format!("\x1b[{};{}R", row, col);
                        self.screen.push_reply(reply.as_bytes());
                    }
                    _ => {}
                }
            }
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

    fn esc_dispatch(&mut self, intermediates: &[u8], byte: u8) {
        if !intermediates.is_empty() {
            match (intermediates, byte) {
                (b"(", charset) => self.screen.designate_g0(charset),
                (b")", charset) => self.screen.designate_g1(charset),
                (b"*", charset) => self.screen.designate_g2(charset),
                (b"+", charset) => self.screen.designate_g3(charset),
                (b"#", b'8') => self.screen.decaln(),
                _ => {}
            }
            return;
        }

        match byte {
            b'N' => {
                self.screen.single_shift_2();
            }
            b'O' => {
                self.screen.single_shift_3();
            }
            b'c' => {
                self.screen.reset_display();
                self.output_events.push(TerminalOutputEvent::TitleReset);
                self.output_events
                    .push(TerminalOutputEvent::WorkingDirectoryReset);
                self.output_events
                    .push(TerminalOutputEvent::ShellIntegrationReset);
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
            b'Z' => self.screen.push_reply(PRIMARY_REPLY),
            _ => {
                tracing::warn!("unsupported escape sequence: ESC 0x{byte:02x}");
            }
        }
    }

    fn osc_dispatch(&mut self, command: &[u8], payload: &[u8], bell_terminated: bool) {
        if matches!(command, b"10" | b"11" | b"12" | b"110" | b"111" | b"112") {
            match osc_color::parse(command, payload) {
                Some(osc_color::Action::Set(slot, rgb)) => {
                    self.screen.set_default_color_rgb(slot, rgb);
                }
                Some(osc_color::Action::Query(slot)) => {
                    let reply = osc_color::format_query(
                        slot,
                        self.screen.default_color(slot),
                        bell_terminated,
                    );
                    self.screen.push_reply(reply.as_bytes());
                }
                Some(osc_color::Action::Reset(slot)) => {
                    self.screen.reset_default_color(slot);
                }
                None => {}
            }
            return;
        }
        if command == b"8" {
            match osc8::parse(payload) {
                Some(osc8::Action::Open { uri, id }) => self.screen.open_hyperlink(uri, id),
                Some(osc8::Action::Close) => self.screen.close_hyperlink(),
                None => {}
            }
            return;
        }
        if command == b"7" {
            if payload.is_empty() {
                self.output_events
                    .push(TerminalOutputEvent::WorkingDirectoryReset);
            } else if let Some(metadata) = osc7::parse(payload) {
                self.output_events
                    .push(TerminalOutputEvent::WorkingDirectoryChanged(metadata));
            }
            return;
        }
        if command == b"133" {
            if payload.is_empty() {
                self.output_events
                    .push(TerminalOutputEvent::ShellIntegrationReset);
            } else if let Some(marker) = osc133::parse(payload) {
                self.output_events
                    .push(TerminalOutputEvent::ShellIntegration(marker));
            }
            return;
        }
        if !matches!(command, b"0" | b"1" | b"2") {
            return;
        }
        if payload.is_empty() {
            self.output_events.push(TerminalOutputEvent::TitleReset);
            return;
        }
        let Ok(title) = std::str::from_utf8(payload) else {
            return;
        };
        let mut chars = title.chars();
        if chars.by_ref().take(257).count() > 256 || title.chars().any(char::is_control) {
            return;
        }
        self.output_events
            .push(TerminalOutputEvent::TitleChanged(title.to_owned()));
    }

    fn dcs_hook(&mut self, params: &Params, intermediates: &[u8], action: u8) {
        self.decrqss.hook(params, intermediates, action);
        self.xtgettcap.hook(params, intermediates, action);
    }

    fn dcs_put(&mut self, byte: u8) {
        self.decrqss.put(byte);
        self.xtgettcap.put(byte);
    }

    fn dcs_unhook(&mut self, terminated: bool) {
        self.decrqss.finish(self.screen, terminated);
        self.xtgettcap.finish(self.screen, terminated);
    }

    fn start_string(&mut self, kind: u8) {
        self.decrqss.cancel();
        self.xtgettcap.cancel();
        tracing::trace!("start string family 0x{kind:02x}");
    }
}
