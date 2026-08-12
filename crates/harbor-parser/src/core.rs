//! Byte state machine. No `Screen` dependency — emits into `VtHandler`.

use crate::params::{CsiAccumulator, MAX_OSC_BYTES, MAX_STRING_BYTES, Utf8State};
use crate::perform::VtHandler;

#[cfg(test)]
mod property_tests;

/// High-level ANSI/VT parser states for incremental parsing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum State {
    Ground,
    Escape,
    EscapeIntermediate,
    CsiEntry,
    CsiParam,
    CsiIntermediate,
    CsiIgnore,
    OscString,
    OscStringEscape,
    DcsEntry,
    DcsParam,
    DcsIntermediate,
    DcsPassthrough,
    DcsIgnore,
    DcsEscape,
    SosPmApcString,
    SosPmApcEscape,
}

/// Pure incremental VT parser core.
#[derive(Debug)]
pub struct Parser {
    state: State,
    csi: CsiAccumulator,
    utf8: Utf8State,
    /// OSC payload buffer (capped).
    osc: Vec<u8>,
    osc_overflow: bool,
    /// Count of payload bytes delivered via `put` for DCS/string families.
    string_len: usize,
    string_overflow: bool,
    /// True after a successful `hook`/`start_string` until matching `unhook`.
    hooked: bool,
    /// When true, DcsEscape returns to DcsIgnore and never calls `put`.
    dcs_ignoring: bool,
    /// Whether 8-bit C1 sequences are recognized in Ground and string states.
    c1_enabled: bool,
}

impl Default for Parser {
    fn default() -> Self {
        Self {
            state: State::Ground,
            csi: CsiAccumulator::default(),
            utf8: Utf8State::default(),
            osc: Vec::new(),
            osc_overflow: false,
            string_len: 0,
            string_overflow: false,
            hooked: false,
            dcs_ignoring: false,
            c1_enabled: false,
        }
    }
}

impl Parser {
    /// Advances the state machine by one byte, emitting actions into `handler`.
    pub fn advance<H: VtHandler>(&mut self, handler: &mut H, byte: u8) {
        match self.state {
            State::Ground => self.ground(handler, byte),
            State::Escape => self.escape(handler, byte),
            State::EscapeIntermediate => self.escape_intermediate(handler, byte),
            State::CsiEntry => self.csi_entry(handler, byte),
            State::CsiParam => self.csi_param(handler, byte),
            State::CsiIntermediate => self.csi_intermediate(handler, byte),
            State::CsiIgnore => self.csi_ignore(handler, byte),
            State::OscString => self.osc_string(handler, byte),
            State::OscStringEscape => self.osc_string_escape(handler, byte),
            State::DcsEntry => self.dcs_entry(handler, byte),
            State::DcsParam => self.dcs_param(handler, byte),
            State::DcsIntermediate => self.dcs_intermediate(handler, byte),
            State::DcsPassthrough => self.dcs_passthrough(handler, byte),
            State::DcsIgnore => self.dcs_ignore(handler, byte),
            State::DcsEscape => self.dcs_escape(handler, byte),
            State::SosPmApcString => self.sos_pm_apc_string(handler, byte),
            State::SosPmApcEscape => self.sos_pm_apc_escape(handler, byte),
        }
    }

    #[cfg(test)]
    pub(crate) fn retained_state_within_limits(&self) -> bool {
        self.osc.len() <= MAX_OSC_BYTES
            && self.string_len <= MAX_STRING_BYTES
            && self.utf8.len <= self.utf8.bytes.len()
            && self.csi.retained_state_within_limits()
    }

    /// Configure whether 8-bit C1 sequences are recognized.
    pub fn set_c1_enabled(&mut self, enabled: bool) {
        self.c1_enabled = enabled;
    }

