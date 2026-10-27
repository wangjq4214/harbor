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
    pub(crate) damage: UpdateDamage,
}
