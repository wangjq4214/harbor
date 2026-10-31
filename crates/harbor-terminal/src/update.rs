//! Owned, read-only projection of one terminal engine state for a single renderer.
use crate::{
    FrameDemand, Preedit, SelectionBounds, TerminalAppearance, TerminalSnapshot, UpdateDamage,
};

/// A coherent engine state at one instant. Reading it never acknowledges visual changes.
///
/// `Ranges` are valid only relative to the most recently acknowledged update of this
/// engine; the first update (and any update after an uncertain projection) is full.
///
/// Engine-issued data cannot be modified:
/// ```compile_fail
/// let mut terminal = harbor_terminal::Terminal::new_headless(2, 4);
/// let mut update = terminal.read_update(std::time::Instant::now());
/// update.damage = harbor_terminal::UpdateDamage::FullUpload;
/// ```
/// Detached candidates cannot consume live damage:
/// ```compile_fail
/// let mut terminal = harbor_terminal::Terminal::new_headless(2, 4);
/// let candidate = terminal.read_update(std::time::Instant::now()).projection().clone();
/// terminal.acknowledge_update(&candidate);
/// ```
#[derive(Clone, Debug)]
pub struct TerminalUpdate {
    pub(crate) projection: TerminalProjection,
    /// Internal projection epoch; invalidation makes old acknowledgements stale.
    pub(crate) projection_epoch: u64,
    pub(crate) acknowledgement: Acknowledgement,
}

/// Renderer-neutral data, including detached configuration candidates and custom
/// synthetic projections. Unlike an engine-issued update, this cannot be acknowledged.
#[derive(Clone, Debug)]
pub struct TerminalProjection {
    pub snapshot: std::sync::Arc<TerminalSnapshot>,
    pub damage: UpdateDamage,
    pub selection: Option<SelectionBounds>,
    pub preedit: Option<Preedit>,
    pub appearance: TerminalAppearance,
    pub backdrop_available: bool,
    pub frame_demand: FrameDemand,
}

/// Engine-issued projections are read-only; clone the projection explicitly for
/// synthetic rendering. There is deliberately no DerefMut or public constructor.
impl std::ops::Deref for TerminalUpdate {
    type Target = TerminalProjection;
    fn deref(&self) -> &Self::Target {
        &self.projection
    }
}

impl TerminalUpdate {
    pub fn projection(&self) -> &TerminalProjection {
        &self.projection
    }
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