    fn handle_c1<H: VtHandler>(&mut self, handler: &mut H, byte: u8) {
        match byte {
            0x9b => {
                // CSI
                self.clear_csi();
                self.state = State::CsiEntry;
            }
            0x9d => {
                // OSC
                self.utf8.reset();
                self.clear_osc();
                self.state = State::OscString;
            }
            0x90 => {
                // DCS
                self.clear_csi();
                self.clear_string();
                self.state = State::DcsEntry;
            }
            0x98 | 0x9e | 0x9f => {
                // SOS / PM / APC
                self.clear_string();
                handler.start_string(byte - 0x40);
                self.hooked = true;
                self.state = State::SosPmApcString;
            }
            0x9c => {
                // ST
                self.enter_ground();
            }
            _ => {
                let final_char = byte - 0x40;
                handler.esc_dispatch(&[], final_char);
                self.enter_ground();
            }
        }
    }

    fn enter_ground(&mut self) {
        self.state = State::Ground;
    }

    fn clear_csi(&mut self) {
        self.csi.reset();
    }

    fn clear_string(&mut self) {
        self.string_len = 0;
        self.string_overflow = false;
        self.hooked = false;
        self.dcs_ignoring = false;
    }

    /// End a hooked string sequence, calling `dcs_unhook` only if a hook is active.
    fn end_hooked<H: VtHandler>(&mut self, handler: &mut H, terminated: bool) {
        if self.hooked {
            handler.dcs_unhook(terminated);
            self.hooked = false;
        }
        self.string_len = 0;
        self.string_overflow = false;
        self.dcs_ignoring = false;
    }

    fn clear_osc(&mut self) {
        self.osc.clear();
        self.osc_overflow = false;
    }

    // ── Ground ───────────────────────────────────────────────────────────

    fn ground<H: VtHandler>(&mut self, handler: &mut H, byte: u8) {
        match byte {
            0x1b => {
                self.state = State::Escape;
            }
            0x00..=0x1f => handler.execute(byte),
            0x7f => {} // DEL: ignore
            0x20..=0x7e => {
                if self.utf8.len > 0 {
                    self.write_replacement(handler);
                }
                handler.print(byte as char);
            }
            0x80..=0x9f if self.c1_enabled && self.utf8.len == 0 => {
                self.handle_c1(handler, byte);
            }
            _ => self.put_utf8_byte(handler, byte),
        }
    }

    // ── Escape ───────────────────────────────────────────────────────────

    fn escape<H: VtHandler>(&mut self, handler: &mut H, byte: u8) {
        match byte {
            b'[' => {
                self.clear_csi();
                self.state = State::CsiEntry;
            }
            b']' => {
                self.utf8.reset();
                self.clear_osc();
                self.state = State::OscString;
            }
            b'P' => {
                self.clear_csi();
                self.clear_string();
                self.state = State::DcsEntry;
            }
            b'X' | b'^' | b'_' => {
                self.clear_string();
                handler.start_string(byte);
                self.hooked = true;
                self.state = State::SosPmApcString;
            }
            0x20..=0x2f => {
                self.clear_csi();
                self.csi.push_intermediate(byte);
                self.state = State::EscapeIntermediate;
            }
            0x18 | 0x1a => self.enter_ground(),
            0x1b => self.state = State::Escape,
            0x00..=0x1f => {
                // C0 executes but leaves the parser in Escape (historical behavior).
                handler.execute(byte);
            }
            _ => {
                // Final byte (including known c/D/E/M/7/8 and unknown).
                handler.esc_dispatch(&[], byte);
                self.enter_ground();
            }
        }
    }

    fn escape_intermediate<H: VtHandler>(&mut self, handler: &mut H, byte: u8) {
        match byte {
            0x20..=0x2f => {
                self.csi.push_intermediate(byte);
            }
            0x18 | 0x1a => {
                self.clear_csi();
                self.enter_ground();
            }
            0x1b => {
                self.clear_csi();
                self.state = State::Escape;
            }
            0x00..=0x1f => {
                handler.execute(byte);
            }
            _ => {
                let intermediates = self.csi.intermediates().to_vec();
                if !self.csi.malformed() {
                    handler.esc_dispatch(&intermediates, byte);
                }
                self.clear_csi();
                self.enter_ground();
            }
        }
    }

    // ── CSI ──────────────────────────────────────────────────────────────

