//! Byte state machine. No `Screen` dependency — emits into `Perform`.

use super::params::{CsiAccumulator, MAX_OSC_BYTES, MAX_STRING_BYTES, Utf8State};
use super::perform::Perform;

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
pub(super) struct Parser {
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
    /// Advances the state machine by one byte, emitting actions into `performer`.
    pub fn advance<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match self.state {
            State::Ground => self.ground(performer, byte),
            State::Escape => self.escape(performer, byte),
            State::EscapeIntermediate => self.escape_intermediate(performer, byte),
            State::CsiEntry => self.csi_entry(performer, byte),
            State::CsiParam => self.csi_param(performer, byte),
            State::CsiIntermediate => self.csi_intermediate(performer, byte),
            State::CsiIgnore => self.csi_ignore(performer, byte),
            State::OscString => self.osc_string(performer, byte),
            State::OscStringEscape => self.osc_string_escape(performer, byte),
            State::DcsEntry => self.dcs_entry(performer, byte),
            State::DcsParam => self.dcs_param(performer, byte),
            State::DcsIntermediate => self.dcs_intermediate(performer, byte),
            State::DcsPassthrough => self.dcs_passthrough(performer, byte),
            State::DcsIgnore => self.dcs_ignore(performer, byte),
            State::DcsEscape => self.dcs_escape(performer, byte),
            State::SosPmApcString => self.sos_pm_apc_string(performer, byte),
            State::SosPmApcEscape => self.sos_pm_apc_escape(performer, byte),
        }
    }

    /// Configure whether 8-bit C1 sequences are recognized.
    pub fn set_c1_enabled(&mut self, enabled: bool) {
        self.c1_enabled = enabled;
    }

    fn handle_c1<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x9b => { // CSI
                self.clear_csi();
                self.state = State::CsiEntry;
            }
            0x9d => { // OSC
                self.utf8.reset();
                self.clear_osc();
                self.state = State::OscString;
            }
            0x90 => { // DCS
                self.clear_csi();
                self.clear_string();
                self.state = State::DcsEntry;
            }
            0x98 | 0x9e | 0x9f => { // SOS / PM / APC
                self.clear_string();
                performer.start_string(byte - 0x40);
                self.hooked = true;
                self.state = State::SosPmApcString;
            }
            0x9c => { // ST
                self.enter_ground();
            }
            _ => {
                let final_char = byte - 0x40;
                performer.esc_dispatch(&[], false, final_char);
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

    /// End a hooked string sequence, calling `unhook` only if a hook is active.
    fn end_hooked<P: Perform>(&mut self, performer: &mut P) {
        if self.hooked {
            performer.unhook();
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

    fn ground<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x1b => {
                self.state = State::Escape;
            }
            0x00..=0x1f => performer.execute(byte),
            0x7f => {} // DEL: ignore
            0x20..=0x7e => {
                if self.utf8.len > 0 {
                    self.write_replacement(performer);
                }
                performer.print(byte as char);
            }
            0x80..=0x9f if self.c1_enabled && self.utf8.len == 0 => {
                self.handle_c1(performer, byte);
            }
            _ => self.put_utf8_byte(performer, byte),
        }
    }

    // ── Escape ───────────────────────────────────────────────────────────

    fn escape<P: Perform>(&mut self, performer: &mut P, byte: u8) {
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
                performer.start_string(byte);
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
                performer.execute(byte);
            }
            _ => {
                // Final byte (including known c/D/E/M/7/8 and unknown).
                performer.esc_dispatch(&[], false, byte);
                self.enter_ground();
            }
        }
    }

    fn escape_intermediate<P: Perform>(&mut self, performer: &mut P, byte: u8) {
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
                performer.execute(byte);
            }
            _ => {
                let intermediates = self.csi.intermediates().to_vec();
                let ignore = self.csi.malformed();
                performer.esc_dispatch(&intermediates, ignore, byte);
                self.clear_csi();
                self.enter_ground();
            }
        }
    }

    // ── CSI ──────────────────────────────────────────────────────────────

    fn csi_entry<P: Perform>(&mut self, performer: &mut P, byte: u8) {
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
                self.csi.push_current();
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
            0x40..=0x7e => self.csi_dispatch_final(performer, byte),
            0x18 | 0x1a => {
                self.clear_csi();
                self.enter_ground();
            }
            0x1b => {
                self.clear_csi();
                self.state = State::Escape;
            }
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => {
                performer.execute(byte);
            }
            _ => {}
        }
    }

    fn csi_param<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x3c..=0x3f => self.csi.set_private(byte),
            b'0'..=b'9' => self.csi.push_digit(byte - b'0'),
            b';' => self.csi.push_current(),
            0x3a => self.csi.push_colon(),
            0x20..=0x2f => {
                self.csi.push_intermediate(byte);
                self.state = State::CsiIntermediate;
            }
            0x40..=0x7e => self.csi_dispatch_final(performer, byte),
            0x18 | 0x1a => {
                self.clear_csi();
                self.enter_ground();
            }
            0x1b => {
                self.clear_csi();
                self.state = State::Escape;
            }
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => {
                performer.execute(byte);
            }
            _ => {}
        }
    }

    fn csi_intermediate<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x20..=0x2f => {
                self.csi.push_intermediate(byte);
            }
            0x30..=0x3f => {
                // Param bytes after intermediate → ignore.
                self.csi.set_malformed();
                self.state = State::CsiIgnore;
            }
            0x40..=0x7e => self.csi_dispatch_final(performer, byte),
            0x18 | 0x1a => {
                self.clear_csi();
                self.enter_ground();
            }
            0x1b => {
                self.clear_csi();
                self.state = State::Escape;
            }
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => {
                performer.execute(byte);
            }
            _ => {}
        }
    }

    fn csi_ignore<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x40..=0x7e => {
                // Consume final without side effects (malformed path).
                tracing::warn!(
                    "malformed CSI sequence: params={:?} final=0x{byte:02x} — ignored",
                    self.csi.params().as_slice(),
                );
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
                performer.execute(byte);
            }
            _ => {}
        }
    }

    fn csi_dispatch_final<P: Perform>(&mut self, performer: &mut P, action: u8) {
        self.csi.finalize_params();
        let params = self.csi.params();
        let intermediates = self.csi.intermediates().to_vec();
        let private = self.csi.private();
        let malformed = self.csi.malformed();
        if malformed {
            tracing::warn!(
                "malformed CSI sequence: params={:?} final=0x{action:02x} — ignored",
                params.as_slice(),
            );
        } else {
            performer.csi_dispatch(&params, &intermediates, false, private, action);
        }
        self.clear_csi();
        self.enter_ground();
    }

    // ── OSC ──────────────────────────────────────────────────────────────

    fn osc_string<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x07 => {
                self.finish_osc(performer, true);
            }
            0x18 | 0x1a => {
                // Abort without dispatch.
                self.clear_osc();
                self.enter_ground();
            }
            0x1b => self.state = State::OscStringEscape,
            0x9c if self.c1_enabled => {
                self.finish_osc(performer, false);
            }
            _ => self.push_osc_byte(byte),
        }
    }

    fn osc_string_escape<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            b'\\' => self.finish_osc(performer, false),
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

    fn finish_osc<P: Perform>(&mut self, performer: &mut P, bell_terminated: bool) {
        if self.osc_overflow {
            tracing::warn!("unsupported OSC sequence (overflowed, discarded)");
        } else {
            // Split on ';' for param slices without allocation of owned strings.
            let parts: Vec<&[u8]> = if self.osc.is_empty() {
                Vec::new()
            } else {
                self.osc.split(|b| *b == b';').collect()
            };
            performer.osc_dispatch(&parts, bell_terminated);
        }
        self.clear_osc();
        self.enter_ground();
    }

    // ── DCS ──────────────────────────────────────────────────────────────

    fn dcs_entry<P: Perform>(&mut self, performer: &mut P, byte: u8) {
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
                self.csi.push_current();
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
            0x40..=0x7e => self.dcs_hook(performer, byte),
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
                performer.execute(byte);
            }
            _ => {}
        }
    }

    fn dcs_param<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x3c..=0x3f => self.csi.set_private(byte),
            b'0'..=b'9' => self.csi.push_digit(byte - b'0'),
            b';' => self.csi.push_current(),
            0x3a => self.csi.push_colon(),
            0x20..=0x2f => {
                self.csi.push_intermediate(byte);
                self.state = State::DcsIntermediate;
            }
            0x40..=0x7e => self.dcs_hook(performer, byte),
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
                performer.execute(byte);
            }
            _ => {}
        }
    }

    fn dcs_intermediate<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x20..=0x2f => {
                self.csi.push_intermediate(byte);
            }
            0x30..=0x3f => {
                self.csi.set_malformed();
                self.dcs_ignoring = true;
                self.state = State::DcsIgnore;
            }
            0x40..=0x7e => self.dcs_hook(performer, byte),
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
                performer.execute(byte);
            }
            _ => {}
        }
    }

    fn dcs_hook<P: Perform>(&mut self, performer: &mut P, action: u8) {
        self.csi.finalize_params();
        let params = self.csi.params();
        let intermediates = self.csi.intermediates().to_vec();
        let ignore = self.csi.malformed();
        performer.hook(&params, &intermediates, ignore, action);
        self.hooked = true;
        self.clear_csi();
        if ignore {
            self.dcs_ignoring = true;
            self.state = State::DcsIgnore;
        } else {
            self.dcs_ignoring = false;
            self.state = State::DcsPassthrough;
        }
    }

    fn dcs_passthrough<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x18 | 0x1a => {
                self.end_hooked(performer);
                self.enter_ground();
            }
            0x1b => self.state = State::DcsEscape,
            0x9c if self.c1_enabled => {
                self.end_hooked(performer);
                self.enter_ground();
            }
            _ => self.put_string_byte(performer, byte),
        }
    }

    fn dcs_ignore<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x18 | 0x1a => {
                // Only unhook if a hook lifecycle was started (final received).
                self.end_hooked(performer);
                self.enter_ground();
            }
            0x1b => {
                self.dcs_ignoring = true;
                self.state = State::DcsEscape;
            }
            0x9c if self.c1_enabled => {
                self.end_hooked(performer);
                self.enter_ground();
            }
            _ => {
                // Swallow payload without put.
            }
        }
    }

    fn dcs_escape<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            b'\\' => {
                self.end_hooked(performer);
                self.enter_ground();
            }
            0x18 | 0x1a => {
                self.end_hooked(performer);
                self.enter_ground();
            }
            0x1b => self.state = State::DcsEscape,
            _ => {
                // ESC was not ST: restore prior DCS mode; never put while ignoring.
                if self.dcs_ignoring {
                    self.state = State::DcsIgnore;
                } else {
                    self.put_string_byte(performer, byte);
                    self.state = State::DcsPassthrough;
                }
            }
        }
    }

    // ── SOS / PM / APC ───────────────────────────────────────────────────

    fn sos_pm_apc_string<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x18 | 0x1a => {
                self.end_hooked(performer);
                self.enter_ground();
            }
            0x1b => self.state = State::SosPmApcEscape,
            0x9c if self.c1_enabled => {
                self.end_hooked(performer);
                self.enter_ground();
            }
            _ => self.put_string_byte(performer, byte),
        }
    }

    fn sos_pm_apc_escape<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            b'\\' => {
                self.end_hooked(performer);
                self.enter_ground();
            }
            0x18 | 0x1a => {
                self.end_hooked(performer);
                self.enter_ground();
            }
            0x1b => self.state = State::SosPmApcEscape,
            _ => {
                self.put_string_byte(performer, byte);
                self.state = State::SosPmApcString;
            }
        }
    }

    fn put_string_byte<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        if self.string_len < MAX_STRING_BYTES {
            performer.put(byte);
            self.string_len += 1;
        } else {
            self.string_overflow = true;
            // Stop retaining / delivering past the limit; keep scanning for ST.
        }
    }

    // ── UTF-8 ────────────────────────────────────────────────────────────

    fn put_utf8_byte<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        if self.utf8.len == self.utf8.bytes.len() {
            self.write_replacement(performer);
        }
        self.utf8.bytes[self.utf8.len] = byte;
        self.utf8.len += 1;

        match std::str::from_utf8(&self.utf8.bytes[..self.utf8.len]) {
            Ok(text) => {
                if let Some(ch) = text.chars().next() {
                    performer.print(ch);
                    self.utf8.reset();
                }
            }
            Err(error) if error.error_len().is_some() => self.write_replacement(performer),
            Err(_) if self.utf8.len == self.utf8.bytes.len() => self.write_replacement(performer),
            Err(_) => {}
        }
    }

    fn write_replacement<P: Perform>(&mut self, performer: &mut P) {
        self.utf8.reset();
        performer.print('\u{fffd}');
    }
}
