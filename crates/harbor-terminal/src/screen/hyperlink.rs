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
#[derive(Debug, Default, Clone)]
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
    pub(super) fn prepare_retained(
        &self,
        reachable: &HashSet<HyperlinkId>,
    ) -> Result<Self, crate::primary_reflow::PreparationError> {
        use crate::primary_reflow::PreparationError;

        let mut entries = HashMap::new();
        entries
            .try_reserve(reachable.len().min(self.entries.len()))
            .map_err(|_| PreparationError::AllocationFailed)?;
        for (&id, hyperlink) in &self.entries {
            if !reachable.contains(&id) {
                continue;
            }
            let mut uri = String::new();
            uri.try_reserve_exact(hyperlink.uri.len())
                .map_err(|_| PreparationError::AllocationFailed)?;
            uri.push_str(&hyperlink.uri);
            let external_id = match &hyperlink.id {
                Some(value) => {
                    let mut cloned = String::new();
                    cloned
                        .try_reserve_exact(value.len())
                        .map_err(|_| PreparationError::AllocationFailed)?;
                    cloned.push_str(value);
                    Some(cloned)
                }
                None => None,
            };
            entries.insert(
                id,
                Hyperlink {
                    uri,
                    id: external_id,
                },
            );
        }
        Ok(Self {
            entries,
            next_id: self.next_id,
            allocations_since_cleanup: self.allocations_since_cleanup,
        })
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
