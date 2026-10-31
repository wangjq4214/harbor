//! Terminal screen and buffer lifecycle owner.
//!
//! Screen retains session colors, replies, focus and synchronized output.
//! BufferState owns only buffer-local storage, cursor/pen, hyperlinks and anchors.
//! Switching buffers never swaps session properties; pointer selections consume
//! the same authoritative BufferTransition without becoming Screen-owned state.

mod cursor;
mod default_colors;
mod edit;
mod focus_reporting;
#[cfg(test)]
mod history_tests;
mod hyperlink;
mod synchronized_output;
#[cfg(test)]
mod tests;

use self::default_colors::DefaultColors;
use crate::content_anchor::{
    Affinity, AnchorMutation, AnchorMutationBatch, ContentAnchor, ContentProjection,
};
use crate::logical_content::{DecodeError, LogicalAtomOffset};
use crate::normal_buf::{CellsIter, LogicalLineId};
use crate::primary_reflow::{
    CursorRetention, PreparationError, PreparedPrimaryResize, PreparedProjection, ReflowViewport,
};
use crate::revision::{Revision, Revisioned};
use crate::selection_model::GenPos;
use crate::{DirtyRange, InputModes, NormalBuf};
use harbor_parser::Params;

use self::cursor::CursorEngine;
use self::edit::{CellOps, CellWriter, PenState};
use self::focus_reporting::FocusReporting;
use self::synchronized_output::SynchronizedOutput;
use harbor_config::{Palette, Rgba};
use hyperlink::HyperlinkRegistry;
use std::collections::HashSet;
use unicode_width::UnicodeWidthChar;

pub(crate) use self::default_colors::DefaultColorSlot;

// ── re-exports ────────────────────────────────────────────────────────

pub use crate::model::AltScreenAction;
pub use crate::model::Cell;
pub use crate::model::CellAttrs;
pub use crate::model::CharacterProtection;
pub use crate::model::CursorShape;
pub use crate::model::CursorStyleArg;
pub use crate::model::SelectionBounds;
pub use harbor_config::Color;

/// State reported by DECRPM for a queried terminal mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ModeStatus {
    Unknown,
    Set,
    Reset,
}

impl ModeStatus {
    pub(crate) const fn code(self) -> usize {
        match self {
            Self::Unknown => 0,
            Self::Set => 1,
            Self::Reset => 2,
        }
    }
}

impl From<bool> for ModeStatus {
    fn from(enabled: bool) -> Self {
        if enabled { Self::Set } else { Self::Reset }
    }
}

// ── Screen ────────────────────────────────────────────────────────────

/// Visible terminal screen state rendered by the text pipeline.
///
/// Owns the active/saved/parked buffers and session-wide protocol state. It does not
/// parse byte streams; `TerminalParser` calls these methods after recognizing control sequences.
#[derive(Debug)]
pub struct Screen {
    /// Accepted primary policy, retained even while alternate backing has zero history.
    history_limits: harbor_config::HistoryLimits,
    /// Active buffer state; session properties never participate in a buffer swap.
    active: BufferState,
    /// Pending alt-screen request set by the parser, consumed by I/O.
    alt_request: Option<AltScreenAction>,
    /// Primary screen saved while the alternate screen is active.
    /// Invariant: `Some` iff in alt screen (`is_alt()` is true).
    saved_primary: Option<Box<BufferState>>,
    /// Latest prepared primary geometry while ConPTY defers resize in alt mode.
    /// The original saved primary remains the source for every intervening resize.
    deferred_primary_resize: Vec<PreparedBufferResize>,
    /// Alternate screen parked across `?47` exit/re-enter.
    /// A parked buffer is a leaf and cannot carry another saved primary.
    parked_alt: Option<Box<BufferState>>,
    /// Outgoing VT replies buffer.
    pub(crate) replies: Vec<u8>,
    /// Session-owned `?1004` mode and latest observed native-window focus.
    focus_reporting: FocusReporting,
    /// Session-owned `?2026` nesting; preserved across alt-screen swap.
    synchronized_output: Revisioned<SynchronizedOutput>,
    /// Startup and active OSC default and indexed colors, owned by the terminal session.
    default_colors: Revisioned<DefaultColors>,
}
#[derive(Debug)]
struct EditAnchorCapture {
    anchor: ContentAnchor,
    atom_count: usize,
    affected_atoms: usize,
    shifted_atoms: usize,
    dropped_atoms: Option<(usize, usize)>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BufferRole {
    Primary,
    Alternate,
}

#[derive(Debug)]
struct RectangularSelectionProjection {
    source: ContentProjection,
    source_live_top: u64,
    target_live_top: u64,
    rows: usize,
    cols: usize,
    retained_cells: Vec<bool>,
}

/// Buffer-level result of preparing one resize role, before shared session state.
#[derive(Debug)]
struct RoleBuffers {
    normal: NormalBuf,
    projection: ContentProjection,
    cursor: CursorEngine,
    review_anchor: Option<ContentAnchor>,
    rectangular_selection: Option<RectangularSelectionProjection>,
}

#[derive(Debug)]
struct PreparedBufferResize {
    normal: NormalBuf,
    review_anchor: Option<ContentAnchor>,
    cursor: CursorEngine,
    pen_state: PenState,
    hyperlinks: HyperlinkRegistry,
    projection: ContentProjection,
    rectangular_selection: Option<RectangularSelectionProjection>,
}

#[derive(Debug)]
pub(crate) struct PreparedScreenResize {
    history_limits: harbor_config::HistoryLimits,
    active: PreparedBufferResize,
    immediate_saved_primary: Vec<PreparedBufferResize>,
    saved_primary: Vec<PreparedBufferResize>,
    deferred_primary_source: Option<ContentProjection>,
    parked_alt: Vec<PreparedBufferResize>,
}

impl PreparedBufferResize {
    pub(crate) fn snapshot(&self, source: &Screen) -> crate::model::TerminalSnapshot {
        let rows = self.normal.rows();
        let cols = self.normal.cols();
        let mut snapshot = source.terminal_snapshot();
        snapshot.rows = rows;
        snapshot.cols = cols;
        snapshot.cells = self.normal.snapshot_cells();
        snapshot.cursor_x = self.cursor.cursor_x();
        snapshot.cursor_y = self.cursor.cursor_y(&self.normal);
        snapshot.scroll_count = self.normal.scroll_count();
        snapshot.view_offset = self.normal.view_offset();
        snapshot.history_start = self.normal.history_start();
        snapshot.wrapped = (0..rows).map(|r| self.normal.is_wrapped(r)).collect();
        snapshot.dirty_ranges = self.normal.dirty_ranges();
        snapshot
    }
    fn project_selection(&self, anchor: ContentAnchor) -> Option<(GenPos, ContentAnchor)> {
        let position = if let Some(rectangular) = &self.rectangular_selection {
            let source = rectangular.source.resolve_selection(anchor)?;
            let row = usize::try_from(source.generation.checked_sub(rectangular.source_live_top)?)
                .ok()?;
            if row >= rectangular.rows || source.col >= rectangular.cols {
                return None;
            }
            let index = row.checked_mul(rectangular.cols)?.checked_add(source.col)?;
            if !rectangular
                .retained_cells
                .get(index)
                .copied()
                .unwrap_or(false)
            {
                return None;
            }
            GenPos::new(
                rectangular
                    .target_live_top
                    .checked_add(u64::try_from(row).ok()?)?,
                source.col,
            )
        } else {
            self.projection.resolve_selection(anchor)?
        };
        let refreshed = self.projection.to_anchor(position, anchor.affinity)?;
        Some((position, refreshed))
    }
}

impl PreparedScreenResize {
    pub(crate) fn snapshot(&self, source: &Screen) -> crate::model::TerminalSnapshot {
        self.active.snapshot(source)
    }

    pub(crate) fn project_active_selection(
        &self,
        anchor: ContentAnchor,
    ) -> Option<(GenPos, ContentAnchor)> {
        self.active.project_selection(anchor)
    }

    pub(crate) fn project_saved_primary_selection(
        &self,
        anchor: ContentAnchor,
    ) -> Option<(GenPos, ContentAnchor)> {
        if let Some(source) = &self.deferred_primary_source {
            let position = source.resolve_selection(anchor)?;
            return Some((position, source.to_anchor(position, anchor.affinity)?));
        }
        self.saved_primary.first()?.project_selection(anchor)
    }

    pub(crate) fn project_parked_alt_selection(
        &self,
        anchor: ContentAnchor,
    ) -> Option<(GenPos, ContentAnchor)> {
        self.parked_alt.first()?.project_selection(anchor)
    }

    pub(crate) fn has_saved_primary(&self) -> bool {
        !self.saved_primary.is_empty()
    }

    pub(crate) fn has_parked_alt(&self) -> bool {
        !self.parked_alt.is_empty()
    }
}

impl Screen {
    pub fn new(rows: usize, cols: usize) -> Self {
        Self::with_palette(rows, cols, Palette::default())
    }

    pub(crate) fn with_palette(rows: usize, cols: usize, palette: Palette) -> Self {
        Self::with_palette_and_history(rows, cols, palette, harbor_config::HistoryLimits::default())
    }

    pub(crate) fn with_palette_and_history(
        rows: usize,
        cols: usize,
        palette: Palette,
        limits: harbor_config::HistoryLimits,
    ) -> Self {
        let rows = rows.max(1);
        let cols = cols.max(1);
        Self {
            history_limits: limits,
            active: BufferState::new(rows, cols, limits),
            alt_request: None,
            saved_primary: None,
            deferred_primary_resize: Vec::new(),
            parked_alt: None,
            replies: Vec::new(),
            focus_reporting: FocusReporting::default(),
            synchronized_output: SynchronizedOutput::default().into(),
            default_colors: DefaultColors::new(palette).into(),
        }
    }

    pub fn history_limits(&self) -> harbor_config::HistoryLimits {
        self.history_limits
    }

    /// Primary scrollback only, including primary saved behind alternate.
    pub fn history_content_bytes(&self) -> usize {
        self.saved_primary.as_deref().map_or_else(
            || self.active.normal.history_content_bytes(),
            |buffer| buffer.normal.history_content_bytes(),
        )
    }