    fn csi_entry<H: VtHandler>(&mut self, handler: &mut H, byte: u8) {
        match byte {
            0x3c..=0x3f => {
                self.csi.set_private(byte);
                self.state = State::CsiParam;
            }
            b'0'..=b'9' => {
                self.csi.push_digit(byte - b'0');
                self.state = State::CsiParam;
            }
            b';' => {
                self.csi.push_separator();
                self.state = State::CsiParam;
            }
            0x3a => {
                self.csi.push_colon();
                self.state = State::CsiParam;
            }
            0x20..=0x2f => {
                self.csi.push_intermediate(byte);
                self.state = State::CsiIntermediate;
            }
            0x40..=0x7e => self.csi_dispatch_final(handler, byte),
            0x18 | 0x1a => {
                self.clear_csi();
                self.enter_ground();
            }
            0x1b => {
                self.clear_csi();
                self.state = State::Escape;
            }
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => {
                handler.execute(byte);
            }
            _ => {}
        }
    }

    fn csi_param<H: VtHandler>(&mut self, handler: &mut H, byte: u8) {
        match byte {
            0x3c..=0x3f => self.csi.set_malformed(),
            b'0'..=b'9' => self.csi.push_digit(byte - b'0'),
            b';' => self.csi.push_separator(),
            0x3a => self.csi.push_colon(),
            0x20..=0x2f => {
                self.csi.push_intermediate(byte);
                self.state = State::CsiIntermediate;
            }
            0x40..=0x7e => self.csi_dispatch_final(handler, byte),
            0x18 | 0x1a => {
                self.clear_csi();
                self.enter_ground();
            }
            0x1b => {
                self.clear_csi();
                self.state = State::Escape;
            }
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => {
                handler.execute(byte);
            }
            _ => {}
        }
    }

    fn csi_intermediate<H: VtHandler>(&mut self, handler: &mut H, byte: u8) {
        match byte {
            0x20..=0x2f => {
                self.csi.push_intermediate(byte);
            }
            0x30..=0x3f => {
                // Param bytes after intermediate → ignore.
                self.csi.set_malformed();
                self.state = State::CsiIgnore;
            }
            0x40..=0x7e => self.csi_dispatch_final(handler, byte),
            0x18 | 0x1a => {
                self.clear_csi();
                self.enter_ground();
            }
            0x1b => {
                self.clear_csi();
                self.state = State::Escape;
            }
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => {
                handler.execute(byte);
            }
            _ => {}
        }
    }

    fn csi_ignore<H: VtHandler>(&mut self, handler: &mut H, byte: u8) {
        match byte {
            0x40..=0x7e => {
                // Consume final without side effects (malformed path).
                self.clear_csi();
                self.enter_ground();
            }
            0x18 | 0x1a => {
                self.clear_csi();
                self.enter_ground();
            }
            0x1b => {
                self.clear_csi();
                self.state = State::Escape;
            }
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => {
                handler.execute(byte);
            }
            _ => {}
        }
    }

    fn csi_dispatch_final<H: VtHandler>(&mut self, handler: &mut H, action: u8) {
        self.csi.finalize_params();
        let params = self.csi.params();
        let intermediates = self.csi.intermediates().to_vec();
        let private_marker = self.csi.private_marker();
        let malformed = self.csi.malformed();
        if !malformed {
            handler.csi_dispatch(&params, &intermediates, action, private_marker);
        }
        self.clear_csi();
        self.enter_ground();
    }

    // ── OSC ──────────────────────────────────────────────────────────────

    fn osc_string<H: VtHandler>(&mut self, handler: &mut H, byte: u8) {
        match byte {
            0x07 => {
                self.finish_osc(handler, true);
            }
            0x18 | 0x1a => {
                // Abort without dispatch.
                self.clear_osc();
                self.enter_ground();
            }
            0x1b => self.state = State::OscStringEscape,
            0x9c if self.c1_enabled => {
                self.finish_osc(handler, false);
            }
            _ => self.push_osc_byte(byte),
        }
    }

    fn osc_string_escape<H: VtHandler>(&mut self, handler: &mut H, byte: u8) {
        match byte {
            b'\\' => self.finish_osc(handler, false),
            0x9c if self.c1_enabled => self.finish_osc(handler, false),
            0x18 | 0x1a => {
                self.clear_osc();
                self.enter_ground();
            }
            0x1b => self.state = State::OscStringEscape,
            _ => {
                // Not ST: treat ESC as part of payload and resume OSC.
                self.push_osc_byte(0x1b);
                self.push_osc_byte(byte);
                self.state = State::OscString;
            }
        }
    }

