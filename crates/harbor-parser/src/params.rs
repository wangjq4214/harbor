//! Fixed-capacity CSI/string accumulators and limits.

pub const MAX_PARAMS: usize = 16;
pub const MAX_SUBPARAMS: usize = 8;
pub const MAX_INTERMEDIATES: usize = 2;
pub const MAX_CSI_PARAM: usize = 65535;
pub const MAX_OSC_BYTES: usize = 4096;
pub const MAX_STRING_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Param {
    pub values: [Option<usize>; MAX_SUBPARAMS],
    pub len: usize,
}

impl Default for Param {
    fn default() -> Self {
        Self {
            values: [None; MAX_SUBPARAMS],
            len: 0,
        }
    }
}

impl Param {
    pub fn get(&self, index: usize) -> Option<usize> {
        self.values.get(index).and_then(|v| *v)
    }
}

/// Fixed CSI parameter list. Empty slots are preserved as `None` (e.g. `CSI ;3 H`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Params {
    pub values: [Param; MAX_PARAMS],
    pub len: usize,
}

impl Params {
    pub fn get(&self, index: usize) -> Option<usize> {
        if index < self.len {
            self.values[index].get(0)
        } else {
            None
        }
    }

    pub fn get_param(&self, index: usize) -> Option<&Param> {
        if index < self.len {
            Some(&self.values[index])
        } else {
            None
        }
    }

    /// Iterates the first sub-parameter of every slot, yielding `None` for empty slots.
    pub fn iter_flat(&self) -> impl Iterator<Item = Option<usize>> + '_ {
        self.values[..self.len].iter().map(|p| p.get(0))
    }

    /// Returns a CSI parameter or `default` for missing/empty parameters.
    pub fn get_or(&self, index: usize, default: usize) -> usize {
        self.get(index).unwrap_or(default)
    }
}

impl From<&[Option<usize>]> for Params {
    fn from(slice: &[Option<usize>]) -> Self {
        let mut values = [Param::default(); MAX_PARAMS];
        let len = slice.len().min(MAX_PARAMS);
        for i in 0..len {
            values[i] = Param {
                values: {
                    let mut vals = [None; MAX_SUBPARAMS];
                    vals[0] = slice[i];
                    vals
                },
                len: 1,
            };
        }
        Params { values, len }
    }
}

/// Accumulator for CSI (and DCS introducer) parameters and intermediates.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CsiAccumulator {
    values: [Param; MAX_PARAMS],
    len: usize,
    /// Digits being accumulated for the current sub-parameter.
    current: Option<usize>,
    /// Sub-parameter list for the current parameter slot.
    current_param: Param,
    /// Holds the private marker byte (e.g. b'?', b'>', b'<', b'=') or 0 if none.
    private: u8,
    /// Set when a parameter or intermediate byte violates expected CSI syntax.
    malformed: bool,
    intermediates: [u8; MAX_INTERMEDIATES],
    intermediate_len: usize,
}

impl CsiAccumulator {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn private_marker(&self) -> Option<u8> {
        if self.private == 0 {
            None
        } else {
            Some(self.private)
        }
    }

    pub fn set_private(&mut self, byte: u8) {
        self.private = byte;
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

    pub fn push_colon(&mut self) {
        if self.current_param.len < MAX_SUBPARAMS {
            self.current_param.values[self.current_param.len] = self.current;
            self.current_param.len += 1;
        } else {
            self.malformed = true;
        }
        self.current = None;
    }

    /// Finishes the current CSI parameter and stores it if there is capacity.
    pub fn push_current(&mut self) {
        // Push the pending sub-parameter first
        if self.current_param.len < MAX_SUBPARAMS {
            self.current_param.values[self.current_param.len] = self.current;
            self.current_param.len += 1;
        } else {
            self.malformed = true;
        }
        self.current = None;

        // Now push the complete Param to self.values
        if self.len < MAX_PARAMS {
            self.values[self.len] = self.current_param;
            self.len += 1;
        }
        self.current_param = Param::default();
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
        if self.current.is_some() || self.current_param.len > 0 || self.len == 0 {
            self.push_current();
        }
    }
}

/// Pending UTF-8 bytes for a possibly split multi-byte character.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Utf8State {
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