    pub(crate) fn active_palette(&self) -> Palette {
        self.default_colors.active_palette()
    }

    pub(crate) fn configuration_palette(&self, baseline: Palette) -> Palette {
        let mut colors = *self.default_colors;
        colors.advance_baseline(baseline);
        colors.active_palette()
    }

    pub(crate) fn accept_configuration_palette(&mut self, baseline: Palette) {
        self.default_colors.advance_baseline(baseline);
        self.mark_all_dirty();
    }

    pub(crate) fn default_color(&self, slot: DefaultColorSlot) -> Rgba {
        self.default_colors.get(slot)
    }

    pub(crate) fn set_default_color_rgb(&mut self, slot: DefaultColorSlot, rgb: [u8; 3]) {
        if self.default_colors.set_rgb(slot, rgb) {
            self.mark_all_dirty();
        }
    }

    pub(crate) fn reset_default_color(&mut self, slot: DefaultColorSlot) {
        if self.default_colors.reset(slot) {
            self.mark_all_dirty();
        }
    }

    pub(crate) fn indexed_color(&self, index: u8) -> Rgba {
        self.default_colors.indexed(index)
    }

    pub(crate) fn set_indexed_color_rgb(&mut self, index: u8, rgb: [u8; 3]) {
        if self.default_colors.set_indexed_rgb(index, rgb) {
            self.mark_all_dirty();
        }
    }

    pub(crate) fn reset_indexed_color(&mut self, index: u8) {
        if self.default_colors.reset_indexed(index) {
            self.mark_all_dirty();
        }
    }

    // ── dimensions / viewport ──────────────────────────────────────────

    pub fn rows(&self) -> usize {
        self.active.normal.rows()
    }

    pub fn cols(&self) -> usize {
        self.active.normal.cols()
    }

    pub fn scroll_count(&self) -> usize {
        self.active.normal.scroll_count()
    }

    pub(crate) fn saved_primary_scroll_count(&self) -> usize {
        self.saved_primary
            .as_deref()
            .map_or(0, |buffer| buffer.normal.scroll_count())
    }

    pub fn view_offset(&self) -> usize {
        self.active.normal.view_offset()
    }

    pub fn history_start(&self) -> u64 {
        self.active.normal.history_start()
    }

    // ── cursor queries ─────────────────────────────────────────────────

    pub fn cursor_x(&self) -> usize {
        self.active.cursor.cursor_x()
    }

    pub fn cursor_y(&self) -> usize {
        self.active.cursor.cursor_y(&self.active.normal)
    }

    #[cfg(test)]
    pub(crate) fn pending_wrap(&self) -> bool {
        self.active.cursor.modes.pending_wrap
    }

    pub fn cursor_shape(&self) -> CursorShape {
        self.active.cursor.cursor_shape()
    }

    pub fn cursor_blink(&self) -> bool {
        self.active.cursor.cursor_blink()
    }

    pub fn cursor_visible(&self) -> bool {
        self.active.cursor.cursor_visible()
    }

    /// Current SGR foreground, background, and attributes as observed for DECRQSS.
    pub(crate) fn current_sgr(&self) -> (Color, Color, CellAttrs, Color) {
        (
            self.active.pen_state.pen.fg,
            self.active.pen_state.pen.bg,
            self.active.pen_state.pen.attrs,
            self.active.pen_state.pen.underline_color,
        )
    }

    /// 1-based inclusive DECSTBM top/bottom bounds.
    pub(crate) fn scroll_region(&self) -> (usize, usize) {
        (
            self.active.cursor.scroll_region.top + 1,
            self.active.cursor.scroll_region.bottom + 1,
        )
    }

    /// 1-based inclusive saved DECSLRM left/right bounds.
    pub(crate) fn left_right_margins(&self) -> (usize, usize) {
        (
            self.active.cursor.margins.left + 1,
            self.active.cursor.margins.right + 1,
        )
    }

    /// Canonical DECSCUSR style derived from current shape and blink.
    pub(crate) fn cursor_style(&self) -> CursorStyleArg {
        match (
            self.active.cursor.cursor.shape,
            self.active.cursor.cursor.blink,
        ) {
            (CursorShape::Block, true) => CursorStyleArg::BlinkingBlock,
            (CursorShape::Block, false) => CursorStyleArg::SteadyBlock,
            (CursorShape::Underline, true) => CursorStyleArg::BlinkingUnderline,
            (CursorShape::Underline, false) => CursorStyleArg::SteadyUnderline,
            (CursorShape::Bar, true) => CursorStyleArg::BlinkingBar,
            (CursorShape::Bar, false) => CursorStyleArg::SteadyBar,
        }
    }

    /// Current DECSCA protection applied to newly written cells.
    pub(crate) fn character_protection(&self) -> CharacterProtection {
        if self.active.pen_state.pen.protected {
            CharacterProtection::Protected
        } else {
            CharacterProtection::Unprotected
        }
    }

    pub fn push_reply(&mut self, reply: &[u8]) {
        if self.replies.len() + reply.len() <= 1024 {
            self.replies.extend_from_slice(reply);
        } else {
            tracing::warn!("Terminal reply buffer limit (1024 bytes) reached. Discarding reply.");
        }
    }

    pub fn drain_replies(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.replies)
    }

    /// Returns the 1-based (row, col) coordinates relative to origin/margins if DECOM is enabled.
    pub fn cpr_coordinates(&self) -> (usize, usize) {
        let row = if self.active.cursor.modes.origin {
            self.active
                .cursor
                .cursor_y(&self.active.normal)
                .saturating_sub(self.active.cursor.scroll_region.top)
                + 1
        } else {
            self.active.cursor.cursor_y(&self.active.normal) + 1
        };
        let col = if self.active.cursor.modes.origin && self.active.cursor.margins.enabled {
            self.active
                .cursor
                .cursor_x()
                .saturating_sub(self.active.cursor.margins.left)
                + 1
        } else {
            self.active.cursor.cursor_x() + 1
        };
        (row, col)
    }

    pub fn set_cursor_style(&mut self, arg: CursorStyleArg) {
        self.active.cursor.set_cursor_style(arg);
    }

    pub fn input_modes(&self) -> InputModes {
        self.active.cursor.input_modes()
    }

    pub fn margin_mode(&self) -> bool {
        self.active.cursor.margin_mode()
    }

    // ── cell access ────────────────────────────────────────────────────

