//! Bounded streaming collection shared by DCS query families.
//!
//! DECRQSS (`DCS $ q`) and XTGETTCAP (`DCS + q`) differ only in their
//! introducer byte, payload bound and reply serialization. Start/cancel/overflow
//! handling is identical, so it lives here once.

use harbor_parser::Params;

/// Result of completing one collected request.
pub(super) enum Completion {
    /// No request was active, or the string was cancelled: answer nothing.
    Silent,
    /// The retained payload exceeded the family bound: answer with the failure frame.
    Overflow,
    /// A complete in-bound payload, released to the family for serialization.
    Payload(Vec<u8>),
}

/// One in-flight DCS payload bounded by the owning family's `limit`.
#[derive(Debug)]
pub(super) struct DcsCollector {
    pub(super) active: bool,
    pub(super) overflow: bool,
    pub(super) payload: Vec<u8>,
    limit: usize,
}

impl DcsCollector {
    pub(super) const fn new(limit: usize) -> Self {
        Self {
            active: false,
            overflow: false,
            payload: Vec::new(),
            limit,
        }
    }

    /// Begins collecting when the introducer is exactly `DCS <introducer> q`.
    pub(super) fn hook(
        &mut self,
        params: &Params,
        intermediates: &[u8],
        action: u8,
        introducer: u8,
    ) {
        self.clear();
        self.active = params.len() == 1
            && params.get(0).is_none()
            && intermediates == [introducer].as_slice()
            && action == b'q';
    }

    /// Appends a payload byte while under the family bound.
    pub(super) fn put(&mut self, byte: u8) {
        if !self.active {
            return;
        }
        if self.payload.len() < self.limit {
            self.payload.push(byte);
        } else {
            self.overflow = true;
        }
    }

    /// Ends the request, releasing an in-bound payload to its family.
    pub(super) fn complete(&mut self, terminated: bool) -> Completion {
        let completion = if !self.active || !terminated {
            Completion::Silent
        } else if self.overflow {
            Completion::Overflow
        } else {
            Completion::Payload(std::mem::take(&mut self.payload))
        };
        self.clear();
        completion
    }

    /// Drops any pending request (e.g. when APC/PM/SOS starts).
    pub(super) fn cancel(&mut self) {
        self.clear();
    }

    fn clear(&mut self) {
        self.active = false;
        self.overflow = false;
        self.payload.clear();
    }
}
