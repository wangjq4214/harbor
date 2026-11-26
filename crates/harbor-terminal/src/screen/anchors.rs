//! Content-anchor mutation bookkeeping and review/saved-cursor reconciliation.

use crate::content_anchor::{
    Affinity, AnchorMutation, AnchorMutationBatch, ContentAnchor, ContentProjection,
};
use crate::logical_content::{DecodeError, LogicalAtomOffset};
use crate::normal_buf::LogicalLineId;
use crate::selection_model::GenPos;

use super::Screen;
use super::edit::CellOps;

#[derive(Debug)]
pub(super) struct EditAnchorCapture {
    pub(super) anchor: ContentAnchor,
    pub(super) atom_count: usize,
    pub(super) affected_atoms: usize,
    pub(super) shifted_atoms: usize,
    pub(super) dropped_atoms: Option<(usize, usize)>,
}

impl Screen {
    pub(crate) fn content_projection(&self) -> Result<ContentProjection, DecodeError> {
        ContentProjection::build(&self.active.normal)
    }

    pub(super) fn capture_edit_anchor(
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

    pub(super) fn capture_insertion_anchor(
        &self,
        requested: usize,
    ) -> (usize, Option<EditAnchorCapture>) {
        let requested_col = self.active.cursor.cursor.x;
        let (left, right) = self
            .active
            .cursor
            .margins
            .active_bounds(self.active.normal.cols());
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

    pub(super) fn record_insertion_delta(
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

    pub(super) fn record_deletion_delta(&mut self, before: Option<EditAnchorCapture>) {
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

    pub(super) fn sync_review_anchor(&mut self) {
        if self.active.normal.view_offset() == 0 {
            self.active.review_anchor = None;
            return;
        }
        let generation = self.active.normal.view_start_generation();
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
                    let live_top = self.active.normal.live_top_generation();
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

    pub(super) fn finish_transition_anchors(&mut self) {
        if self.finish_anchor_mutations(None).is_err() {
            self.active.cursor.cursor.saved = None;
            self.active.review_anchor = None;
            self.active.anchor_mutations = AnchorMutationBatch::default();
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
}