    pub fn cells(&self) -> CellsIter<'_> {
        self.active.normal.cells()
    }

    pub fn cell_char(&self, row: usize, col: usize) -> char {
        self.active.normal.cell(row, col).ch
    }

    pub fn cell(&self, row: usize, col: usize) -> &Cell {
        self.active.normal.cell(row, col)
    }

    pub fn cell_at_generation(&self, generation: u64, col: usize) -> Option<&Cell> {
        self.active.normal.cell_at_generation(generation, col)
    }

    pub(crate) fn hyperlink_at_generation(
        &self,
        generation: u64,
        col: usize,
    ) -> Option<(crate::model::HyperlinkId, &str)> {
        let id = self
            .active
            .normal
            .cell_at_generation(generation, col)?
            .hyperlink?;
        Some((id, self.active.hyperlinks.get(id)?.uri.as_str()))
    }

    pub(crate) fn read_buffer(&self) -> &NormalBuf {
        &self.active.normal
    }

    pub(crate) fn read_hyperlink(
        &self,
        id: crate::model::HyperlinkId,
    ) -> Option<(&str, Option<&str>)> {
        let link = self.active.hyperlinks.get(id)?;
        Some((&link.uri, link.id.as_deref()))
    }
    pub(crate) fn open_hyperlink(&mut self, uri: String, external_id: Option<String>) {
        let BufferState {
            normal,
            pen_state,
            hyperlinks,
            ..
        } = &mut self.active;
        let id = hyperlinks.intern(uri, external_id, || {
            let mut reachable: HashSet<_> = normal
                .retained_cells()
                .filter_map(|cell| cell.hyperlink)
                .collect();
            reachable.extend(pen_state.hyperlink_ids());
            reachable
        });
        pen_state.active_hyperlink = Some(id);
    }

    pub(crate) fn close_hyperlink(&mut self) {
        self.active.pen_state.active_hyperlink = None;
    }

    pub fn is_wrapped_at_generation(&self, generation: u64) -> bool {
        self.active
            .normal
            .is_wrapped_at_generation(generation)
            .unwrap_or(false)
    }

    /// Direct cell mutation for test setup.
    #[cfg(test)]
    pub fn cell_mut(&mut self, row: usize, col: usize) -> &mut Cell {
        self.active.normal.cell_mut(row, col)
    }

    pub(crate) fn content_projection(&self) -> Result<ContentProjection, DecodeError> {
        ContentProjection::build(&self.active.normal)
    }

    #[cfg(test)]
    pub(crate) fn prepare_primary_resize(
        &self,
        requested_rows: usize,
        requested_cols: usize,
    ) -> Result<PreparedPrimaryResize, PreparationError> {
        self.active.prepare_primary_resize_with_viewport(
            requested_rows,
            requested_cols,
            ReflowViewport::PullHistory,
            self.history_limits,
            &self.active.normal,
        )
    }

    fn live_cursor_position(&self) -> GenPos {
        self.active.live_cursor_position()
    }

    fn capture_edit_anchor(
        &self,
        affected_cells: usize,
        dropped_cells: Option<(usize, usize)>,
    ) -> Option<EditAnchorCapture> {
        if !self.tracks_content_anchors() {
            return None;
        }
        let projection = self.content_projection().ok()?;
        let position = self.live_cursor_position();
        let anchor = projection.to_anchor(position, Affinity::Before)?;
        let atom_count = projection.line_atom_count(anchor.line_id)?;
        let affected_atoms = projection.atom_count_in_cell_range(
            anchor.line_id,
            position.generation,
            position.col,
            position.col.saturating_add(affected_cells),
        )?;
        let dropped_atoms = if let Some((start, end)) = dropped_cells {
            projection.atom_range_in_cell_range(anchor.line_id, position.generation, start, end)?
        } else {
            None
        };
        let shifted_atoms = dropped_cells.map_or(0, |(start, _)| {
            projection
                .atom_count_in_cell_range(anchor.line_id, position.generation, position.col, start)
                .unwrap_or(0)
        });
        Some(EditAnchorCapture {
            anchor,
            atom_count,
            affected_atoms,
            shifted_atoms,
            dropped_atoms,
        })
    }

    fn capture_insertion_anchor(&self, requested: usize) -> (usize, Option<EditAnchorCapture>) {
        let requested_col = self.active.cursor.cursor.x;
        let (left, right) = if self.active.cursor.margins.enabled {
            (
                self.active.cursor.margins.left,
                self.active.cursor.margins.right,
            )
        } else {
            (0, self.active.normal.cols().saturating_sub(1))
        };
        let base = CellOps::wide_range(
            &self.active.normal,
            self.active.cursor.cursor.y,
            requested_col,
        )
        .map_or(requested_col, |(base, _)| base);
        let actual = if requested_col >= left && requested_col <= right {
            requested.min(right - base + 1)
        } else {
            0
        };
        let dropped = (actual > 0).then_some((right + 1 - actual, right.saturating_add(1)));
        (actual, self.capture_edit_anchor(actual, dropped))
    }

    fn record_insertion_delta(
        &mut self,
        before: Option<EditAnchorCapture>,
        inserted_atom_capacity: usize,
        inserted_is_meaningful: bool,
    ) {
        let Some(EditAnchorCapture {
            anchor,
            shifted_atoms,
            dropped_atoms,
            ..
        }) = before
        else {
            return;
        };
        let inserted_atoms = if inserted_is_meaningful || shifted_atoms > 0 {
            inserted_atom_capacity
        } else {
            0
        };
        if inserted_atoms > 0 {
            self.active.anchor_mutations.push(AnchorMutation::Insert {
                line_id: anchor.line_id,
                at: anchor.offset,
                count: inserted_atoms,
            });
        }
        if let Some((dropped_start, dropped_count)) = dropped_atoms {
            self.active.anchor_mutations.push(AnchorMutation::Delete {
                line_id: anchor.line_id,
                start: LogicalAtomOffset(dropped_start + inserted_atoms),
                end: LogicalAtomOffset(dropped_start + inserted_atoms + dropped_count),
            });
        }
        self.active
            .anchor_mutations
            .push(AnchorMutation::ReprojectLine(anchor.line_id));
    }

    fn record_deletion_delta(&mut self, before: Option<EditAnchorCapture>) {
        let Some(EditAnchorCapture {
            anchor,
            atom_count: before_count,
            affected_atoms,
            ..
        }) = before
        else {
            return;
        };
        let Ok(after) = self.content_projection() else {
            self.active
                .anchor_mutations
                .push(AnchorMutation::InvalidateLine(anchor.line_id));
            return;
        };
        let Some(after_count) = after.line_atom_count(anchor.line_id) else {
            self.active
                .anchor_mutations
                .push(AnchorMutation::InvalidateLine(anchor.line_id));
            return;
        };
        let removed = affected_atoms.min(before_count.saturating_sub(anchor.offset.0));
        if removed > 0 {
            self.active.anchor_mutations.push(AnchorMutation::Delete {
                line_id: anchor.line_id,
                start: anchor.offset,
                end: LogicalAtomOffset(anchor.offset.0 + removed),
            });
        }
        let retained_after_delete = before_count.saturating_sub(removed);
        let inserted_tail = after_count.saturating_sub(retained_after_delete);
        if inserted_tail > 0 {
            self.active.anchor_mutations.push(AnchorMutation::Insert {
                line_id: anchor.line_id,
                at: LogicalAtomOffset(retained_after_delete),
                count: inserted_tail,
            });
        }
        self.active
            .anchor_mutations
            .push(AnchorMutation::ReprojectLine(anchor.line_id));
    }

    fn record_structural_atom_delta(
        &mut self,
        line_id: LogicalLineId,
        before: &[(u64, usize, usize)],
        after: &[(u64, usize, usize)],
    ) {
        if before.len() == after.len() {
            if before
                .iter()
                .zip(after)
                .any(|(old, new)| (old.1, old.2) != (new.1, new.2))
            {
                self.active
                    .anchor_mutations
                    .push(AnchorMutation::ReprojectLine(line_id));
            }
            return;
        }
        let prefix = before
            .iter()
            .zip(after.iter())
            .take_while(|(old, new)| old == new)
            .count();
        let mut suffix = 0;
        while suffix < before.len().saturating_sub(prefix)
            && suffix < after.len().saturating_sub(prefix)
            && before[before.len() - 1 - suffix] == after[after.len() - 1 - suffix]
        {
            suffix += 1;
        }
        let old_count = before.len() - prefix - suffix;
        let new_count = after.len() - prefix - suffix;
        self.active.anchor_mutations.push(AnchorMutation::Replace {
            line_id,
            start: LogicalAtomOffset(prefix),
            old_count,
            new_count,
        });
    }

    fn sync_review_anchor(&mut self) {
        if self.active.normal.view_offset() == 0 {
            self.active.review_anchor = None;
            return;
        }
        let generation = self.active.normal.history_start()
            + self
                .active
                .normal
                .scroll_count()
                .saturating_sub(self.active.normal.view_offset()) as u64;
        self.active.review_anchor = self.content_projection().ok().and_then(|projection| {
            projection.to_anchor(GenPos::new(generation, 0), Affinity::Before)
        });
    }

    pub(crate) fn finish_anchor_mutations(
        &mut self,
        before: Option<&ContentProjection>,
    ) -> Result<(AnchorMutationBatch, ContentProjection), DecodeError> {
        let projection = self.content_projection()?;
        if let Some(before) = before {
            for line_id in before.line_ids() {
                if !projection.contains_line(line_id) {
                    self.active
                        .anchor_mutations
                        .push(AnchorMutation::InvalidateLine(line_id));
                } else if let Some(first_retained_generation) = projection.first_generation(line_id)
                    && before.first_generation(line_id) != Some(first_retained_generation)
                    && projection.history_start() > before.history_start()
                    && let (Some(before_start), Some(after_start)) =
                        (before.atom_start(line_id), projection.atom_start(line_id))
                    && after_start.0 > before_start.0
                {
                    self.active
                        .anchor_mutations
                        .push(AnchorMutation::EvictPrefix {
                            line_id,
                            end: after_start,
                        });
                } else if !self.active.anchor_mutations.affects_line(line_id)
                    && let (Some(old_spans), Some(new_spans)) =
                        (before.atom_spans(line_id), projection.atom_spans(line_id))
                {
                    self.record_structural_atom_delta(line_id, &old_spans, &new_spans);
                }
            }
        }

        let mutations = std::mem::take(&mut self.active.anchor_mutations);
        let globally_invalidated = mutations
            .iter()
            .any(|mutation| matches!(mutation, AnchorMutation::InvalidateAll));
        let mut invalidate_saved = false;
        if !globally_invalidated
            && let Some(saved) = self.active.cursor.cursor.saved.as_mut()
            && let Some(anchor) = saved.anchor
        {
            let adjusted = mutations.apply(anchor);
            saved.anchor = adjusted;
            match adjusted {
                None => invalidate_saved = true,
                Some(adjusted) => {
                    let live_top = self.active.normal.history_start()
                        + self.active.normal.scroll_count() as u64;
                    let old_generation = live_top.saturating_add(saved.cursor_y as u64);
                    let needs_projection = adjusted != anchor
                        || mutations.affects_line(adjusted.line_id)
                        || !projection.contains_generation(adjusted.line_id, old_generation);
                    if needs_projection {
                        if let Some(projected) = projection.resolve_cursor(adjusted)
                            && let Ok(row) =
                                usize::try_from(projected.pos.generation.saturating_sub(live_top))
                            && row < self.active.normal.rows()
                        {
                            saved.cursor_y = row;
                            if adjusted != anchor || mutations.affects_line(adjusted.line_id) {
                                saved.cursor_x = projected.pos.col;
                                saved.pending_wrap = projected.pending_wrap;
                            }
                        } else {
                            invalidate_saved = true;
                        }
                    }
                }
            }
        }
        if invalidate_saved {
            self.active.cursor.cursor.saved = None;
        }

        if let Some(anchor) = self.active.review_anchor {
            self.active.review_anchor =
                if globally_invalidated && projection.contains_line(anchor.line_id) {
                    Some(anchor)
                } else {
                    mutations.apply(anchor)
                }
                .or_else(|| projection.oldest_anchor());
            if let Some(offset) = self
                .active
                .review_anchor
                .and_then(|value| projection.view_offset_for(value))
            {
                self.active.normal.set_view_offset(offset);
            }
        }

        Ok((mutations, projection))
    }

    // ── read-only queries ──────────────────────────────────────────────

    pub fn terminal_snapshot(&self) -> crate::model::TerminalSnapshot {
        self.snapshot_with_storage(Vec::new(), Vec::new())
    }

    /// Only a uniquely owned cached snapshot may lend its vectors for reuse.
    pub(crate) fn refresh_terminal_snapshot(&self, snapshot: &mut crate::model::TerminalSnapshot) {
        let wrapped = std::mem::take(&mut snapshot.wrapped);
        let damage = std::mem::take(&mut snapshot.dirty_ranges);
        *snapshot = self.snapshot_with_storage(wrapped, damage);
    }

    fn snapshot_with_storage(
        &self,
        mut wrapped: Vec<bool>,
        mut dirty_ranges: Vec<DirtyRange>,
    ) -> crate::model::TerminalSnapshot {
        let rows = self.rows();
        wrapped.clear();
        wrapped.extend((0..rows).map(|row| self.is_wrapped(row)));
        self.active.normal.dirty_ranges_into(&mut dirty_ranges);
        let cols = self.cols();
        let cells = self.active.normal.snapshot_cells();
        crate::model::TerminalSnapshot {
            rows,
            cols,
            cells,
            cursor_x: self.cursor_x(),
            cursor_y: self.cursor_y(),
            cursor_visible: self.cursor_visible(),
            cursor_blink: self.cursor_blink(),
            cursor_shape: self.cursor_shape(),
            scroll_count: self.scroll_count(),
            view_offset: self.view_offset(),
            history_start: self.history_start(),
            wrapped,
            is_alt: self.is_alt(),
            input_modes: self.input_modes(),
            dirty_ranges,
        }
    }
    /// State-owner provenance, independent of damage consumption and read-model revisions.
    pub(crate) fn mutation_revisions(&self) -> [Revision; 4] {
        [
            self.active.normal.stamp(),
            self.active.cursor.stamp(),
            self.default_colors.stamp(),
            self.synchronized_output.stamp(),
        ]
    }

    pub fn selected_text(&self, bounds: SelectionBounds) -> String {
        match crate::logical_content::selected_text(&self.active.normal, bounds) {
            Ok(text) => text,
            Err(error) => {
                tracing::error!(generation = error.generation, column = error.column, kind = ?error.kind, "selected text decode failed");
                String::new()
            }
        }
    }

    // ── dirty tracking ─────────────────────────────────────────────────

    pub fn dirty_rows(&self) -> Vec<usize> {
        self.active.normal.dirty_rows()
    }

    pub fn dirty_ranges(&self) -> Vec<DirtyRange> {
        self.active.normal.dirty_ranges()
    }

    pub fn clear_dirty(&mut self) {
        self.active.normal.clear_dirty()
    }

    pub fn mark_row_dirty(&mut self, row: usize) {
        self.active.normal.mark_row_dirty(row);
    }

    pub fn mark_rows_dirty(&mut self, start_row: usize, end_row: usize) {
        self.active.normal.mark_rows_dirty(start_row, end_row);
    }

    pub fn mark_range_dirty(&mut self, row: usize, start_col: usize, end_col: usize) {
        self.active.normal.mark_range_dirty(row, start_col, end_col);
    }

    pub fn mark_all_dirty(&mut self) {
        self.active.normal.mark_all_dirty();
    }

    // ── viewport scroll ────────────────────────────────────────────────

    pub fn scroll_up(&mut self, n: usize) {
        self.active.normal.scroll_up(n);
        self.sync_review_anchor();
    }

    pub fn scroll_down(&mut self, n: usize) {
        self.active.normal.scroll_down(n);
        self.sync_review_anchor();
    }

    pub fn scroll_to_bottom(&mut self) {
        self.active.normal.scroll_to_bottom();
        self.sync_review_anchor();
    }

    /// Converts a mouse wheel delta (line or pixel) to row changes and scrolls the primary screen.
    /// Alt-screen wheel events are consumed without scrolling. Returns the signed line count.
    pub fn scroll_wheel(&mut self, dy: f32, is_pixel: bool) -> isize {
        if self.is_alt() {
            return 0;
        }
        let lines = if is_pixel {
            (dy / 20.0) as isize
        } else {
            (dy * 3.0) as isize
        };
        if lines > 0 {
            self.scroll_up(lines as usize);
        } else if lines < 0 {
            self.scroll_down(lines.unsigned_abs());
        }
        lines
    }

    // ── alt screen ─────────────────────────────────────────────────────

    pub fn is_alt(&self) -> bool {
        self.saved_primary.is_some()
    }

    #[cfg(test)]
    pub(crate) fn has_parked_alt(&self) -> bool {
        self.parked_alt.is_some()
    }

    pub fn request_alt_enter(&mut self, clear: bool) {
        self.alt_request = Some(AltScreenAction::Enter { clear });
    }

    pub fn request_alt_exit(&mut self) {
        self.alt_request = Some(AltScreenAction::Exit);
    }

    pub fn alt_request(&self) -> Option<AltScreenAction> {
        self.alt_request
    }

    pub fn take_alt_request(&mut self) -> Option<AltScreenAction> {
        self.alt_request.take()
    }

    /// Standalone buffer switch. Terminal-owned selections use transition_buffer.
    pub fn enter_alt(&mut self, clear: bool) {
        let _ = self.transition_buffer(AltScreenAction::Enter { clear });
    }

    pub fn exit_alt(&mut self) {
        let _ = self.transition_buffer(AltScreenAction::Exit);
    }

    /// The single buffer lifecycle decision, consumed by the pointer owner too.
    pub(crate) fn transition_buffer(
        &mut self,
        action: AltScreenAction,
    ) -> Option<BufferTransition> {
        match action {
            AltScreenAction::Enter { clear } if !self.is_alt() => {
                self.finish_transition_anchors();
                let parked = if clear { None } else { self.parked_alt.take() };
                let restored_parked = parked.is_some();
                let alternate = parked.map(|buffer| *buffer).unwrap_or_else(|| {
                    BufferState::new(
                        self.rows(),
                        self.cols(),
                        harbor_config::HistoryLimits {
                            max_rows: 0,
                            max_bytes: 0,
                        },
                    )
                });
                self.parked_alt = None;
                let primary = std::mem::replace(&mut self.active, alternate);
                self.saved_primary = Some(Box::new(primary));
                if restored_parked {
                    self.mark_all_dirty();
                }
                Some(BufferTransition::Entered { restored_parked })
            }
            AltScreenAction::Exit if self.is_alt() => {
                self.finish_transition_anchors();
                if let Some(prepared) = self.deferred_primary_resize.pop()
                    && let Some(primary) = self.saved_primary.as_deref_mut()
                {
                    primary.commit_resize(prepared);
                }
                let primary = self
                    .saved_primary
                    .take()
                    .expect("active alternate has primary");
                let alternate = std::mem::replace(&mut self.active, *primary);
                self.parked_alt = Some(Box::new(alternate));
                self.mark_all_dirty();
                Some(BufferTransition::Exited)
            }
            _ => None,
        }
    }

    fn finish_transition_anchors(&mut self) {
        if self.finish_anchor_mutations(None).is_err() {
            self.active.cursor.cursor.saved = None;
            self.active.review_anchor = None;
            self.active.anchor_mutations = AnchorMutationBatch::default();
        }
    }

    // ── resize ─────────────────────────────────────────────────────────

    pub(crate) fn prepare_resize(
        &self,
        rows: usize,
        cols: usize,
    ) -> Result<PreparedScreenResize, PreparationError> {
        self.prepare_resize_with_viewport(rows, cols, ReflowViewport::PullHistory)
    }

    pub(crate) fn prepare_resize_with_viewport(
        &self,
        rows: usize,
        cols: usize,
        viewport: ReflowViewport,
    ) -> Result<PreparedScreenResize, PreparationError> {
        self.prepare_resize_with_history(rows, cols, viewport, self.history_limits)
    }

    pub(crate) fn prepare_resize_with_history(
        &self,
        rows: usize,
        cols: usize,
        viewport: ReflowViewport,
        limits: harbor_config::HistoryLimits,
    ) -> Result<PreparedScreenResize, PreparationError> {
        let rows = rows.max(1);
        let cols = cols.max(2);
        let role = if self.is_alt() {
            BufferRole::Alternate
        } else {
            BufferRole::Primary
        };
        let active = self
            .active
            .prepare_resize_for_role(rows, cols, role, viewport, limits, None)?;
        let mut immediate_saved_primary = Vec::new();
        let mut saved_primary = Vec::new();
        let mut deferred_primary_source = None;
        if let Some(saved) = self.saved_primary.as_deref() {
            saved_primary
                .try_reserve_exact(1)
                .map_err(|_| PreparationError::AllocationFailed)?;
            if viewport == ReflowViewport::PreserveLiveTop {
                if limits != saved.normal.history_limits() {
                    immediate_saved_primary
                        .try_reserve_exact(1)
                        .map_err(|_| PreparationError::AllocationFailed)?;
                    immediate_saved_primary.push(saved.prepare_resize_for_role(
                        saved.rows(),
                        saved.cols(),
                        BufferRole::Primary,
                        viewport,
                        limits,
                        None,
                    )?);
                }
                let source = immediate_saved_primary
                    .first()
                    .map_or(&*saved.normal, |p| &p.normal);
                deferred_primary_source = Some(ContentProjection::build(source)?);
                // Deferred geometry must use the already-trimmed source, never resurrect
                // content evicted by the immediate accepted shrink.
                saved_primary.push(saved.prepare_resize_for_role(
                    rows,
                    cols,
                    BufferRole::Primary,
                    viewport,
                    limits,
                    Some(source),
                )?);
            } else {
                saved_primary.push(saved.prepare_resize_for_role(
                    rows,
                    cols,
                    BufferRole::Primary,
                    viewport,
                    limits,
                    None,
                )?);
            }
        }
        let mut parked_alt = Vec::new();
        if let Some(alt) = self.parked_alt.as_deref() {
            parked_alt
                .try_reserve_exact(1)
                .map_err(|_| PreparationError::AllocationFailed)?;
            parked_alt.push(alt.prepare_resize_for_role(
                rows,
                cols,
                BufferRole::Alternate,
                viewport,
                limits,
                None,
            )?);
        }

        Ok(PreparedScreenResize {
            history_limits: limits,
            active,
            immediate_saved_primary,
            saved_primary,
            deferred_primary_source,
            parked_alt,
        })
    }

    pub(crate) fn commit_resize(&mut self, prepared: PreparedScreenResize) {
        let PreparedScreenResize {
            history_limits,
            active,
            mut immediate_saved_primary,
            mut saved_primary,
            deferred_primary_source,
            mut parked_alt,
        } = prepared;
        self.active.commit_resize(active);
        self.history_limits = history_limits;
        if deferred_primary_source.is_some() {
            if let (Some(primary), Some(prepared)) = (
                self.saved_primary.as_deref_mut(),
                immediate_saved_primary.pop(),
            ) {
                primary.commit_resize(prepared);
            }
            self.deferred_primary_resize = saved_primary;
        } else {
            self.deferred_primary_resize.clear();
            match (self.saved_primary.as_deref_mut(), saved_primary.pop()) {
                (Some(buffer), Some(prepared)) => buffer.commit_resize(prepared),
                (None, None) => {}
                _ => debug_assert!(false, "saved primary changed during resize preparation"),
            }
        }
        match (self.parked_alt.as_deref_mut(), parked_alt.pop()) {
            (Some(buffer), Some(prepared)) => buffer.commit_resize(prepared),
            (None, None) => {}
            _ => debug_assert!(false, "parked alternate changed during resize preparation"),
        }
    }

    /// Resizes this standalone model through the same detached preparation path as `Terminal`.
    ///
    /// Callers that also own a PTY should use `Terminal` so PTY and model commit transactionally.
    pub fn resize(&mut self, rows: usize, cols: usize) {
        match self.prepare_resize(rows, cols) {
            Ok(prepared) => self.commit_resize(prepared),
            Err(error) => {
                tracing::error!(?error, "failed to prepare standalone screen resize");
            }
        }
    }

    // ── cursor movement (delegated) ────────────────────────────────────

    pub fn cursor_up(&mut self, n: usize) {
        self.active.cursor.cursor_up(n);
    }

    pub fn cursor_down(&mut self, n: usize) {
        self.active.cursor.cursor_down(&self.active.normal, n);
    }

    pub fn cursor_left(&mut self, n: usize) {
        self.active.cursor.cursor_left(n);
    }

    pub fn cursor_right(&mut self, n: usize) {
        self.active.cursor.cursor_right(&self.active.normal, n);
    }

    pub fn carriage_return(&mut self) {
        self.active.cursor.carriage_return();
    }

    pub fn backspace(&mut self) {
        self.active.cursor.backspace(&self.active.normal);
    }

    pub fn set_cursor_position(&mut self, row_1_based: usize, col_1_based: usize) {
        self.active
            .cursor
            .set_cursor_position(&self.active.normal, row_1_based, col_1_based);
    }

    pub fn set_cursor_col(&mut self, col_1_based: usize) {
        self.active
            .cursor
            .set_cursor_col(&self.active.normal, col_1_based);
    }

    pub fn set_cursor_row(&mut self, row_1_based: usize) {
        self.active
            .cursor
            .set_cursor_row(&self.active.normal, row_1_based);
    }

    pub fn home_cursor(&mut self) {
        self.active.cursor.home_cursor();
    }

    // ── scroll region / margins ────────────────────────────────────────

    pub fn set_scroll_region(&mut self, top: usize, bottom: usize) {
        self.active
            .cursor
            .set_scroll_region(&self.active.normal, top, bottom);
    }

    pub fn set_left_right_margins(&mut self, left: usize, right: usize) {
        self.active
            .cursor
            .set_left_right_margins(&self.active.normal, left, right);
    }

    // ── modes ──────────────────────────────────────────────────────────

    pub fn set_private_mode(&mut self, param: usize, enabled: bool) {
        match param {
            FocusReporting::MODE => self.focus_reporting.set_enabled(enabled),
            47 => {
                if enabled {
                    self.request_alt_enter(false);
                } else {
                    self.request_alt_exit();
                }
            }
            1047 | 1049 => {
                if enabled {
                    self.request_alt_enter(true);
                } else {
                    self.request_alt_exit();
                }
            }
            1048 => {
                if enabled {
                    self.save_cursor();
                } else {
                    self.restore_cursor();
                }
            }
            SynchronizedOutput::MODE => {
                if enabled {
                    self.synchronized_output.enable();
                } else {
                    self.synchronized_output.disable();
                }
            }
            other => {
                if !self.active.cursor.set_private_mode(other, enabled) {
                    tracing::warn!("unsupported private mode: ?{}", other);
                }
            }
        }
    }

    pub fn set_standard_mode(&mut self, param: usize, enabled: bool) {
        if !self.active.cursor.set_standard_mode(param, enabled) {
            tracing::warn!("unsupported standard mode: {}", param);
        }
    }

    pub(crate) fn mode_status(&self, private: bool, param: usize) -> ModeStatus {
        let enabled = if private {
            match param {
                FocusReporting::MODE => return self.focus_reporting.mode_status(),
                47 | 1047 | 1049 => Some(self.is_alt()),
                1048 => Some(self.active.cursor.cursor.saved.is_some()),
                SynchronizedOutput::MODE => return self.synchronized_output.mode_status(),
                _ => self.active.cursor.private_mode_enabled(param),
            }
        } else {
            self.active.cursor.standard_mode_enabled(param)
        };
        enabled.map(ModeStatus::from).unwrap_or(ModeStatus::Unknown)
    }

    pub fn set_application_keypad(&mut self, enabled: bool) {
        self.active.cursor.set_application_keypad(enabled);
    }

    pub(crate) fn ordinary_present_eligible(&self) -> bool {
        self.synchronized_output.ordinary_present_eligible()
    }

    pub(crate) fn observe_focus(&mut self, event: crate::TerminalFocusEvent) -> bool {
        self.focus_reporting.observe(event)
    }

    pub(crate) fn clear_synchronized_output(&mut self) {
        self.synchronized_output.clear();
    }

    // ── SGR / charsets / protection ────────────────────────────────────

    pub fn set_sgr(&mut self, params: &Params) {
        self.active.pen_state.set_sgr(params);
    }

    pub fn set_sgr_slice(&mut self, slice: &[Option<usize>]) {
        self.active.pen_state.set_sgr_slice(slice);
    }

    pub fn designate_g0(&mut self, charset: u8) {
        self.active.pen_state.designate_g0(charset);
    }

    pub fn designate_g1(&mut self, charset: u8) {
        self.active.pen_state.designate_g1(charset);
    }

    pub fn designate_g2(&mut self, charset: u8) {
        self.active.pen_state.designate_g2(charset);
    }

    pub fn designate_g3(&mut self, charset: u8) {
        self.active.pen_state.designate_g3(charset);
    }

    pub fn single_shift_2(&mut self) {
        self.active.pen_state.single_shift_2();
    }

    pub fn single_shift_3(&mut self) {
        self.active.pen_state.single_shift_3();
    }

    pub fn set_active_charset(&mut self, active: u8) {
        self.active.pen_state.set_active_charset(active);
    }

    pub fn set_character_protection(&mut self, arg: CharacterProtection) {
        self.active.pen_state.set_character_protection(arg);
    }

    // ── erase ──────────────────────────────────────────────────────────

    pub fn erase_display(&mut self, mode: usize) {
        if mode == 3 {
            // Saved-lines erasure is buffer-local and independent of editing margins.
            self.active.cursor.clear_pending_wrap();
            self.active.normal.clear_scrollback();
            self.active.review_anchor = None;
            return;
        }
        let BufferState {
            normal,
            cursor,
            pen_state,
            ..
        } = &mut self.active;
        CellOps::erase_display(pen_state, normal, cursor, mode);
    }

    pub fn erase_line(&mut self, mode: usize) {
        let BufferState {
            normal,
            cursor,
            pen_state,
            ..
        } = &mut self.active;
        CellOps::erase_line(pen_state, normal, cursor, mode);
    }

    pub fn erase_chars(&mut self, n: usize) {
        let BufferState {
            normal,
            cursor,
            pen_state,
            ..
        } = &mut self.active;
        CellOps::erase_chars(pen_state, normal, cursor, n);
    }

    pub fn selective_erase_display(&mut self, mode: usize) {
        let BufferState {
            normal,
            cursor,
            pen_state,
            ..
        } = &mut self.active;
        CellOps::selective_erase_display(pen_state, normal, cursor, mode);
    }

    pub fn selective_erase_line(&mut self, mode: usize) {
        let BufferState {
            normal,
            cursor,
            pen_state,
            ..
        } = &mut self.active;
        CellOps::selective_erase_line(pen_state, normal, cursor, mode);
    }

    // ── insert / delete ────────────────────────────────────────────────

    pub fn insert_chars(&mut self, n: usize) {
        let (actual, before) = self.capture_insertion_anchor(n.max(1));
        let inserted_is_meaningful =
            actual > 0 && NormalBuf::fill_is_meaningful(self.active.pen_state.erase_cell());
        let BufferState {
            normal,
            cursor,
            pen_state,
            ..
        } = &mut self.active;
        if CellOps::insert_chars(pen_state, normal, cursor, n) {
            self.record_insertion_delta(before, actual, inserted_is_meaningful);
        }
    }

    pub fn delete_chars(&mut self, n: usize) {
        let before = self.capture_edit_anchor(n.max(1), None);
        let BufferState {
            normal,
            cursor,
            pen_state,
            ..
        } = &mut self.active;
        if CellOps::delete_chars(pen_state, normal, cursor, n) {
            self.record_deletion_delta(before);
        }
    }

    pub fn insert_lines(&mut self, n: usize) {
        let BufferState {
            normal,
            cursor,
            pen_state,
            ..
        } = &mut self.active;
        CellOps::insert_lines(pen_state, normal, cursor, n);
    }

    pub fn delete_lines(&mut self, n: usize) {
        let BufferState {
            normal,
            cursor,
            pen_state,
            ..
        } = &mut self.active;
        CellOps::delete_lines(pen_state, normal, cursor, n);
    }

    // ── scroll region (CSI S / CSI T) ──────────────────────────────────

    pub fn scroll_up_region(&mut self, n: usize) {
        let BufferState {
            normal,
            cursor,
            pen_state,
            ..
        } = &mut self.active;
        CellOps::scroll_up_region(pen_state, normal, cursor, n);
    }

    pub fn scroll_down_region(&mut self, n: usize) {
        let BufferState {
            normal,
            cursor,
            pen_state,
            ..
        } = &mut self.active;
        CellOps::scroll_down_region(pen_state, normal, cursor, n);
    }

    // ── DEC rectangle ops ──────────────────────────────────────────────

    pub fn decera(&mut self, params: &Params) {
        let BufferState {
            normal,
            cursor,
            pen_state,
            ..
        } = &mut self.active;
        CellOps::decera(pen_state, normal, cursor, params);
    }

    pub fn decsera(&mut self, params: &Params) {
        let BufferState {
            normal,
            cursor,
            pen_state,
            ..
        } = &mut self.active;
        CellOps::decsera(pen_state, normal, cursor, params);
    }

    pub fn decfra(&mut self, params: &Params) {
        let BufferState {
            normal,
            cursor,
            pen_state,
            ..
        } = &mut self.active;
        CellOps::decfra(pen_state, normal, cursor, params);
    }

    pub fn deccra(&mut self, params: &Params) {
        let BufferState {
            normal,
            cursor,
            pen_state,
            ..
        } = &mut self.active;
        CellOps::deccra(pen_state, normal, cursor, params);
    }

    pub fn deccara(&mut self, params: &Params) {
        let BufferState { normal, cursor, .. } = &mut self.active;
        CellOps::deccara(normal, cursor, params);
    }

    pub fn decrara(&mut self, params: &Params) {
        let BufferState { normal, cursor, .. } = &mut self.active;
        CellOps::decrara(normal, cursor, params);
    }

    // ── tab stops ──────────────────────────────────────────────────────

    pub fn set_tab_stop(&mut self) {
        self.active
            .pen_state
            .set_tab_stop(self.active.cursor.cursor.x);
    }

    pub fn clear_tab_stops(&mut self, mode: usize) {
        self.active
            .pen_state
            .clear_tab_stops(self.active.cursor.cursor.x, mode);
    }

    // ── cursor save / restore ──────────────────────────────────────────

    pub fn save_cursor(&mut self) {
        let position = self.live_cursor_position();
        let pending_wrap = self.active.cursor.modes.pending_wrap;
        let anchor = self
            .content_projection()
            .ok()
            .and_then(|projection| projection.cursor_anchor(position, pending_wrap));
        self.active.cursor.save_cursor_position(anchor);
        self.active.pen_state.save_pen();
    }

    pub fn restore_cursor(&mut self) {
        self.active.cursor.restore_cursor_position();
        self.active.pen_state.restore_pen();
    }

    // ── write_char (coordinator) ───────────────────────────────────────

    pub fn write_char(&mut self, ch: char) {
        let mut width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if CellWriter::continuation_base(&self.active.normal, &self.active.cursor, ch).is_some() {
            width = 0; // An existing logical atom, even if promotion moves its projection.
        } else if (CellWriter::is_combining_mark(ch)
            || CellWriter::is_selector(ch)
            || ch == '\u{200d}')
            && CellWriter::combining_base(&self.active.normal, &self.active.cursor).is_none()
        {
            width = 1; // An isolated zero-width scalar writes a new atom.
        }
        // Width promotion consumes the neighboring physical cell. Capture its
        // content identity before the writer shifts or overwrites it.
        let promotion = CellWriter::promotion_base(&self.active.normal, &self.active.cursor, ch)
            .filter(|&(_, col)| {
                let right = if self.active.cursor.margins.enabled {
                    self.active.cursor.margins.right
                } else {
                    self.active.normal.cols() - 1
                };
                col < right
            });
        let promoted_before = promotion.and_then(|_| {
            let dropped = self.active.cursor.modes.insert.then(|| {
                let right = if self.active.cursor.margins.enabled {
                    self.active.cursor.margins.right
                } else {
                    self.active.normal.cols() - 1
                };
                (right, right + 1)
            });
            self.capture_edit_anchor(1, dropped)
        });
        let cursor_before = (
            self.active.cursor.cursor.x,
            self.active.cursor.cursor.y,
            self.active.cursor.modes.pending_wrap,
        );
        let before_generation = self.live_cursor_position().generation;
        let right_before_write = if self.active.cursor.margins.enabled {
            self.active.cursor.margins.right
        } else {
            self.active.normal.cols().saturating_sub(1)
        };
        let wide_wraps = width == 2
            && self.active.cursor.modes.autowrap
            && self.active.cursor.cursor.x == right_before_write;
        let before = if self.active.cursor.modes.insert
            && !self.active.cursor.modes.pending_wrap
            && width > 0
            && !wide_wraps
        {
            self.capture_insertion_anchor(width).1
        } else {
            None
        };
        let BufferState {
            normal,
            cursor,
            pen_state,
            ..
        } = &mut self.active;
        if !CellWriter::write_char(pen_state, normal, cursor, ch) {
            return;
        }
        normal.repair_following_soft_chain(cursor.cursor.y);
        let cursor_after = (
            self.active.cursor.cursor.x,
            self.active.cursor.cursor.y,
            self.active.cursor.modes.pending_wrap,
        );
        let inserted_atoms = usize::from(
            width > 0
                && cursor_after != cursor_before
                && self.live_cursor_position().generation == before_generation,
        );
        if inserted_atoms > 0 {
            self.record_insertion_delta(before, inserted_atoms, true);
        }
        if let Some((row, col)) = promotion
            && self.active.normal.cell(row, col).grid_width() == 2
        {
            if self.active.cursor.modes.insert {
                if let Some(capture) = promoted_before {
                    if let Some((start, count)) = capture.dropped_atoms {
                        self.active.anchor_mutations.push(AnchorMutation::Delete {
                            line_id: capture.anchor.line_id,
                            start: LogicalAtomOffset(start),
                            end: LogicalAtomOffset(start + count),
                        });
                    }
                    self.active
                        .anchor_mutations
                        .push(AnchorMutation::ReprojectLine(capture.anchor.line_id));
                }
            } else {
                self.record_deletion_delta(promoted_before);
            }
        }
    }

    // ── horizontal_tab (coordinator) ───────────────────────────────────

    pub fn horizontal_tab(&mut self) {
        self.active.cursor.clear_pending_wrap();
        let right_limit = if self.active.cursor.margins.enabled {
            self.active.cursor.margins.right
        } else {
            self.active.normal.cols()
        };
        let mut target = right_limit;
        for col in (self.active.cursor.cursor.x + 1)..=right_limit {
            if col < self.active.pen_state.tab_stops.0.len()
                && self.active.pen_state.tab_stops.0[col]
            {
                target = col;
                break;
            }
        }
        if target > self.active.cursor.cursor.x {
            let spaces = target - self.active.cursor.cursor.x;
            let BufferState {
                normal,
                cursor,
                pen_state,
                ..
            } = &mut self.active;
            for _ in 0..spaces {
                CellWriter::write_internal_char(pen_state, normal, cursor, ' ');
            }
        }
        self.active.cursor.clear_pending_wrap();
    }

    /// Moves forward over `steps` tab stops without changing any cells.
    pub fn forward_tab(&mut self, steps: usize) {
        self.active.cursor.clear_pending_wrap();
        let steps = steps.max(1);
        let (left_limit, right_limit) = if self.active.cursor.margins.enabled {
            (
                self.active.cursor.margins.left,
                self.active.cursor.margins.right,
            )
        } else {
            (0, self.active.normal.cols().saturating_sub(1))
        };
        let start = self
            .active
            .cursor
            .cursor
            .x
            .saturating_add(1)
            .max(left_limit);

        if start <= right_limit {
            let mut remaining = steps;
            for col in start..=right_limit {
                if self.active.pen_state.tab_stops.0[col] {
                    remaining -= 1;
                    if remaining == 0 {
                        self.active.cursor.cursor.x = col;
                        return;
                    }
                }
            }
        }
        self.active.cursor.cursor.x = right_limit;
    }

    /// Moves backward over `steps` tab stops without changing any cells.
    pub fn backward_tab(&mut self, steps: usize) {
        self.active.cursor.clear_pending_wrap();
        let steps = steps.max(1);
        let (left_limit, right_limit) = if self.active.cursor.margins.enabled {
            (
                self.active.cursor.margins.left,
                self.active.cursor.margins.right,
            )
        } else {
            (0, self.active.normal.cols().saturating_sub(1))
        };
        let end = self
            .active
            .cursor
            .cursor
            .x
            .saturating_sub(1)
            .min(right_limit);

        if end >= left_limit {
            let mut remaining = steps;
            for col in (left_limit..=end).rev() {
                if self.active.pen_state.tab_stops.0[col] {
                    remaining -= 1;
                    if remaining == 0 {
                        self.active.cursor.cursor.x = col;
                        return;
                    }
                }
            }
        }
        self.active.cursor.cursor.x = left_limit;
    }

    // ── repeat_char (coordinator) ──────────────────────────────────────

    pub fn repeat_char(&mut self, n: usize) {
        if let Some(ch) = self.active.pen_state.charsets.last_char {
            let n = if n == 0 { 1 } else { n };
            let count = n.min(self.active.normal.cols());
            let BufferState {
                normal,
                cursor,
                pen_state,
                ..
            } = &mut self.active;
            for _ in 0..count {
                CellWriter::write_internal_char(pen_state, normal, cursor, ch);
            }
        }
    }

    // ── newline / line_feed / index (coordinators) ─────────────────────

    pub fn newline(&mut self) {
        self.active.cursor.carriage_return();
        self.index();
    }

    pub fn line_feed(&mut self) {
        if self.active.cursor.modes.line_feed {
            self.active.cursor.carriage_return();
        }
        self.index();
    }

    pub fn index(&mut self) {
        self.active.cursor.clear_pending_wrap();
        let before = self.active.cursor.cursor.y;
        let scrolled = self.active.cursor.index_needs_scroll();
        if scrolled {
            self.scroll_region_up_one();
        } else {
            self.active.cursor.index_advance(&self.active.normal);
        }
        // Only mark the destination row as a new logical line when the cursor
        // actually moved or a scroll occurred; a no-op index (cursor pinned at
        // the bottom below the scroll region) must not clear an existing flag.
        if scrolled || self.active.cursor.cursor.y != before {
            self.active
                .normal
                .begin_hard_line(self.active.cursor.cursor.y);
        }
    }

    // ── reverse_index (coordinator) ────────────────────────────────────

    pub fn reverse_index(&mut self) {
        self.active.cursor.clear_pending_wrap();
        let before = self.active.cursor.cursor.y;
        let mut transitioned = false;
        tracing::debug!(
            cursor_y = self.active.cursor.cursor.y,
            scroll_top = self.active.cursor.scroll_region.top,
            scroll_bottom = self.active.cursor.scroll_region.bottom,
            full_screen = (self.active.cursor.scroll_region.top == 0
                && self.active.cursor.scroll_region.bottom == self.active.normal.rows() - 1),
            "reverse_index"
        );

        if self.active.cursor.cursor.y == self.active.cursor.scroll_region.top
            && self.active.cursor.cursor.y <= self.active.cursor.scroll_region.bottom
        {
            transitioned = true;
            self.mark_rows_dirty(
                self.active.cursor.scroll_region.top,
                self.active.cursor.scroll_region.bottom.saturating_add(1),
            );
            if self.active.cursor.margins.enabled {
                let BufferState {
                    normal,
                    cursor,
                    pen_state,
                    ..
                } = &mut self.active;
                CellOps::scroll_margin_rect_down(
                    pen_state,
                    normal,
                    cursor,
                    cursor.scroll_region.top,
                    cursor.scroll_region.bottom,
                    1,
                );
            } else {
                self.active
                    .normal
                    .sever_soft_wrap_after(self.active.cursor.scroll_region.bottom);
                let tr = self.active.normal.total_rows();
                let vis = self.active.normal.visible_start();
                let src_start = (vis + self.active.cursor.scroll_region.top) % tr;
                let src_end = (vis + self.active.cursor.scroll_region.bottom) % tr;
                let dst = (vis + self.active.cursor.scroll_region.top + 1) % tr;
                self.active.normal.copy_ring_rows(src_start, src_end, dst);
                self.active
                    .normal
                    .sever_soft_wrap(self.active.cursor.scroll_region.top + 1);
                self.active.normal.fill_row_with(
                    self.active.cursor.scroll_region.top,
                    self.active.pen_state.erase_cell(),
                );
            }
        } else if self.active.cursor.cursor.y > 0 {
            self.active.cursor.cursor.y -= 1;
        }
        if transitioned || self.active.cursor.cursor.y != before {
            self.active
                .normal
                .begin_hard_line(self.active.cursor.cursor.y);
        }
    }
    pub(crate) fn requires_anchor_baseline(&self) -> bool {
        self.active.review_anchor.is_some()
            || self
                .active
                .cursor
                .cursor
                .saved
                .as_ref()
                .is_some_and(|saved| saved.anchor.is_some())
    }

    /// Registers or clears an external consumer of content-anchor mutations.
    ///
    /// The I/O owner sets this before parsing a feed: selection state lives with
    /// the pointer, so `Screen` never queries the pointer directly.
    pub(crate) fn set_external_anchor_interest(&mut self, active: bool) {
        self.active.external_anchor_interest = active;
    }

    /// Whether anything currently needs content-anchor bookkeeping.
    ///
    /// Anchor mutations, and the projection that resolves them, exist only for a
    /// selection, a saved cursor anchor and a review anchor. Projecting the whole
    /// retained buffer without one costs a full rebuild per accepted feed and two
    /// per width promotion, so both recording paths check this first.
    pub(crate) fn tracks_content_anchors(&self) -> bool {
        self.active.external_anchor_interest || self.requires_anchor_baseline()
    }

    /// Drops mutations no consumer will read, matching one reconciliation pass
    /// without building a projection for it.
    pub(crate) fn discard_anchor_mutations(&mut self) {
        let _ = std::mem::take(&mut self.active.anchor_mutations);
    }

    // ── scroll_region_up_one (coordinator) ─────────────────────────────

    fn scroll_region_up_one(&mut self) {
        let BufferState {
            pen_state,
            normal,
            cursor,
            ..
        } = &mut self.active;
        CellOps::scroll_region_up_one(pen_state, normal, cursor);
    }

    // ── screen alignment / reset ───────────────────────────────────────

    /// Performs DECALN (`ESC # 8`) on the active visible buffer.
    pub fn decaln(&mut self) {
        let cell = Cell {
            ch: 'E',
            ..Cell::default()
        };
        self.active.normal.fill_all_with(cell);
        self.active.cursor.alignment_home();
        self.mark_all_dirty();
    }

    pub fn reset_display(&mut self) {
        self.synchronized_output.clear();
        self.active
            .anchor_mutations
            .push(AnchorMutation::InvalidateAll);
        self.active.review_anchor = None;
        self.focus_reporting.reset_for_ris();
        self.alt_request = None;
        self.saved_primary = None;
        self.deferred_primary_resize.clear();
        self.parked_alt = None;

        let rows = self.active.normal.rows();
        let cols = self.active.normal.cols();
        if self.active.normal.history_limits() != self.history_limits {
            self.active.normal =
                NormalBuf::with_history_limits(rows, cols, self.history_limits).into();
        } else {
            self.active.normal.reset_all_retained();
        }
        self.active.cursor.reset(rows, cols);
        self.active.pen_state.reset(cols);
        self.active.hyperlinks.clear();
        self.mark_all_dirty();
    }

    pub fn soft_reset(&mut self) {
        let rows = self.active.normal.rows();
        let cols = self.active.normal.cols();
        self.active.cursor.reset(rows, cols);
        self.active.pen_state.soft_reset();
    }

    // ── misc ───────────────────────────────────────────────────────────

    pub fn row_text(&self, row: usize) -> String {
        self.active.normal.row_text(row)
    }

    /// Returns whether the given display row is a soft-wrapped continuation of
    /// the logical line above. `row` is viewport-relative (0..`rows`) and
    /// view-offset aware — it reports the row currently displayed at that
    /// position, consistent with `cell`/`cells`. Consumers working in
    /// scrollback generation coordinates must map to display rows first.
    pub fn is_wrapped(&self, row: usize) -> bool {
        self.active.normal.is_wrapped(row)
    }
}
/// Buffer-local storage, cursor/pen, hyperlinks and anchor evidence only.
#[derive(Debug)]
struct BufferState {
    normal: Revisioned<NormalBuf>,
    anchor_mutations: AnchorMutationBatch,
    review_anchor: Option<ContentAnchor>,
    external_anchor_interest: bool,
    cursor: Revisioned<CursorEngine>,
    pen_state: PenState,
    hyperlinks: HyperlinkRegistry,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BufferTransition {
    Entered { restored_parked: bool },
    Exited,
}

impl BufferState {
    fn new(rows: usize, cols: usize, limits: harbor_config::HistoryLimits) -> Self {
        Self {
            normal: NormalBuf::with_history_limits(rows, cols, limits).into(),
            anchor_mutations: AnchorMutationBatch::default(),
            review_anchor: None,
            external_anchor_interest: false,
            cursor: CursorEngine::new(rows, cols).into(),
            pen_state: PenState::new(cols),
            hyperlinks: HyperlinkRegistry::default(),
        }
    }

