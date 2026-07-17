//! Shared retained GPU vertex-grid cache for renderer backends.
//!
//! Both [`super::solid::SolidRenderer`] and [`super::text::TextRenderer`]
//! maintain per-identity vertex buffers keyed by [`crate::RenderIdentity`].
//! This module provides the shared `HashMap` lifecycle so cache-eviction
//! logic lives in one place.

use crate::RenderIdentity;
use std::collections::{HashMap, HashSet};

/// One retained vertex buffer for a particular renderer identity.
pub(crate) struct CachedGrid {
    pub slots: usize,
    pub vertices: wgpu::Buffer,
}

/// Typed cache of retained vertex grids, keyed by renderer identity.
pub(crate) struct GridCache {
    grids: HashMap<RenderIdentity, CachedGrid>,
}

impl GridCache {
    pub(crate) fn new() -> Self {
        Self {
            grids: HashMap::new(),
        }
    }

    pub(crate) fn get(&self, identity: RenderIdentity) -> Option<&CachedGrid> {
        self.grids.get(&identity)
    }

    pub(crate) fn insert(&mut self, identity: RenderIdentity, grid: CachedGrid) {
        self.grids.insert(identity, grid);
    }

    /// Drops every grid whose identity was not visited during the last frame.
    pub(crate) fn retain(&mut self, visited: &HashSet<RenderIdentity>) {
        self.grids.retain(|identity, _| visited.contains(identity));
    }
}
