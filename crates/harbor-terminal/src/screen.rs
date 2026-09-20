//! Terminal screen: thin coordinator delegating to internal engines.
//!
//! - [`cursor::CursorEngine`] — cursor position, scroll region, margins, modes
//! - [`edit::PenState`]       — pen (SGR), tab stops, charsets, erase-cell helper
//! - [`edit::CellOps`]        — cell-level mutations (erase, insert, delete, scroll, DEC rects)
//! - [`edit::CellWriter`]     — character writing (write_char and helpers)
//! - [`synchronized_output::SynchronizedOutput`] — saturating `?2026` nesting
//!
//! `Screen` keeps the public API stable; most methods are one-line
//! delegations.  Cross-engine operations (e.g. `reverse_index`,
//! `scroll_region_up_one`) stay here.

mod cursor;
mod default_colors;
mod edit;
mod focus_reporting;
mod hyperlink;
mod reader;
mod synchronized_output;
#[cfg(test)]
mod tests;

use self::default_colors::DefaultColors;
use crate::content_anchor::{
    Affinity, AnchorMutation, AnchorMutationBatch, ContentAnchor, ContentProjection,
};
use crate::logical_content::{DecodeError, LogicalAtomOffset};
use crate::normal_buf::{CellsIter, LogicalLineId};
use crate::primary_reflow::{PreparationError, PreparedPrimaryWidthReflow, PreparedProjection};
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
pub use self::reader::ScreenReader;

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
// The permanent DECRPM statuses are reserved for future fixed-mode support.
#[allow(dead_code)]
pub(crate) enum ModeStatus {
    Unknown,
    Set,
    Reset,
    PermanentlySet,
    PermanentlyReset,
}