    fn rows(&self) -> usize {
        self.normal.rows()
    }
    fn cols(&self) -> usize {
        self.normal.cols()
    }

    fn commit_resize(&mut self, prepared: PreparedBufferResize) {
        self.normal = prepared.normal.into();
        self.cursor = prepared.cursor.into();
        self.pen_state = prepared.pen_state;
        self.hyperlinks = prepared.hyperlinks;
        self.review_anchor = prepared.review_anchor;
        self.anchor_mutations = AnchorMutationBatch::default();
    }

    fn prepare_primary_resize_with_viewport(
        &self,
        requested_rows: usize,
        requested_cols: usize,
        viewport: ReflowViewport,
        limits: harbor_config::HistoryLimits,
        normal_source: &NormalBuf,
    ) -> Result<PreparedPrimaryResize, PreparationError> {
        let cursor_floor = Some(
            self.cursor
                .cursor
                .y
                .max(self.cursor.cursor.saved.as_ref().map_or(0, |s| s.cursor_y)),
        );
        let mut cursor_retentions = vec![CursorRetention::Position(self.live_cursor_position())];
        if let Some(saved) = self.cursor.cursor.saved.as_ref() {
            if let Some(anchor) = saved.anchor {
                if let Some(anchor) = self.anchor_mutations.apply(anchor) {
                    cursor_retentions.push(CursorRetention::Anchor(anchor));
                }
            } else {
                let live_top = self.normal.history_start() + self.normal.scroll_count() as u64;
                cursor_retentions.push(CursorRetention::Position(GenPos::new(
                    live_top.saturating_add(saved.cursor_y as u64),
                    saved.cursor_x,
                )));
            }
        }
        let prepared = PreparedPrimaryResize::prepare_geometry_with_limits(
            normal_source,
            requested_rows,
            requested_cols,
            cursor_floor,
            viewport,
            &cursor_retentions,
            limits,
        )?;
        let live_cursor = prepared
            .source_cursor_anchor(self.live_cursor_position(), self.cursor.modes.pending_wrap)
            .ok_or(PreparationError::UnresolvedLiveCursor)?;
        let target_live_top = prepared
            .normal()
            .history_start()
            .checked_add(
                u64::try_from(prepared.normal().scroll_count())
                    .map_err(|_| PreparationError::ArithmeticOverflow)?,
            )
            .ok_or(PreparationError::ArithmeticOverflow)?;
        let target_live_end = target_live_top
            .checked_add(
                u64::try_from(requested_rows.max(1))
                    .map_err(|_| PreparationError::ArithmeticOverflow)?,
            )
            .ok_or(PreparationError::ArithmeticOverflow)?;
        let live_is_retained = prepared
            .project_cursor(live_cursor)
            .is_some_and(|projection| {
                projection.position.generation >= target_live_top
                    && projection.position.generation < target_live_end
            });
        let live_cursor = if !live_is_retained
            && self
                .normal
                .live_row_metadata(self.cursor.cursor.y)
                .meaningful_extent
                == 0
        {
            let source_live_top = self
                .normal
                .history_start()
                .checked_add(
                    u64::try_from(self.normal.scroll_count())
                        .map_err(|_| PreparationError::ArithmeticOverflow)?,
                )
                .ok_or(PreparationError::ArithmeticOverflow)?;
            let active_visible = self
                .normal
                .active_retained_row_count(cursor_floor)
                .saturating_sub(self.normal.scroll_count());
            let fallback_row = active_visible.min(self.normal.rows()).saturating_sub(1);
            let fallback_generation = source_live_top
                .checked_add(
                    u64::try_from(fallback_row)
                        .map_err(|_| PreparationError::ArithmeticOverflow)?,
                )
                .ok_or(PreparationError::ArithmeticOverflow)?;
            prepared
                .source_cursor_anchor(
                    GenPos::new(
                        fallback_generation,
                        self.cursor
                            .cursor
                            .x
                            .min(self.normal.cols().saturating_sub(1)),
                    ),
                    false,
                )
                .ok_or(PreparationError::UnresolvedLiveCursor)?
        } else {
            live_cursor
        };
        let live_top = self.normal.history_start() + self.normal.scroll_count() as u64;
        let saved_cursor = match self.cursor.cursor.saved.as_ref() {
            None => PreparedProjection::Absent,
            Some(saved) => {
                let anchor = if let Some(anchor) = saved.anchor {
                    self.anchor_mutations.apply(anchor)
                } else {
                    prepared.source_cursor_anchor(
                        GenPos::new(
                            live_top.saturating_add(saved.cursor_y as u64),
                            saved.cursor_x,
                        ),
                        saved.pending_wrap,
                    )
                };
                anchor.map_or(PreparedProjection::Invalid, PreparedProjection::Projected)
            }
        };
        let review = if self.normal.view_offset() == 0 {
            PreparedProjection::Absent
        } else if let Some(anchor) = self.review_anchor {
            self.anchor_mutations
                .apply(anchor)
                .map_or(PreparedProjection::Invalid, PreparedProjection::Projected)
        } else {
            let generation = self.normal.history_start()
                + self
                    .normal
                    .scroll_count()
                    .saturating_sub(self.normal.view_offset()) as u64;
            prepared
                .source_anchor(GenPos::new(generation, 0), Affinity::Before)
                .map_or(PreparedProjection::Invalid, PreparedProjection::Projected)
        };

        prepared.attach_screen_anchors(live_cursor, saved_cursor, review)
    }

