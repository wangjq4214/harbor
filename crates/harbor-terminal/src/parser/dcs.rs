//! Bounded streaming collection for the mutually exclusive DCS query families.

use super::{status_strings, xtgettcap};
use crate::screen::Screen;
use harbor_parser::Params;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RequestKind {
    Decrqss,
    Xtgettcap,
}

/// The current DCS hook can select at most one request family.
#[derive(Debug, Default)]
pub(super) struct DcsCollector {
    pub(super) kind: Option<RequestKind>,
    pub(super) overflow: bool,
    pub(super) payload: Vec<u8>,
}

impl DcsCollector {
    /// Reset on every hook, then select the matching query family if any.
    pub(super) fn hook(&mut self, params: &Params, intermediates: &[u8], action: u8) {
        self.clear();
        if params.len() == 1 && params.get(0).is_none() && action == b'q' {
            self.kind = match intermediates {
                [b'$'] => Some(RequestKind::Decrqss),
                [b'+'] => Some(RequestKind::Xtgettcap),
                _ => None,
            };
        }
    }

    /// Append a payload byte while under the selected family's bound.
    pub(super) fn put(&mut self, byte: u8) {
        let limit = match self.kind {
            Some(RequestKind::Decrqss) => status_strings::MAX_PT,
            Some(RequestKind::Xtgettcap) => xtgettcap::MAX_PT,
            None => return,
        };
        if self.payload.len() < limit {
            self.payload.push(byte);
        } else {
            self.overflow = true;
        }
    }

    /// Cancel silently or queue the selected family's success/failure frame.
    pub(super) fn finish(&mut self, screen: &mut Screen, terminated: bool) {
        let kind = match self.kind {
            Some(kind) if terminated => kind,
            _ => {
                self.clear();
                return;
            }
        };
        let reply = match (kind, self.overflow) {
            (RequestKind::Decrqss, false) => status_strings::build_reply(screen, &self.payload),
            (RequestKind::Decrqss, true) => status_strings::failure_frame(),
            (RequestKind::Xtgettcap, false) => xtgettcap::build_reply(&self.payload),
            (RequestKind::Xtgettcap, true) => xtgettcap::failure_frame(),
        };
        // Reuse the one bounded buffer across families, including after success.
        self.clear();
        screen.push_reply(&reply);
    }

    /// Drop any pending request (e.g. when APC/PM/SOS starts).
    pub(super) fn cancel(&mut self) {
        self.clear();
    }

    fn clear(&mut self) {
        self.kind = None;
        self.overflow = false;
        self.payload.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_payload_buffer_is_reused_across_family_switches() {
        let mut collector = DcsCollector::default();
        let mut screen = Screen::new(3, 5);
        let params = Params::from(&[None][..]);

        collector.hook(&params, b"+", b'q');
        for _ in 0..=xtgettcap::MAX_PT {
            collector.put(b'x');
        }
        let buffer = collector.payload.as_ptr();
        let capacity = collector.payload.capacity();
        collector.finish(&mut screen, true);
        assert_eq!(screen.drain_replies(), b"\x1bP0+r\x1b\\");

        collector.hook(&params, b"$", b'q');
        collector.put(b'm');
        collector.finish(&mut screen, true);
        assert_eq!(screen.drain_replies(), b"\x1bP1$r0m\x1b\\");
        assert!(collector.payload.is_empty());
        assert_eq!(collector.payload.as_ptr(), buffer);
        assert_eq!(collector.payload.capacity(), capacity);

        collector.hook(&params, b"+", b'q');
        for &byte in b"544E" {
            collector.put(byte);
        }
        assert_eq!(collector.payload.as_ptr(), buffer);
        assert_eq!(collector.payload.capacity(), capacity);
        collector.finish(&mut screen, true);
        assert_eq!(
            screen.drain_replies(),
            b"\x1bP1+r544E=787465726D2D323536636F6C6F72\x1b\\"
        );
    }
}