impl ModeStatus {
    pub(crate) const fn code(self) -> usize {
        match self {
            Self::Unknown => 0,
            Self::Set => 1,
            Self::Reset => 2,
            Self::PermanentlySet => 3,
            Self::PermanentlyReset => 4,
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
/// `Screen` owns only display state: cell contents, dimensions, and cursor position. It does not
/// parse byte streams; `TerminalParser` calls these methods after recognizing control sequences.
#[derive(Debug)]
pub struct Screen {
    /// Ring-buffer scrollback storage.
    normal: NormalBuf,
    /// Ordered semantic edits awaiting terminal-owned selection reconciliation.
    anchor_mutations: AnchorMutationBatch,
    /// Canonical top-left retained content while reviewing scrollback.
    review_anchor: Option<ContentAnchor>,
    /// Cursor position, scroll region, margins, and terminal modes.
    cursor: CursorEngine,
    /// Pen state, tab stops, character-set designations, and saved-pen snapshot.
    pen_state: PenState,
    /// OSC 8 values owned by this screen and referenced from cells/pen state.
    hyperlinks: HyperlinkRegistry,
    /// Pending alt-screen request set by the parser, consumed by I/O.
    alt_request: Option<AltScreenAction>,
    /// Primary screen saved while the alternate screen is active.
    /// Invariant: `Some` iff in alt screen (`is_alt()` is true).
    saved_primary: Option<Box<Screen>>,
    /// Alternate screen parked across `?47` exit/re-enter.
    /// Invariant: parked buffers keep `saved_primary = None`.
    parked_alt: Option<Box<Screen>>,
    /// Outgoing VT replies buffer.
    pub(crate) replies: Vec<u8>,
    /// Session-owned `?1004` mode and latest observed native-window focus.
    focus_reporting: FocusReporting,
    /// Session-owned `?2026` nesting; preserved across alt-screen swap.
    synchronized_output: SynchronizedOutput,
    /// Startup and active OSC default colors, owned by the terminal session.
    default_colors: DefaultColors,
}
#[derive(Debug)]
struct EditAnchorCapture {
    anchor: ContentAnchor,
    cells: Vec<Cell>,
    affected_atoms: usize,
    shifted_atoms: usize,
    dropped_atoms: Option<(usize, usize)>,
}

impl Screen {
    pub fn new(rows: usize, cols: usize) -> Self {
        Self::with_palette(rows, cols, Palette::default())
    }

    pub(crate) fn with_palette(rows: usize, cols: usize, palette: Palette) -> Self {
        let rows = rows.max(1);
        let cols = cols.max(1);
        Self {
            normal: NormalBuf::new(rows, cols),
            anchor_mutations: AnchorMutationBatch::default(),
            review_anchor: None,
            cursor: CursorEngine::new(rows, cols),
            pen_state: PenState::new(cols),
            hyperlinks: HyperlinkRegistry::default(),
            alt_request: None,
            saved_primary: None,
            parked_alt: None,
            replies: Vec::new(),
            focus_reporting: FocusReporting::default(),
            synchronized_output: SynchronizedOutput::default(),
            default_colors: DefaultColors::new(palette),
        }
    }

    pub(crate) fn active_palette(&self) -> Palette {
        self.default_colors.active_palette()
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

    // ── dimensions / viewport ──────────────────────────────────────────

    pub fn rows(&self) -> usize {
        self.normal.rows()
    }

    pub fn cols(&self) -> usize {
        self.normal.cols()
    }

    pub fn scroll_count(&self) -> usize {
        self.normal.scroll_count()
    }

    pub fn view_offset(&self) -> usize {
        self.normal.view_offset()
    }

    pub fn visible_rows(&self) -> usize {
        self.normal.rows()
    }

    pub fn history_start(&self) -> u64 {
        self.normal.history_start()
    }

    // ── cursor queries ─────────────────────────────────────────────────

    pub fn cursor_x(&self) -> usize {
        self.cursor.cursor_x()
    }

    pub fn cursor_y(&self) -> usize {
        self.cursor.cursor_y(&self.normal)
    }

    #[cfg(test)]
    pub(crate) fn pending_wrap(&self) -> bool {
        self.cursor.modes.pending_wrap
    }

    pub fn cursor_shape(&self) -> CursorShape {
        self.cursor.cursor_shape()
    }

    pub fn cursor_blink(&self) -> bool {
        self.cursor.cursor_blink()
    }

    pub fn cursor_visible(&self) -> bool {
        self.cursor.cursor_visible()
    }

    /// Current SGR foreground, background, and attributes as observed for DECRQSS.
    pub(crate) fn current_sgr(&self) -> (Color, Color, CellAttrs) {
        (
            self.pen_state.pen.fg,
            self.pen_state.pen.bg,
            self.pen_state.pen.attrs,
        )
    }

    /// 1-based inclusive DECSTBM top/bottom bounds.
    pub(crate) fn scroll_region(&self) -> (usize, usize) {
        (
            self.cursor.scroll_region.top + 1,
            self.cursor.scroll_region.bottom + 1,
        )
    }

    /// 1-based inclusive saved DECSLRM left/right bounds.
    pub(crate) fn left_right_margins(&self) -> (usize, usize) {
        (self.cursor.margins.left + 1, self.cursor.margins.right + 1)
    }

    /// Canonical DECSCUSR style derived from current shape and blink.
    pub(crate) fn cursor_style(&self) -> CursorStyleArg {
        match (self.cursor.cursor.shape, self.cursor.cursor.blink) {
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
        if self.pen_state.pen.protected {
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
        let row = if self.cursor.modes.origin {
            self.cursor
                .cursor_y(&self.normal)
                .saturating_sub(self.cursor.scroll_region.top)
                + 1
        } else {
            self.cursor.cursor_y(&self.normal) + 1
        };
        let col = if self.cursor.modes.origin && self.cursor.margins.enabled {
            self.cursor
                .cursor_x()
                .saturating_sub(self.cursor.margins.left)
                + 1
        } else {
            self.cursor.cursor_x() + 1
        };
        (row, col)
    }

    pub fn set_cursor_style(&mut self, arg: CursorStyleArg) {
        self.cursor.set_cursor_style(arg);
    }

    pub fn input_modes(&self) -> InputModes {
        self.cursor.input_modes()
    }

    pub fn margin_mode(&self) -> bool {
        self.cursor.margin_mode()
    }

    // ── cell access ────────────────────────────────────────────────────

    pub fn cells(&self) -> CellsIter<'_> {
        self.normal.cells()
    }

    pub fn cell_char(&self, row: usize, col: usize) -> char {
        self.normal.cell(row, col).ch
    }

    pub fn cell(&self, row: usize, col: usize) -> &Cell {
        self.normal.cell(row, col)
    }

    pub fn cell_at_generation(&self, generation: u64, col: usize) -> Option<&Cell> {
        self.normal.cell_at_generation(generation, col)
    }

    pub(crate) fn hyperlink_at_generation(
        &self,
        generation: u64,
        col: usize,
    ) -> Option<(crate::model::HyperlinkId, &str)> {
        let id = self.normal.cell_at_generation(generation, col)?.hyperlink?;
        Some((id, self.hyperlinks.get(id)?.uri.as_str()))
    }

    pub(crate) fn open_hyperlink(&mut self, uri: String, external_id: Option<String>) {
        let Screen {
            normal,
            pen_state,
            hyperlinks,
            ..
        } = self;
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
        self.pen_state.active_hyperlink = None;
    }

    pub fn is_wrapped_at_generation(&self, generation: u64) -> bool {
        self.normal
            .is_wrapped_at_generation(generation)
            .unwrap_or(false)
    }

    /// Direct cell mutation for test setup.
    #[cfg(test)]
    pub fn cell_mut(&mut self, row: usize, col: usize) -> &mut Cell {
        self.normal.cell_mut(row, col)
    }

    pub(crate) fn content_projection(&self) -> Result<ContentProjection, DecodeError> {
        ContentProjection::build(&self.normal)
    }

    #[allow(dead_code)]
    pub(crate) fn prepare_primary_width_reflow(
        &self,
        requested_cols: usize,
    ) -> Result<PreparedPrimaryWidthReflow, PreparationError> {
        let prepared = self.normal.prepare_primary_width_reflow(requested_cols)?;
        let live_cursor = prepared
            .source_cursor_anchor(self.live_cursor_position(), self.cursor.modes.pending_wrap)
            .ok_or(PreparationError::UnresolvedLiveCursor)?;
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

    fn sync_current_cursor_anchor(&mut self) {
        let position = self.live_cursor_position();
        let pending_wrap = self.cursor.modes.pending_wrap;
        self.cursor.cursor.anchor = self
            .content_projection()
            .ok()
            .and_then(|projection| projection.cursor_anchor(position, pending_wrap));
    }

    fn capture_edit_anchor(
        &self,
        affected_cells: usize,
        dropped_cells: Option<(usize, usize)>,
    ) -> Option<EditAnchorCapture> {
        let projection = self.content_projection().ok()?;
        let position = self.live_cursor_position();
        let anchor = projection.to_anchor(position, Affinity::Before)?;
        let cells = projection.line_cells(anchor.line_id)?;
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
            cells,
            affected_atoms,
            shifted_atoms,
            dropped_atoms,
        })
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
            self.anchor_mutations.push(AnchorMutation::Insert {
                line_id: anchor.line_id,
                at: anchor.offset,
                count: inserted_atoms,
            });
        }
        if let Some((dropped_start, dropped_count)) = dropped_atoms {
            self.anchor_mutations.push(AnchorMutation::Delete {
                line_id: anchor.line_id,
                start: LogicalAtomOffset(dropped_start + inserted_atoms),
                end: LogicalAtomOffset(dropped_start + inserted_atoms + dropped_count),
            });
        }
        self.anchor_mutations
            .push(AnchorMutation::ReprojectLine(anchor.line_id));
    }

    fn record_deletion_delta(&mut self, before: Option<EditAnchorCapture>) {
        let Some(EditAnchorCapture {
            anchor,
            cells: before_cells,
            affected_atoms,
            ..
        }) = before
        else {
            return;
        };
        let Ok(after) = self.content_projection() else {
            self.anchor_mutations
                .push(AnchorMutation::InvalidateLine(anchor.line_id));
            return;
        };
        let Some(after_cells) = after.line_cells(anchor.line_id) else {
            self.anchor_mutations
                .push(AnchorMutation::InvalidateLine(anchor.line_id));
            return;
        };
        let removed = affected_atoms.min(before_cells.len().saturating_sub(anchor.offset.0));
        if removed > 0 {
            self.anchor_mutations.push(AnchorMutation::Delete {
                line_id: anchor.line_id,
                start: anchor.offset,
                end: LogicalAtomOffset(anchor.offset.0 + removed),
            });
        }
        let retained_after_delete = before_cells.len().saturating_sub(removed);
        let inserted_tail = after_cells.len().saturating_sub(retained_after_delete);
        if inserted_tail > 0 {
            self.anchor_mutations.push(AnchorMutation::Insert {
                line_id: anchor.line_id,
                at: LogicalAtomOffset(retained_after_delete),
                count: inserted_tail,
            });
        }
        self.anchor_mutations
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
                self.anchor_mutations
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
        self.anchor_mutations.push(AnchorMutation::Replace {
            line_id,
            start: LogicalAtomOffset(prefix),
            old_count,
            new_count,
        });
    }

    fn sync_review_anchor(&mut self) {
        if self.normal.view_offset() == 0 {
            self.review_anchor = None;
            return;
        }
        let generation = self.normal.history_start()
            + self
                .normal
                .scroll_count()
                .saturating_sub(self.normal.view_offset()) as u64;
        self.review_anchor = self.content_projection().ok().and_then(|projection| {
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
                    self.anchor_mutations
                        .push(AnchorMutation::InvalidateLine(line_id));
                } else if let Some(first_retained_generation) = projection.first_generation(line_id)
                    && before.first_generation(line_id) != Some(first_retained_generation)
                    && projection.history_start() > before.history_start()
                    && let Some(removed) =
                        before.atom_count_before_generation(line_id, first_retained_generation)
                    && removed > 0
                {
                    self.anchor_mutations.push(AnchorMutation::EvictPrefix {
                        line_id,
                        end: LogicalAtomOffset(removed),
                    });
                } else if !self.anchor_mutations.transforms_line(line_id)
                    && let (Some(old_spans), Some(new_spans)) =
                        (before.atom_spans(line_id), projection.atom_spans(line_id))
                {
                    self.record_structural_atom_delta(line_id, &old_spans, &new_spans);
                }
            }
        }

        let mutations = std::mem::take(&mut self.anchor_mutations);
        let globally_invalidated = mutations
            .iter()
            .any(|mutation| matches!(mutation, AnchorMutation::InvalidateAll));
        let mut invalidate_saved = false;
        if !globally_invalidated
            && let Some(saved) = self.cursor.cursor.saved.as_mut()
            && let Some(anchor) = saved.anchor
        {
            let adjusted = mutations.apply(anchor);
            saved.anchor = adjusted;
            match adjusted {
                None => invalidate_saved = true,
                Some(adjusted) => {
                    let live_top = self.normal.history_start() + self.normal.scroll_count() as u64;
                    let old_generation = live_top.saturating_add(saved.cursor_y as u64);
                    let needs_projection = adjusted != anchor
                        || mutations.requires_reprojection(adjusted.line_id)
                        || !projection.contains_generation(adjusted.line_id, old_generation);
                    if needs_projection {
                        if let Some(projected) = projection.resolve_cursor(adjusted)
                            && let Ok(row) =
                                usize::try_from(projected.pos.generation.saturating_sub(live_top))
                            && row < self.normal.rows()
                        {
                            saved.cursor_y = row;
                            if adjusted != anchor
                                || mutations.requires_reprojection(adjusted.line_id)
                            {
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
            self.cursor.cursor.saved = None;
        }

        if let Some(anchor) = self.review_anchor {
            self.review_anchor =
                if globally_invalidated && projection.contains_line(anchor.line_id) {
                    Some(anchor)
                } else {
                    mutations.apply(anchor)
                }
                .or_else(|| projection.oldest_anchor());
            if let Some(offset) = self
                .review_anchor
                .and_then(|value| projection.view_offset_for(value))
            {
                self.normal.set_view_offset(offset);
            }
        }

        let cursor_position = self.live_cursor_position();
        let pending_wrap = self.cursor.modes.pending_wrap;
        self.cursor.cursor.anchor = projection.cursor_anchor(cursor_position, pending_wrap);
        Ok((mutations, projection))
    }

    // ── read-only queries ──────────────────────────────────────────────

    /// Returns a `ScreenReader` for snapshot and text-extraction queries.
    pub fn reader(&self) -> ScreenReader<'_> {
        ScreenReader::new(self)
    }

    pub fn terminal_snapshot(&self) -> crate::model::TerminalSnapshot {
        self.reader().terminal_snapshot()
    }

    pub fn selected_text(&self, bounds: SelectionBounds) -> String {
        self.reader().selected_text(bounds)
    }

    // ── dirty tracking ─────────────────────────────────────────────────

    pub fn dirty_rows(&self) -> Vec<usize> {
        self.normal.dirty_rows()
    }

    pub fn dirty_ranges(&self) -> Vec<DirtyRange> {
        self.normal.dirty_ranges()
    }

    pub fn clear_dirty(&mut self) {
        self.normal.clear_dirty()
    }

    pub fn mark_row_dirty(&mut self, row: usize) {
        self.normal.mark_row_dirty(row);
    }

    pub fn mark_rows_dirty(&mut self, start_row: usize, end_row: usize) {
        self.normal.mark_rows_dirty(start_row, end_row);
    }

    pub fn mark_range_dirty(&mut self, row: usize, start_col: usize, end_col: usize) {
        self.normal.mark_range_dirty(row, start_col, end_col);
    }

    pub fn mark_all_dirty(&mut self) {
        self.normal.mark_all_dirty();
    }

    // ── viewport scroll ────────────────────────────────────────────────

    pub fn scroll_up(&mut self, n: usize) {
        self.normal.scroll_up(n);
        self.sync_review_anchor();
    }

    pub fn scroll_down(&mut self, n: usize) {
        self.normal.scroll_down(n);
        self.sync_review_anchor();
    }

    pub fn scroll_to_bottom(&mut self) {
        self.normal.scroll_to_bottom();
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

    pub fn enter_alt(&mut self, clear: bool) {
        if self.is_alt() {
            return;
        }
        if self.finish_anchor_mutations(None).is_err() {
            self.cursor.cursor.saved = None;
            self.review_anchor = None;
            self.anchor_mutations = AnchorMutationBatch::default();
        }
        let pending_mutations = std::mem::take(&mut self.anchor_mutations);
        let rows = self.rows();
        let cols = self.cols();
        let replies = std::mem::take(&mut self.replies);
        let sync = self.synchronized_output;
        let focus_reporting = self.focus_reporting;
        let default_colors = self.default_colors;
        // Save the primary screen (cells + scrollback + cursor + pen + modes),
        // carrying its parked alternate buffer along to the fresh screen.
        let mut primary = std::mem::replace(self, Self::new(rows, cols));
        let parked = primary.parked_alt.take();
        // Install the alternate screen: clear it, or restore the persistent one.
        if clear {
            // Parked contents (if any) are dropped — `?1047`/`?1049` clear on entry.
        } else if let Some(alt) = parked {
            *self = *alt;
            self.mark_all_dirty();
        }
        // Save the primary after the install block: a restored parked buffer
        // carries `saved_primary = None`, so assigning first would be clobbered.
        self.default_colors = default_colors;
        self.saved_primary = Some(Box::new(primary));
        self.replies = replies;
        self.focus_reporting = focus_reporting;
        self.synchronized_output = sync;
        self.anchor_mutations = pending_mutations;
        self.anchor_mutations.push(AnchorMutation::InvalidateAll);
    }

    pub fn exit_alt(&mut self) {
        if self.saved_primary.is_none() {
            return;
        }
        if self.finish_anchor_mutations(None).is_err() {
            self.cursor.cursor.saved = None;
            self.review_anchor = None;
            self.anchor_mutations = AnchorMutationBatch::default();
        }
        let pending_mutations = std::mem::take(&mut self.anchor_mutations);
        let replies = std::mem::take(&mut self.replies);
        let sync = self.synchronized_output;
        let focus_reporting = self.focus_reporting;
        let default_colors = self.default_colors;
        if let Some(primary) = self.saved_primary.take() {
            // Preserve the alternate-screen contents for a later `?47` re-entry.
            let rows = self.rows();
            let cols = self.cols();
            let mut alt = std::mem::replace(self, Self::new(rows, cols));
            // Drop the nested primary copy so parked alt stays a leaf buffer.
            alt.saved_primary = None;
            alt.default_colors = default_colors;

            *self = *primary;
            self.default_colors = default_colors;
            // Park the alternate buffer after the primary is back, since the
            // saved primary carries an empty `parked_alt` slot.
            self.parked_alt = Some(Box::new(alt));
            self.mark_all_dirty();
        }
        self.replies = replies;
        self.focus_reporting = focus_reporting;
        self.synchronized_output = sync;
        self.anchor_mutations = pending_mutations;
        self.anchor_mutations.push(AnchorMutation::InvalidateAll);
        debug_assert!(!self.is_alt(), "not in alt => no primary saved");
    }

    // ── resize ─────────────────────────────────────────────────────────

    pub fn resize(&mut self, rows: usize, cols: usize) {
        if self.finish_anchor_mutations(None).is_err() {
            self.cursor.cursor.saved = None;
            self.review_anchor = None;
            self.anchor_mutations = AnchorMutationBatch::default();
        }

        let rows = rows.max(1);
        let cols = cols.max(1);
        self.normal.resize(rows, cols);
        self.cursor.clamp_to_grid(rows, cols);
        self.pen_state.tab_stops.resize(cols);

        if let Ok(projection) = self.content_projection() {
            let live_top = self.normal.history_start() + self.normal.scroll_count() as u64;
            let cursor_position = GenPos::new(
                live_top.saturating_add(self.cursor.cursor.y as u64),
                self.cursor.cursor.x,
            );
            self.cursor.cursor.anchor = projection.cursor_anchor(cursor_position, false);
            if let Some(saved) = &mut self.cursor.cursor.saved {
                let saved_position = GenPos::new(
                    live_top.saturating_add(saved.cursor_y as u64),
                    saved.cursor_x,
                );
                saved.anchor = projection.cursor_anchor(saved_position, saved.pending_wrap);
            }
        } else {
            self.cursor.cursor.anchor = None;
            if let Some(saved) = &mut self.cursor.cursor.saved {
                saved.anchor = None;
            }
        }
        self.sync_review_anchor();

        if let Some(saved) = &mut self.saved_primary {
            saved.resize(rows, cols);
        }
        if let Some(alt) = &mut self.parked_alt {
            alt.resize(rows, cols);
        }
    }

    // ── cursor movement (delegated) ────────────────────────────────────

    pub fn cursor_up(&mut self, n: usize) {
        self.cursor.cursor_up(n);
    }

    pub fn cursor_down(&mut self, n: usize) {
        self.cursor.cursor_down(&self.normal, n);
    }

    pub fn cursor_left(&mut self, n: usize) {
        self.cursor.cursor_left(n);
    }

    pub fn cursor_right(&mut self, n: usize) {
        self.cursor.cursor_right(&self.normal, n);
    }

    pub fn carriage_return(&mut self) {
        self.cursor.carriage_return();
    }

    pub fn backspace(&mut self) {
        self.cursor.backspace(&self.normal);
    }

    pub fn set_cursor_position(&mut self, row_1_based: usize, col_1_based: usize) {
        self.cursor
            .set_cursor_position(&self.normal, row_1_based, col_1_based);
    }

    pub fn set_cursor_col(&mut self, col_1_based: usize) {
        self.cursor.set_cursor_col(&self.normal, col_1_based);
    }

    pub fn set_cursor_row(&mut self, row_1_based: usize) {
        self.cursor.set_cursor_row(&self.normal, row_1_based);
    }

    pub fn set_cursor(&mut self, row_1_based: usize, col_1_based: usize) {
        self.cursor
            .set_cursor_position(&self.normal, row_1_based, col_1_based);
    }

    pub fn home_cursor(&mut self) {
        self.cursor.home_cursor();
    }

    // ── scroll region / margins ────────────────────────────────────────

    pub fn set_scroll_region(&mut self, top: usize, bottom: usize) {
        self.cursor.set_scroll_region(&self.normal, top, bottom);
    }

    pub fn set_left_right_margins(&mut self, left: usize, right: usize) {
        self.cursor
            .set_left_right_margins(&self.normal, left, right);
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
            1047 => {
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
            1049 => {
                if enabled {
                    self.request_alt_enter(true);
                } else {
                    self.request_alt_exit();
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
                if !self.cursor.set_private_mode(&self.normal, other, enabled) {
                    tracing::warn!("unsupported private mode: ?{}", other);
                }
            }
        }
    }

    pub fn set_standard_mode(&mut self, param: usize, enabled: bool) {
        if !self.cursor.set_standard_mode(param, enabled) {
            tracing::warn!("unsupported standard mode: {}", param);
        }
    }

    pub(crate) fn mode_status(&self, private: bool, param: usize) -> ModeStatus {
        let enabled = if private {
            match param {
                FocusReporting::MODE => return self.focus_reporting.mode_status(),
                47 | 1047 | 1049 => Some(self.is_alt()),
                1048 => Some(self.cursor.cursor.saved.is_some()),
                SynchronizedOutput::MODE => return self.synchronized_output.mode_status(),
                _ => self.cursor.private_mode_enabled(param),
            }
        } else {
            self.cursor.standard_mode_enabled(param)
        };
        enabled.map(ModeStatus::from).unwrap_or(ModeStatus::Unknown)
    }

    pub fn set_application_keypad(&mut self, enabled: bool) {
        self.cursor.set_application_keypad(enabled);
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
        self.pen_state.set_sgr(params);
    }

    pub fn set_sgr_slice(&mut self, slice: &[Option<usize>]) {
        self.pen_state.set_sgr_slice(slice);
    }

    pub fn designate_g0(&mut self, charset: u8) {
        self.pen_state.designate_g0(charset);
    }

    pub fn designate_g1(&mut self, charset: u8) {
        self.pen_state.designate_g1(charset);
    }

    pub fn designate_g2(&mut self, charset: u8) {
        self.pen_state.designate_g2(charset);
    }

    pub fn designate_g3(&mut self, charset: u8) {
        self.pen_state.designate_g3(charset);
    }

    pub fn single_shift_2(&mut self) {
        self.pen_state.single_shift_2();
    }

    pub fn single_shift_3(&mut self) {
        self.pen_state.single_shift_3();
    }

    pub fn set_active_charset(&mut self, active: u8) {
        self.pen_state.set_active_charset(active);
    }

    pub fn set_character_protection(&mut self, arg: CharacterProtection) {
        self.pen_state.set_character_protection(arg);
    }

    // ── erase ──────────────────────────────────────────────────────────

    pub fn erase_display(&mut self, mode: usize) {
        let Screen {
            normal,
            cursor,
            pen_state,
            ..
        } = self;
        CellOps::erase_display(pen_state, normal, cursor, mode);
    }

    pub fn erase_line(&mut self, mode: usize) {
        let Screen {
            normal,
            cursor,
            pen_state,
            ..
        } = self;
        CellOps::erase_line(pen_state, normal, cursor, mode);
    }

    pub fn erase_chars(&mut self, n: usize) {
        let Screen {
            normal,
            cursor,
            pen_state,
            ..
        } = self;
        CellOps::erase_chars(pen_state, normal, cursor, n);
    }

    pub fn selective_erase_display(&mut self, mode: usize) {
        let Screen {
            normal,
            cursor,
            pen_state,
            ..
        } = self;
        CellOps::selective_erase_display(pen_state, normal, cursor, mode);
    }

    pub fn selective_erase_line(&mut self, mode: usize) {
        let Screen {
            normal,
            cursor,
            pen_state,
            ..
        } = self;
        CellOps::selective_erase_line(pen_state, normal, cursor, mode);
    }

    // ── insert / delete ────────────────────────────────────────────────

    pub fn insert_chars(&mut self, n: usize) {
        let requested = n.max(1);
        let requested_col = self.cursor.cursor.x;
        let (left, right) = if self.cursor.margins.enabled {
            (self.cursor.margins.left, self.cursor.margins.right)
        } else {
            (0, self.normal.cols().saturating_sub(1))
        };
        let base = CellOps::wide_range(&self.normal, self.cursor.cursor.y, requested_col)
            .map_or(requested_col, |(base, _)| base);
        let actual = if requested_col >= left && requested_col <= right {
            requested.min(right - base + 1)
        } else {
            0
        };
        let dropped_cells = (actual > 0).then_some((right + 1 - actual, right.saturating_add(1)));
        let before = self.capture_edit_anchor(actual, dropped_cells);
        let inserted_is_meaningful =
            actual > 0 && NormalBuf::fill_is_meaningful(self.pen_state.erase_cell());
        let Screen {
            normal,
            cursor,
            pen_state,
            ..
        } = self;
        if CellOps::insert_chars(pen_state, normal, cursor, n) {
            self.record_insertion_delta(before, actual, inserted_is_meaningful);
        }
    }

    pub fn delete_chars(&mut self, n: usize) {
        let before = self.capture_edit_anchor(n.max(1), None);
        let Screen {
            normal,
            cursor,
            pen_state,
            ..
        } = self;
        if CellOps::delete_chars(pen_state, normal, cursor, n) {
            self.record_deletion_delta(before);
        }
    }

    pub fn insert_lines(&mut self, n: usize) {
        let Screen {
            normal,
            cursor,
            pen_state,
            ..
        } = self;
        CellOps::insert_lines(pen_state, normal, cursor, n);
    }

    pub fn delete_lines(&mut self, n: usize) {
        let Screen {
            normal,
            cursor,
            pen_state,
            ..
        } = self;
        CellOps::delete_lines(pen_state, normal, cursor, n);
    }

    // ── scroll region (CSI S / CSI T) ──────────────────────────────────

    pub fn scroll_up_region(&mut self, n: usize) {
        let Screen {
            normal,
            cursor,
            pen_state,
            ..
        } = self;
        CellOps::scroll_up_region(pen_state, normal, cursor, n);
    }

    pub fn scroll_down_region(&mut self, n: usize) {
        let Screen {
            normal,
            cursor,
            pen_state,
            ..
        } = self;
        CellOps::scroll_down_region(pen_state, normal, cursor, n);
    }

    // ── DEC rectangle ops ──────────────────────────────────────────────

    pub fn decera(&mut self, params: &Params) {
        let Screen {
            normal,
            cursor,
            pen_state,
            ..
        } = self;
        CellOps::decera(pen_state, normal, cursor, params);
    }

    pub fn decsera(&mut self, params: &Params) {
        let Screen {
            normal,
            cursor,
            pen_state,
            ..
        } = self;
        CellOps::decsera(pen_state, normal, cursor, params);
    }

    pub fn decfra(&mut self, params: &Params) {
        let Screen {
            normal,
            cursor,
            pen_state,
            ..
        } = self;
        CellOps::decfra(pen_state, normal, cursor, params);
    }

    pub fn deccra(&mut self, params: &Params) {
        let Screen {
            normal,
            cursor,
            pen_state,
            ..
        } = self;
        CellOps::deccra(pen_state, normal, cursor, params);
    }

    pub fn deccara(&mut self, params: &Params) {
        let Screen { normal, cursor, .. } = self;
        CellOps::deccara(normal, cursor, params);
    }

    pub fn decrara(&mut self, params: &Params) {
        let Screen { normal, cursor, .. } = self;
        CellOps::decrara(normal, cursor, params);
    }

    // ── tab stops ──────────────────────────────────────────────────────

    pub fn set_tab_stop(&mut self) {
        self.pen_state.set_tab_stop(self.cursor.cursor.x);
    }

    pub fn clear_tab_stops(&mut self, mode: usize) {
        self.pen_state.clear_tab_stops(self.cursor.cursor.x, mode);
    }

    // ── cursor save / restore ──────────────────────────────────────────

    pub fn save_cursor(&mut self) {
        self.sync_current_cursor_anchor();
        self.cursor.save_cursor_position();
        self.pen_state.save_pen();
    }

    pub fn restore_cursor(&mut self) {
        self.cursor.restore_cursor_position();
        self.cursor.cursor.anchor = self
            .cursor
            .cursor
            .saved
            .as_ref()
            .and_then(|saved| saved.anchor);
        self.pen_state.restore_pen();
    }

    // ── write_char (coordinator) ───────────────────────────────────────

    pub fn write_char(&mut self, ch: char) {
        let width = UnicodeWidthChar::width(ch).unwrap_or(0);
        let cursor_before = (
            self.cursor.cursor.x,
            self.cursor.cursor.y,
            self.cursor.modes.pending_wrap,
        );
        let before_generation = self.live_cursor_position().generation;
        let right_before_write = if self.cursor.margins.enabled {
            self.cursor.margins.right
        } else {
            self.normal.cols().saturating_sub(1)
        };
        let wide_wraps =
            width == 2 && self.cursor.modes.autowrap && self.cursor.cursor.x == right_before_write;
        let before = if self.cursor.modes.insert
            && !self.cursor.modes.pending_wrap
            && width > 0
            && !wide_wraps
        {
            let requested_col = self.cursor.cursor.x;
            let (left, right) = if self.cursor.margins.enabled {
                (self.cursor.margins.left, self.cursor.margins.right)
            } else {
                (0, self.normal.cols().saturating_sub(1))
            };
            let base = CellOps::wide_range(&self.normal, self.cursor.cursor.y, requested_col)
                .map_or(requested_col, |(base, _)| base);
            let actual = if requested_col >= left && requested_col <= right {
                width.min(right - base + 1)
            } else {
                0
            };
            let dropped = (actual > 0).then_some((right + 1 - actual, right.saturating_add(1)));
            self.capture_edit_anchor(actual, dropped)
        } else {
            None
        };
        let Screen {
            normal,
            cursor,
            pen_state,
            ..
        } = self;
        CellWriter::write_char(pen_state, normal, cursor, ch);
        normal.repair_following_soft_chain(cursor.cursor.y);
        let cursor_after = (
            self.cursor.cursor.x,
            self.cursor.cursor.y,
            self.cursor.modes.pending_wrap,
        );
        let inserted_atoms = usize::from(
            width > 0
                && cursor_after != cursor_before
                && self.live_cursor_position().generation == before_generation,
        );
        if inserted_atoms > 0 {
            self.record_insertion_delta(before, inserted_atoms, true);
        }
    }

    // ── horizontal_tab (coordinator) ───────────────────────────────────

    pub fn horizontal_tab(&mut self) {
        self.cursor.clear_pending_wrap();
        let right_limit = if self.cursor.margins.enabled {
            self.cursor.margins.right
        } else {
            self.normal.cols()
        };
        let mut target = right_limit;
        for col in (self.cursor.cursor.x + 1)..=right_limit {
            if col < self.pen_state.tab_stops.0.len() && self.pen_state.tab_stops.0[col] {
                target = col;
                break;
            }
        }
        if target > self.cursor.cursor.x {
            let spaces = target - self.cursor.cursor.x;
            let Screen {
                normal,
                cursor,
                pen_state,
                ..
            } = self;
            for _ in 0..spaces {
                CellWriter::write_internal_char(pen_state, normal, cursor, ' ');
            }
        }
        self.cursor.clear_pending_wrap();
    }

    /// Moves forward over `steps` tab stops without changing any cells.
    pub fn forward_tab(&mut self, steps: usize) {
        self.cursor.clear_pending_wrap();
        let steps = steps.max(1);
        let (left_limit, right_limit) = if self.cursor.margins.enabled {
            (self.cursor.margins.left, self.cursor.margins.right)
        } else {
            (0, self.normal.cols().saturating_sub(1))
        };
        let start = self.cursor.cursor.x.saturating_add(1).max(left_limit);

        if start <= right_limit {
            let mut remaining = steps;
            for col in start..=right_limit {
                if self.pen_state.tab_stops.0[col] {
                    remaining -= 1;
                    if remaining == 0 {
                        self.cursor.cursor.x = col;
                        return;
                    }
                }
            }
        }
        self.cursor.cursor.x = right_limit;
    }

    /// Moves backward over `steps` tab stops without changing any cells.
    pub fn backward_tab(&mut self, steps: usize) {
        self.cursor.clear_pending_wrap();
        let steps = steps.max(1);
        let (left_limit, right_limit) = if self.cursor.margins.enabled {
            (self.cursor.margins.left, self.cursor.margins.right)
        } else {
            (0, self.normal.cols().saturating_sub(1))
        };
        let end = self.cursor.cursor.x.saturating_sub(1).min(right_limit);

        if end >= left_limit {
            let mut remaining = steps;
            for col in (left_limit..=end).rev() {
                if self.pen_state.tab_stops.0[col] {
                    remaining -= 1;
                    if remaining == 0 {
                        self.cursor.cursor.x = col;
                        return;
                    }
                }
            }
        }
        self.cursor.cursor.x = left_limit;
    }

    // ── repeat_char (coordinator) ──────────────────────────────────────

    pub fn repeat_char(&mut self, n: usize) {
        if let Some(ch) = self.pen_state.charsets.last_char {
            let n = if n == 0 { 1 } else { n };
            let count = n.min(self.normal.cols());
            let Screen {
                normal,
                cursor,
                pen_state,
                ..
            } = self;
            for _ in 0..count {
                CellWriter::write_internal_char(pen_state, normal, cursor, ch);
            }
        }
    }

    // ── newline / line_feed / index (coordinators) ─────────────────────

    pub fn newline(&mut self) {
        self.cursor.carriage_return();
        self.index();
    }

    pub fn line_feed(&mut self) {
        if self.cursor.modes.line_feed {
            self.cursor.carriage_return();
        }
        self.index();
    }

    pub fn index(&mut self) {
        self.cursor.clear_pending_wrap();
        let before = self.cursor.cursor.y;
        let scrolled = self.cursor.index_needs_scroll();
        if scrolled {
            self.scroll_region_up_one();
        } else {
            self.cursor.index_advance(&self.normal);
        }
        // Only mark the destination row as a new logical line when the cursor
        // actually moved or a scroll occurred; a no-op index (cursor pinned at
        // the bottom below the scroll region) must not clear an existing flag.
        if scrolled || self.cursor.cursor.y != before {
            self.normal.begin_hard_line(self.cursor.cursor.y);
        }
    }

    // ── reverse_index (coordinator) ────────────────────────────────────

    pub fn reverse_index(&mut self) {
        self.cursor.clear_pending_wrap();
        let before = self.cursor.cursor.y;
        let mut transitioned = false;
        tracing::debug!(
            cursor_y = self.cursor.cursor.y,
            scroll_top = self.cursor.scroll_region.top,
            scroll_bottom = self.cursor.scroll_region.bottom,
            full_screen = (self.cursor.scroll_region.top == 0
                && self.cursor.scroll_region.bottom == self.normal.rows() - 1),
            "reverse_index"
        );

        if self.cursor.cursor.y == self.cursor.scroll_region.top
            && self.cursor.cursor.y <= self.cursor.scroll_region.bottom
        {
            transitioned = true;
            self.mark_rows_dirty(
                self.cursor.scroll_region.top,
                self.cursor.scroll_region.bottom.saturating_add(1),
            );
            if self.cursor.margins.enabled {
                let Screen {
                    normal,
                    cursor,
                    pen_state,
                    ..
                } = self;
                CellOps::scroll_margin_rect_down(
                    pen_state,
                    normal,
                    cursor,
                    cursor.scroll_region.top,
                    cursor.scroll_region.bottom,
                    1,
                );
            } else {
                let tr = self.normal.total_rows();
                let vis = self.normal.visible_start();
                let c = self.normal.cols();
                let src_start = ((vis + self.cursor.scroll_region.top) % tr) * c;
                let src_end = ((vis + self.cursor.scroll_region.bottom) % tr) * c;
                let dst = ((vis + self.cursor.scroll_region.top + 1) % tr) * c;
                self.normal
                    .copy_ring_rows(src_start / c, src_end / c, dst / c);
                self.normal
                    .fill_row_with(self.cursor.scroll_region.top, self.pen_state.erase_cell());
            }
        } else if self.cursor.cursor.y > 0 {
            self.cursor.cursor.y -= 1;
        }
        if transitioned || self.cursor.cursor.y != before {
            self.normal.begin_hard_line(self.cursor.cursor.y);
        }
    }
    pub(crate) fn requires_anchor_baseline(&self) -> bool {
        self.review_anchor.is_some()
            || self
                .cursor
                .cursor
                .saved
                .as_ref()
                .is_some_and(|saved| saved.anchor.is_some())
    }

    // ── scroll_region_up_one (coordinator) ─────────────────────────────

    fn scroll_region_up_one(&mut self) {
        tracing::debug!(
            scroll_top = self.cursor.scroll_region.top,
            scroll_bottom = self.cursor.scroll_region.bottom,
            visible_rows = self.normal.rows(),
            full_screen = (self.cursor.scroll_region.top == 0
                && self.cursor.scroll_region.bottom == self.normal.rows() - 1),
            "scroll_region_up_one"
        );

        self.mark_rows_dirty(
            self.cursor.scroll_region.top,
            self.cursor.scroll_region.bottom.saturating_add(1),
        );
        if self.cursor.margins.enabled {
            let Screen {
                normal,
                cursor,
                pen_state,
                ..
            } = self;
            CellOps::scroll_margin_rect_up(
                pen_state,
                normal,
                cursor,
                cursor.scroll_region.top,
                cursor.scroll_region.bottom,
                1,
            );
        } else if self.cursor.scroll_region.top == 0
            && self.cursor.scroll_region.bottom == self.normal.rows() - 1
        {
            self.normal
                .scroll_up_full_screen(1, self.pen_state.erase_cell());
        } else {
            let tr = self.normal.total_rows();
            let vis = self.normal.visible_start();
            let c = self.normal.cols();
            let src_start = ((vis + self.cursor.scroll_region.top + 1) % tr) * c;
            let src_end = ((vis + self.cursor.scroll_region.bottom + 1) % tr) * c;
            let dst = ((vis + self.cursor.scroll_region.top) % tr) * c;
            self.normal
                .copy_ring_rows(src_start / c, src_end / c, dst / c);
            self.normal.fill_row_with(
                self.cursor.scroll_region.bottom,
                self.pen_state.erase_cell(),
            );
        }
        self.cursor.cursor.y = self.cursor.scroll_region.bottom;
    }

    // ── screen alignment / reset ───────────────────────────────────────

    /// Performs DECALN (`ESC # 8`) on the active visible buffer.
    pub fn decaln(&mut self) {
        let cell = Cell {
            ch: 'E',
            ..Cell::default()
        };
        self.normal.fill_all_with(cell);
        self.cursor.alignment_home();
        self.mark_all_dirty();
    }

    pub fn reset_display(&mut self) {
        self.synchronized_output.clear();
        self.anchor_mutations.push(AnchorMutation::InvalidateAll);
        self.review_anchor = None;
        self.focus_reporting.reset_for_ris();
        self.alt_request = None;
        self.saved_primary = None;
        self.parked_alt = None;

        let rows = self.normal.rows();
        let cols = self.normal.cols();
        self.normal.reset_all_retained();
        self.cursor.reset(rows, cols);
        self.pen_state.reset(cols);
        self.hyperlinks.clear();
        self.mark_all_dirty();
    }

    pub fn soft_reset(&mut self) {
        let rows = self.normal.rows();
        let cols = self.normal.cols();
        self.cursor.reset(rows, cols);
        self.pen_state.soft_reset();
    }

    // ── misc ───────────────────────────────────────────────────────────

    pub fn row_text(&self, row: usize) -> String {
        self.normal.row_text(row)
    }

    /// Returns whether the given display row is a soft-wrapped continuation of
    /// the logical line above. `row` is viewport-relative (0..`rows`) and
    /// view-offset aware — it reports the row currently displayed at that
    /// position, consistent with `cell`/`cells`. Consumers working in
    /// scrollback generation coordinates must map to display rows first.
    pub fn is_wrapped(&self, row: usize) -> bool {
        self.normal.is_wrapped(row)
    }
}
