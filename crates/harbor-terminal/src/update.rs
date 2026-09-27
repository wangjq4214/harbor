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
    pub snapshot: TerminalSnapshot,
    pub damage: UpdateDamage,
    pub selection: Option<SelectionBounds>,
    pub preedit: Option<Preedit>,
    pub appearance: TerminalAppearance,
    pub backdrop_available: bool,
    pub frame_demand: FrameDemand,
    /// Internal projection epoch; invalidation makes old acknowledgements stale.
    pub(crate) projection_epoch: u64,
}