    fn push_osc_byte(&mut self, byte: u8) {
        if self.osc.len() < MAX_OSC_BYTES {
            self.osc.push(byte);
        } else {
            self.osc_overflow = true;
        }
    }

    fn finish_osc<H: VtHandler>(&mut self, handler: &mut H, bell_terminated: bool) {
        if !self.osc_overflow {
            // Split on ';' for param slices without allocation of owned strings.
            let parts: Vec<&[u8]> = if self.osc.is_empty() {
                Vec::new()
            } else {
                self.osc.split(|b| *b == b';').collect()
            };
            handler.osc_dispatch(&parts, bell_terminated);
        }
        self.clear_osc();
        self.enter_ground();
    }

    // ── DCS ──────────────────────────────────────────────────────────────

    fn dcs_entry<H: VtHandler>(&mut self, handler: &mut H, byte: u8) {
        match byte {
            0x3c..=0x3f => {
                self.csi.set_private(byte);
                self.state = State::DcsParam;
            }
            b'0'..=b'9' => {
                self.csi.push_digit(byte - b'0');
                self.state = State::DcsParam;
            }
            b';' => {
                self.csi.push_separator();
                self.state = State::DcsParam;
            }
            0x3a => {
                self.csi.push_colon();
                self.state = State::DcsParam;
            }
            0x20..=0x2f => {
                self.csi.push_intermediate(byte);
                self.state = State::DcsIntermediate;
            }
            0x40..=0x7e => self.dcs_hook(handler, byte),
            0x18 | 0x1a => {
                self.clear_csi();
                self.clear_string();
                self.enter_ground();
            }
            0x1b => {
                self.clear_csi();
                self.clear_string();
                self.state = State::Escape;
            }
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => {
                handler.execute(byte);
            }
            _ => {}
        }
    }

    fn dcs_param<H: VtHandler>(&mut self, handler: &mut H, byte: u8) {
        match byte {
            0x3c..=0x3f => self.csi.set_malformed(),
            b'0'..=b'9' => self.csi.push_digit(byte - b'0'),
            b';' => self.csi.push_separator(),
            0x3a => self.csi.push_colon(),
            0x20..=0x2f => {
                self.csi.push_intermediate(byte);
                self.state = State::DcsIntermediate;
            }
            0x40..=0x7e => self.dcs_hook(handler, byte),
            0x18 | 0x1a => {
                self.clear_csi();
                self.clear_string();
                self.enter_ground();
            }
            0x1b => {
                self.clear_csi();
                self.clear_string();
                self.state = State::Escape;
            }
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => {
                handler.execute(byte);
            }
            _ => {}
        }
    }

    fn dcs_intermediate<H: VtHandler>(&mut self, handler: &mut H, byte: u8) {
        match byte {
            0x20..=0x2f => {
                self.csi.push_intermediate(byte);
            }
            0x30..=0x3f => {
                self.csi.set_malformed();
                self.dcs_ignoring = true;
                self.state = State::DcsIgnore;
            }
            0x40..=0x7e => self.dcs_hook(handler, byte),
            0x18 | 0x1a => {
                self.clear_csi();
                self.clear_string();
                self.enter_ground();
            }
            0x1b => {
                self.clear_csi();
                self.clear_string();
                self.state = State::Escape;
            }
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => {
                handler.execute(byte);
            }
            _ => {}
        }
    }

    fn dcs_hook<H: VtHandler>(&mut self, handler: &mut H, action: u8) {
        self.csi.finalize_params();
        let params = self.csi.params();
        let intermediates = self.csi.intermediates().to_vec();
        let ignore = self.csi.malformed();
        self.clear_csi();
        if ignore {
            self.hooked = false;
            self.dcs_ignoring = true;
            self.state = State::DcsIgnore;
        } else {
            handler.dcs_hook(&params, &intermediates, action);
            self.hooked = true;
            self.dcs_ignoring = false;
            self.state = State::DcsPassthrough;
        }
    }

