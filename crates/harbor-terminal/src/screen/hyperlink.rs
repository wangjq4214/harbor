use crate::model::HyperlinkId;
use std::collections::{HashMap, HashSet};
use std::num::NonZeroU32;

const CLEANUP_INTERVAL: usize = 256;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct Hyperlink {
    pub(super) uri: String,
    pub(super) id: Option<String>,
}

/// Screen-local ownership for OSC 8 values referenced by compact cell IDs.
#[derive(Debug, Default)]
pub(super) struct HyperlinkRegistry {
    entries: HashMap<HyperlinkId, Hyperlink>,
    next_id: u32,
    allocations_since_cleanup: usize,
}

impl HyperlinkRegistry {
    pub(super) fn intern(
        &mut self,
        uri: String,
        id: Option<String>,
        reachable: impl FnOnce() -> HashSet<HyperlinkId>,
    ) -> HyperlinkId {
        if let Some((&existing, _)) = self
            .entries
            .iter()
            .find(|(_, value)| value.uri == uri && value.id == id)
        {
            return existing;
        }

        if self.allocations_since_cleanup >= CLEANUP_INTERVAL {
            let reachable = reachable();
            self.entries.retain(|key, _| reachable.contains(key));
            self.allocations_since_cleanup = 0;
        }

        let hyperlink_id = self.allocate_id();
        self.entries.insert(hyperlink_id, Hyperlink { uri, id });
        self.allocations_since_cleanup += 1;
        hyperlink_id
    }

    pub(super) fn get(&self, id: HyperlinkId) -> Option<&Hyperlink> {
        self.entries.get(&id)
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.allocations_since_cleanup = 0;
    }

    fn allocate_id(&mut self) -> HyperlinkId {
        loop {
            self.next_id = self.next_id.wrapping_add(1);
            let Some(raw) = NonZeroU32::new(self.next_id) else {
                continue;
            };
            let candidate = HyperlinkId::from_nonzero(raw);
            if !self.entries.contains_key(&candidate) {
                return candidate;
            }
        }
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }
}
