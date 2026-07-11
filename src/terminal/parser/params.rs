//! Fixed-capacity CSI/string accumulators and limits.

/// Maximum number of CSI numeric parameters retained.
pub(super) const MAX_PARAMS: usize = 16;
/// Maximum number of CSI/ESC intermediate bytes retained.
pub(super) const MAX_INTERMEDIATES: usize = 2;
/// Maximum allowed value for a single CSI numeric parameter.
///
/// Parameters exceeding this value are treated as malformed and the entire CSI
/// sequence is ignored with a warning.
pub(super) const MAX_CSI_PARAM: usize = 65535;
/// Cap for OSC payload retention. Past this, stop pushing but keep scanning for
/// terminators.
pub(super) const MAX_OSC_BYTES: usize = 4096;
/// Cap for DCS/APC/PM/SOS payload bytes delivered via `put`.
pub(super) const MAX_STRING_BYTES: usize = 4096;

/// Fixed CSI parameter list. Empty slots are preserved as `None` (e.g. `CSI ;3 H`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct Params {
    values: [Option<usize>; MAX_PARAMS],
    len: usize,
}

impl Params {
    pub fn get(&self, index: usize) -> Option<usize> {
        if index < self.len {
            self.values[index]
        } else {
            None
        }
    }

    pub fn as_slice(&self) -> &[Option<usize>] {
        &self.values[..self.len]
    }
}

/// Accumulator for CSI (and DCS introducer) parameters and intermediates.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct CsiAccumulator {
    values: [Option<usize>; MAX_PARAMS],
    len: usize,
    /// Digits being accumulated for the current parameter.
    current: Option<usize>,
    /// Whether this is a private CSI sequence such as `CSI ? 1049 h`.
    private: bool,
    /// Set when a parameter or intermediate byte violates expected CSI syntax.
    malformed: bool,
    intermediates: [u8; MAX_INTERMEDIATES],
    intermediate_len: usize,
}

impl CsiAccumulator {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn private(&self) -> bool {
        self.private
    }

    pub fn set_private(&mut self) {
        self.private = true;
    }

    pub fn malformed(&self) -> bool {
        self.malformed
    }

    pub fn set_malformed(&mut self) {
        self.malformed = true;
    }

    pub fn params(&self) -> Params {
        Params {
            values: self.values,
            len: self.len,
        }
    }

    pub fn intermediates(&self) -> &[u8] {
        &self.intermediates[..self.intermediate_len]
    }

    /// Finishes the current CSI parameter and stores it if there is capacity.
    ///
    /// Empty parameters are represented as `None` so dispatch can apply
    /// sequence-specific defaults.
    pub fn push_current(&mut self) {
        if self.len < MAX_PARAMS {
            self.values[self.len] = self.current;
            self.len += 1;
        } else {
            tracing::warn!(
                "CSI parameter buffer full (max {MAX_PARAMS}), dropping extra parameters",
            );
        }
        self.current = None;
    }

    pub fn push_digit(&mut self, digit: u8) {
        let d = usize::from(digit);
        let current = self.current.unwrap_or(0);
        let value = current.saturating_mul(10).saturating_add(d);
        if value > MAX_CSI_PARAM {
            self.malformed = true;
        }
        self.current = Some(value);
    }

    pub fn push_intermediate(&mut self, byte: u8) {
        if self.intermediate_len < MAX_INTERMEDIATES {
            self.intermediates[self.intermediate_len] = byte;
            self.intermediate_len += 1;
        } else {
            self.malformed = true;
        }
    }

    /// Finalize params before dispatch: push trailing current if needed.
    pub fn finalize_params(&mut self) {
        if self.current.is_some() || self.len == 0 {
            self.push_current();
        }
    }
}

/// Pending UTF-8 bytes for a possibly split multi-byte character.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct Utf8State {
    /// Buffered UTF-8 bytes. Four bytes is the maximum length of one Unicode scalar value.
    pub bytes: [u8; 4],
    /// Number of valid bytes currently stored in `bytes`.
    pub len: usize,
}

impl Utf8State {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
