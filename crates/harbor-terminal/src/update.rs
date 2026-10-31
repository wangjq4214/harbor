//! Owned, read-only projection of one terminal engine state for a single renderer.
use crate::{
    FrameDemand, Preedit, SelectionBounds, TerminalAppearance, TerminalSnapshot, UpdateDamage,
};

/// A coherent engine state at one instant. Reading it never acknowledges visual changes.
///
/// `Ranges` are valid only relative to the most recently acknowledged update of this
/// engine; the first update (and any update after an uncertain projection) is full.
#[derive(Clone, Debug)]
pub struct TerminalUpdate {
    /// Shared coherent projection. Detaching it creates a synthetic, non-acknowledgeable update.
    pub snapshot: std::sync::Arc<TerminalSnapshot>,
    pub damage: UpdateDamage,
    pub selection: Option<SelectionBounds>,
    pub preedit: Option<Preedit>,
    pub appearance: TerminalAppearance,
    pub backdrop_available: bool,
    pub frame_demand: FrameDemand,
    /// Internal projection epoch; invalidation makes old acknowledgements stale.
    pub(crate) projection_epoch: u64,
    pub(crate) acknowledgement: Acknowledgement,
}
#[derive(Clone, Debug)]
pub(crate) struct Acknowledgement {
    pub(crate) snapshot: std::sync::Arc<TerminalSnapshot>,
    pub(crate) revisions: [crate::revision::Revision; 6],
    pub(crate) session: crate::read_model::SessionId,
    pub(crate) presentation: u64,
    pub(crate) full_upload: bool,
}

impl Acknowledgement {
    pub(crate) fn matches_damage(&self, damage: &UpdateDamage) -> bool {
        match (self.full_upload, damage) {
            (true, UpdateDamage::FullUpload) => true,
            (false, UpdateDamage::Ranges(ranges)) => *ranges == self.snapshot.dirty_ranges,
            _ => false,
        }
    }
}

/// One latest projection, not a history of frames. Public owned Vec metadata stays
/// independent whenever an earlier update is still alive.
#[derive(Default)]
pub(crate) struct SnapshotCache {
    latest: Option<(
        [crate::revision::Revision; 4],
        std::sync::Arc<TerminalSnapshot>,
    )>,
}

impl SnapshotCache {
    pub(crate) fn project(&mut self, screen: &crate::Screen) -> std::sync::Arc<TerminalSnapshot> {
        let revisions = screen.mutation_revisions();
        if let Some((old, snapshot)) = &mut self.latest {
            if *old == revisions {
                return snapshot.clone();
            }
            if let Some(unique) = std::sync::Arc::get_mut(snapshot) {
                screen.refresh_terminal_snapshot(unique);
                *old = revisions;
                return snapshot.clone();
            }
        }
        let snapshot = std::sync::Arc::new(screen.terminal_snapshot());
        self.latest = Some((revisions, snapshot.clone()));
        snapshot
    }
}