    fn live_cursor_position(&self) -> GenPos {
        GenPos::new(
            self.normal.history_start()
                + self.normal.scroll_count() as u64
                + self.cursor.cursor.y as u64,
            self.cursor.cursor.x,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_resize_for_role(
        &self,
        rows: usize,
        cols: usize,
        role: BufferRole,
        viewport: ReflowViewport,
        limits: harbor_config::HistoryLimits,
        normal_source: Option<&NormalBuf>,
    ) -> Result<PreparedBufferResize, PreparationError> {
        let RoleBuffers {
            mut normal,
            projection,
            cursor,
            review_anchor,
            rectangular_selection,
        } = self.prepare_role_buffers(role, rows, cols, viewport, limits, normal_source)?;
        normal.mark_all_dirty();
        let pen_state = self.pen_state.prepare_resize(cols)?;
        let mut reachable = HashSet::new();
        let reachable_cells = normal
            .retained_rows()
            .flat_map(|row| row.cells.iter())
            .filter(|cell| cell.hyperlink.is_some())
            .count();
        reachable
            .try_reserve(
                reachable_cells
                    .checked_add(2)
                    .ok_or(PreparationError::ArithmeticOverflow)?,
            )
            .map_err(|_| PreparationError::AllocationFailed)?;
        reachable.extend(
            normal
                .retained_rows()
                .flat_map(|row| row.cells.iter())
                .filter_map(|cell| cell.hyperlink),
        );
        reachable.extend(pen_state.hyperlink_ids());
        let hyperlinks = self.hyperlinks.prepare_retained(&reachable)?;

        Ok(PreparedBufferResize {
            normal,
            review_anchor,
            cursor,
            pen_state,
            hyperlinks,
            projection,
            rectangular_selection,
        })
    }

    /// Prepares only the role-owned buffer contents; callers share session-level state.
    fn prepare_role_buffers(
        &self,
        role: BufferRole,
        rows: usize,
        cols: usize,
        viewport: ReflowViewport,
        limits: harbor_config::HistoryLimits,
        normal_source: Option<&NormalBuf>,
    ) -> Result<RoleBuffers, PreparationError> {
        match role {
            BufferRole::Primary => {
                self.prepare_primary_buffer(rows, cols, viewport, limits, normal_source)
            }
            BufferRole::Alternate => self.prepare_alternate_buffer(rows, cols),
        }
    }

    /// Primary resize, or a geometry-preserving retrim of the history budget.
    fn prepare_primary_buffer(
        &self,
        rows: usize,
        cols: usize,
        viewport: ReflowViewport,
        limits: harbor_config::HistoryLimits,
        normal_source: Option<&NormalBuf>,
    ) -> Result<RoleBuffers, PreparationError> {
        if rows == self.rows() && cols == self.cols() && limits != self.normal.history_limits() {
            let normal = normal_source
                .unwrap_or(&self.normal)
                .prepare_history_limits(limits)?;
            let projection = ContentProjection::build(&normal)?;
            let review_anchor = if normal.view_offset() == 0 {
                None
            } else {
                self.review_anchor
                    .filter(|anchor| projection.resolve_selection(*anchor).is_some())
                    .or_else(|| projection.oldest_anchor())
            };
            return Ok(RoleBuffers {
                normal,
                projection,
                cursor: (*self.cursor).clone(),
                review_anchor,
                rectangular_selection: None,
            });
        }

        let prepared = self.prepare_primary_resize_with_viewport(
            rows,
            cols,
            viewport,
            limits,
            normal_source.unwrap_or(&self.normal),
        )?;
        let (normal, live_cursor, saved_cursor, review) = prepared.into_screen_parts();
        let projection = ContentProjection::build(&normal)?;
        let live_top = normal
            .history_start()
            .checked_add(
                u64::try_from(normal.scroll_count())
                    .map_err(|_| PreparationError::ArithmeticOverflow)?,
            )
            .ok_or(PreparationError::ArithmeticOverflow)?;
        let mut cursor = (*self.cursor).clone();
        cursor.clamp_to_grid(rows, cols);
        let PreparedProjection::Projected(live) = live_cursor else {
            return Err(PreparationError::UnresolvedLiveCursor);
        };
        cursor.cursor.y = usize::try_from(live.position.generation - live_top)
            .map_err(|_| PreparationError::ArithmeticOverflow)?;
        cursor.cursor.x = live.position.col;
        cursor.modes.pending_wrap = live.pending_wrap;
        match saved_cursor {
            PreparedProjection::Projected(projected) => {
                if let Some(saved) = cursor.cursor.saved.as_mut() {
                    saved.cursor_y = usize::try_from(projected.position.generation - live_top)
                        .map_err(|_| PreparationError::ArithmeticOverflow)?;
                    saved.cursor_x = projected.position.col;
                    saved.pending_wrap = projected.pending_wrap;
                    saved.anchor =
                        projection.cursor_anchor(projected.position, projected.pending_wrap);
                }
            }
            PreparedProjection::Absent | PreparedProjection::Invalid => {
                cursor.cursor.saved = None;
            }
        }
        let review_anchor = match review {
            PreparedProjection::Projected(position) => {
                projection.to_anchor(position, Affinity::Before)
            }
            PreparedProjection::Absent | PreparedProjection::Invalid => None,
        };
        Ok(RoleBuffers {
            normal,
            projection,
            cursor,
            review_anchor,
            rectangular_selection: None,
        })
    }

    /// Alternate resize is rectangular: retained cells keep their position and content.
    fn prepare_alternate_buffer(
        &self,
        rows: usize,
        cols: usize,
    ) -> Result<RoleBuffers, PreparationError> {
        let normal = self.normal.prepare_rectangular_resize(rows, cols)?;
        let projection = ContentProjection::build(&normal)?;
        let source_projection = ContentProjection::build(&self.normal)?;
        let source_live_top = self
            .normal
            .history_start()
            .checked_add(
                u64::try_from(self.normal.scroll_count())
                    .map_err(|_| PreparationError::ArithmeticOverflow)?,
            )
            .ok_or(PreparationError::ArithmeticOverflow)?;
        let target_live_top = normal.history_start();
        let copied_rows = self.normal.rows().min(rows);
        let copied_cols = self.normal.cols().min(cols);
        let retained_count = copied_rows
            .checked_mul(copied_cols)
            .ok_or(PreparationError::ArithmeticOverflow)?;
        let mut retained_cells = Vec::new();
        retained_cells
            .try_reserve_exact(retained_count)
            .map_err(|_| PreparationError::AllocationFailed)?;
        for row in 0..copied_rows {
            let source_generation = source_live_top
                .checked_add(u64::try_from(row).map_err(|_| PreparationError::ArithmeticOverflow)?)
                .ok_or(PreparationError::ArithmeticOverflow)?;
            let target_generation = target_live_top
                .checked_add(u64::try_from(row).map_err(|_| PreparationError::ArithmeticOverflow)?)
                .ok_or(PreparationError::ArithmeticOverflow)?;
            for col in 0..copied_cols {
                retained_cells.push(
                    self.normal.cell_at_generation(source_generation, col)
                        == normal.cell_at_generation(target_generation, col),
                );
            }
        }
        let rectangular_selection = Some(RectangularSelectionProjection {
            source: source_projection,
            source_live_top,
            target_live_top,
            rows: copied_rows,
            cols: copied_cols,
            retained_cells,
        });
        let mut cursor = (*self.cursor).clone();
        cursor.clamp_to_grid(rows, cols);
        let live_top = normal.history_start();
        if let Some(saved) = cursor.cursor.saved.as_mut() {
            let saved_position = GenPos::new(
                live_top
                    .checked_add(
                        u64::try_from(saved.cursor_y)
                            .map_err(|_| PreparationError::ArithmeticOverflow)?,
                    )
                    .ok_or(PreparationError::ArithmeticOverflow)?,
                saved.cursor_x,
            );
            saved.anchor = projection.cursor_anchor(saved_position, saved.pending_wrap);
        }
        Ok(RoleBuffers {
            normal,
            projection,
            cursor,
            review_anchor: None,
            rectangular_selection,
        })
    }
}