    fn dcs_passthrough<H: VtHandler>(&mut self, handler: &mut H, byte: u8) {
        match byte {
            0x18 | 0x1a => {
                self.end_hooked(handler, false);
                self.enter_ground();
            }
            0x1b => self.state = State::DcsEscape,
            0x9c if self.c1_enabled => {
                self.end_hooked(handler, true);
                self.enter_ground();
            }
            _ => self.put_string_byte(handler, byte),
        }
    }

    fn dcs_ignore<H: VtHandler>(&mut self, handler: &mut H, byte: u8) {
        match byte {
            0x18 | 0x1a => {
                // Only unhook if a hook lifecycle was started (final received).
                self.end_hooked(handler, false);
                self.enter_ground();
            }
            0x1b => {
                self.dcs_ignoring = true;
                self.state = State::DcsEscape;
            }
            0x9c if self.c1_enabled => {
                self.end_hooked(handler, true);
                self.enter_ground();
            }
            _ => {
                // Swallow payload without put.
            }
        }
    }

    fn dcs_escape<H: VtHandler>(&mut self, handler: &mut H, byte: u8) {
        match byte {
            b'\\' | 0x9c if byte == b'\\' || self.c1_enabled => {
                self.end_hooked(handler, true);
                self.enter_ground();
            }
            0x18 | 0x1a => {
                self.end_hooked(handler, false);
                self.enter_ground();
            }
            0x1b => self.state = State::DcsEscape,
            _ => {
                // ESC was not ST: restore prior DCS mode; never put while ignoring.
                if self.dcs_ignoring {
                    self.state = State::DcsIgnore;
                } else {
                    self.put_string_byte(handler, byte);
                    self.state = State::DcsPassthrough;
                }
            }
        }
    }

    // ── SOS / PM / APC ───────────────────────────────────────────────────

    fn sos_pm_apc_string<H: VtHandler>(&mut self, handler: &mut H, byte: u8) {
        match byte {
            0x18 | 0x1a => {
                self.end_hooked(handler, false);
                self.enter_ground();
            }
            0x1b => self.state = State::SosPmApcEscape,
            0x9c if self.c1_enabled => {
                self.end_hooked(handler, true);
                self.enter_ground();
            }
            _ => self.put_string_byte(handler, byte),
        }
    }

    fn sos_pm_apc_escape<H: VtHandler>(&mut self, handler: &mut H, byte: u8) {
        match byte {
            b'\\' | 0x9c if byte == b'\\' || self.c1_enabled => {
                self.end_hooked(handler, true);
                self.enter_ground();
            }
            0x18 | 0x1a => {
                self.end_hooked(handler, false);
                self.enter_ground();
            }
            0x1b => self.state = State::SosPmApcEscape,
            _ => {
                self.put_string_byte(handler, byte);
                self.state = State::SosPmApcString;
            }
        }
    }

    fn put_string_byte<H: VtHandler>(&mut self, handler: &mut H, byte: u8) {
        if self.string_len < MAX_STRING_BYTES {
            handler.dcs_put(byte);
            self.string_len += 1;
        } else {
            self.string_overflow = true;
            // Stop retaining / delivering past the limit; keep scanning for ST.
        }
    }

    // ── UTF-8 ────────────────────────────────────────────────────────────

    fn put_utf8_byte<H: VtHandler>(&mut self, handler: &mut H, byte: u8) {
        if self.utf8.len == self.utf8.bytes.len() {
            self.write_replacement(handler);
        }
        self.utf8.bytes[self.utf8.len] = byte;
        self.utf8.len += 1;

        match std::str::from_utf8(&self.utf8.bytes[..self.utf8.len]) {
            Ok(text) => {
                if let Some(ch) = text.chars().next() {
                    handler.print(ch);
                    self.utf8.reset();
                }
            }
            Err(error) if error.error_len().is_some() => self.write_replacement(handler),
            Err(_) if self.utf8.len == self.utf8.bytes.len() => self.write_replacement(handler),
            Err(_) => {}
        }
    }

    fn write_replacement<H: VtHandler>(&mut self, handler: &mut H) {
        self.utf8.reset();
        handler.print('\u{fffd}');
    }
}
