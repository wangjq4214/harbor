//! Mutation provenance for state owners participating in renderer acknowledgement.
use std::ops::{Deref, DerefMut};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Revision {
    identity: u64,
    mutation: u64,
}

/// All mutable access advances provenance, including nested field edits and no-ops.
/// This deliberately prefers conservative rejection over reviving an old update.
#[derive(Debug)]
pub(crate) struct Revisioned<T> {
    value: T,
    revision: Revision,
}

impl<T> Revisioned<T> {
    pub(crate) fn into_inner(self) -> T {
        self.value
    }

    pub(crate) fn stamp(&self) -> Revision {
        self.revision
    }

    pub(crate) fn advance(&mut self) {
        self.revision.mutation = self
            .revision
            .mutation
            .checked_add(1)
            .expect("terminal mutation revision exhausted");
    }
}

impl<T> From<T> for Revisioned<T> {
    fn from(value: T) -> Self {
        Self {
            value,
            revision: Revision {
                identity: crate::read_model::fresh_identity(),
                mutation: 0,
            },
        }
    }
}

impl<T: Clone> Clone for Revisioned<T> {
    fn clone(&self) -> Self {
        self.value.clone().into()
    }
}

impl<T> Deref for Revisioned<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> DerefMut for Revisioned<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.advance();
        &mut self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutable_access_and_replacement_never_revive_old_provenance() {
        let mut value: Revisioned<usize> = 1.into();
        let old = value.stamp();
        *value = 2;
        *value = 1;
        assert_ne!(value.stamp(), old);
        assert_ne!(value.clone().stamp(), value.stamp());
        let changed = value.stamp();
        value = 1.into();
        assert_ne!(value.stamp(), changed);
    }
}
