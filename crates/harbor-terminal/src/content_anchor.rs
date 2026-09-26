//! Crate-private logical content positions and mutation transforms.

use crate::logical_content::{self, DecodeError, LogicalAtom, LogicalAtomOffset, SourceSpan};
use crate::normal_buf::{LogicalLineId, NormalBuf};
use crate::selection_model::GenPos;

use std::collections::HashMap;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Affinity {
    Before,
    After,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ContentAnchor {
    pub(crate) line_id: LogicalLineId,
    pub(crate) offset: LogicalAtomOffset,
    pub(crate) affinity: Affinity,
    projection_hint: Option<GenPos>,
    prefer_previous_projection: bool,
}

impl ContentAnchor {
    pub(crate) const fn from_logical_parts(
        line_id: LogicalLineId,
        offset: LogicalAtomOffset,
        affinity: Affinity,
        projection_hint: Option<GenPos>,
        prefer_previous_projection: bool,
    ) -> Self {
        Self {
            line_id,
            offset,
            affinity,
            projection_hint,
            prefer_previous_projection,
        }
    }

    pub(crate) const fn projection_hint_col(self) -> Option<usize> {
        match self.projection_hint {
            Some(position) => Some(position.col),
            None => None,
        }
    }

    pub(crate) const fn projection_hint(self) -> Option<GenPos> {
        self.projection_hint
    }

    pub(crate) const fn prefers_previous_projection(self) -> bool {
        self.prefer_previous_projection
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProjectedPosition {
    pub(crate) pos: GenPos,
    pub(crate) pending_wrap: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AnchorMutation {
    Insert {
        line_id: LogicalLineId,
        at: LogicalAtomOffset,
        count: usize,
    },
    Delete {
        line_id: LogicalLineId,
        start: LogicalAtomOffset,
        end: LogicalAtomOffset,
    },
    Replace {
        line_id: LogicalLineId,
        start: LogicalAtomOffset,
        old_count: usize,
        new_count: usize,
    },
    EvictPrefix {
        line_id: LogicalLineId,
        end: LogicalAtomOffset,
    },
    ReprojectLine(LogicalLineId),
    InvalidateLine(LogicalLineId),
    InvalidateAll,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct AnchorMutationBatch {
    mutations: Vec<AnchorMutation>,
}

impl AnchorMutationBatch {
    pub(crate) fn push(&mut self, mutation: AnchorMutation) {
        self.mutations.push(mutation);
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = AnchorMutation> + '_ {
        self.mutations.iter().copied()
    }

    pub(crate) fn affects_line(&self, line_id: LogicalLineId) -> bool {
        self.mutations.iter().any(|mutation| {
            matches!(
                mutation,
                AnchorMutation::Insert { line_id: id, .. }
                    | AnchorMutation::Delete { line_id: id, .. }
                    | AnchorMutation::Replace { line_id: id, .. }
                    | AnchorMutation::EvictPrefix { line_id: id, .. }
                    | AnchorMutation::ReprojectLine(id)
                    | AnchorMutation::InvalidateLine(id)
                    if *id == line_id
            ) || matches!(mutation, AnchorMutation::InvalidateAll)
        })
    }

    pub(crate) fn apply(&self, anchor: ContentAnchor) -> Option<ContentAnchor> {
        self.mutations
            .iter()
            .try_fold(anchor, |current, mutation| mutation.apply(current))
    }
}

impl AnchorMutation {
    fn apply(self, mut anchor: ContentAnchor) -> Option<ContentAnchor> {
        match self {
            Self::InvalidateAll => return None,
            Self::InvalidateLine(line_id) if anchor.line_id == line_id => return None,
            Self::Insert { line_id, at, count } if anchor.line_id == line_id => {
                anchor.projection_hint = None;
                if anchor.offset.0 > at.0
                    || (anchor.offset == at && anchor.affinity == Affinity::After)
                {
                    anchor.offset.0 = anchor
                        .offset
                        .0
                        .checked_add(count)
                        .expect("content-anchor insertion offset overflow");
                }
            }
            Self::EvictPrefix { line_id, end } if anchor.line_id == line_id => {
                anchor.projection_hint = None;
                if anchor.offset.0 < end.0 {
                    return None;
                }
            }
            Self::ReprojectLine(line_id) if anchor.line_id == line_id => {
                anchor.projection_hint = None;
            }
            Self::Replace {
                line_id,
                start,
                old_count,
                new_count,
            } if anchor.line_id == line_id => {
                anchor.projection_hint = None;
                let end = start.0.checked_add(old_count)?;
                if anchor.offset.0 >= end {
                    if new_count >= old_count {
                        anchor.offset.0 = anchor
                            .offset
                            .0
                            .checked_add(new_count - old_count)
                            .expect("content-anchor replacement offset overflow");
                    } else {
                        anchor.offset.0 = anchor.offset.0.checked_sub(old_count - new_count)?;
                    }
                } else if anchor.offset.0 >= start.0 {
                    let relative = anchor.offset.0 - start.0;
                    anchor.offset.0 = start.0.checked_add(relative.min(new_count))?;
                }
            }
            Self::Delete {
                line_id,
                start,
                end,
            } if anchor.line_id == line_id => {
                anchor.projection_hint = None;
                let removed = end.0.checked_sub(start.0)?;
                if anchor.offset.0 >= end.0 {
                    anchor.offset.0 = anchor.offset.0.checked_sub(removed)?;
                } else if anchor.offset.0 >= start.0 {
                    anchor.offset = start;
                }
            }
            _ => {}
        }
        Some(anchor)
    }
}

#[derive(Clone, Debug)]
struct ProjectedAtom {
    offset: LogicalAtomOffset,
    source_span: SourceSpan,
    cell: crate::screen::Cell,
}

#[derive(Clone, Debug)]
struct ProjectedLine {
    line_id: LogicalLineId,
    generations: Vec<u64>,
    atom_start: LogicalAtomOffset,
    atoms: Vec<ProjectedAtom>,
}

/// Owned, bounded mapping for the retained rows of one `NormalBuf`.
#[derive(Clone, Debug)]
pub(crate) struct ContentProjection {
    cols: usize,
    history_start: u64,
    scroll_count: usize,
    lines: Vec<ProjectedLine>,
    line_indices: HashMap<LogicalLineId, usize>,
}

impl ContentProjection {
    pub(crate) fn build(normal: &NormalBuf) -> Result<Self, DecodeError> {
        let mut lines: Vec<ProjectedLine> = Vec::new();
        let mut line_indices: HashMap<LogicalLineId, usize> = HashMap::new();
        for row in normal.retained_rows() {
            if let Some(index) = line_indices.get(&row.metadata.logical_line_id).copied() {
                lines[index].generations.push(row.generation);
            } else {
                let index = lines.len();
                line_indices.insert(row.metadata.logical_line_id, index);
                lines.push(ProjectedLine {
                    line_id: row.metadata.logical_line_id,
                    generations: vec![row.generation],
                    atom_start: LogicalAtomOffset(row.metadata.logical_atom_start),
                    atoms: Vec::new(),
                });
            }
        }

        for atom in logical_content::decode(normal)? {
            if let LogicalAtom::Glyph {
                line_id,
                atom_offset,
                source_span,
                glyph,
            } = atom
                && let Some(index) = line_indices.get(&line_id).copied()
                && let Some(line) = lines.get_mut(index)
            {
                line.atoms.push(ProjectedAtom {
                    offset: atom_offset,
                    cell: glyph.cell,
                    source_span,
                });
            }
        }

        Ok(Self {
            cols: normal.cols(),
            history_start: normal.history_start(),
            scroll_count: normal.scroll_count(),
            lines,
            line_indices,
        })
    }

    pub(crate) fn to_anchor(&self, pos: GenPos, affinity: Affinity) -> Option<ContentAnchor> {
        if pos.col >= self.cols {
            return None;
        }
        let line = self
            .lines
            .iter()
            .find(|line| line.generations.contains(&pos.generation))?;
        let covering = line.atoms.iter().find(|atom| {
            atom.source_span.generation == pos.generation
                && pos.col >= atom.source_span.start_col
                && pos.col <= atom.source_span.end_col
        });
        let (offset, projection_hint, prefer_previous_projection) = if let Some(atom) = covering {
            (atom.offset, None, false)
        } else {
            let next = line.atoms.iter().find(|atom| {
                atom.source_span.generation > pos.generation
                    || (atom.source_span.generation == pos.generation
                        && atom.source_span.start_col > pos.col)
            });
            (
                next.map_or(
                    LogicalAtomOffset(line.atom_start.0.checked_add(line.atoms.len())?),
                    |atom| atom.offset,
                ),
                Some(pos),
                next.is_some(),
            )
        };
        Some(ContentAnchor {
            line_id: line.line_id,
            offset,
            affinity,
            projection_hint,
            prefer_previous_projection,
        })
    }

    pub(crate) fn cursor_anchor(&self, pos: GenPos, pending_wrap: bool) -> Option<ContentAnchor> {
        // Saving a pending-wrap cursor must retain its physical source cell,
        // not jump past any continuation rows later in the logical line.
        self.to_anchor(
            pos,
            if pending_wrap {
                Affinity::After
            } else {
                Affinity::Before
            },
        )
    }

    pub(crate) fn resolve_selection(&self, anchor: ContentAnchor) -> Option<GenPos> {
        let line = self.line(anchor.line_id)?;
        if let Some(hint) = anchor.projection_hint
            && hint.col < self.cols
            && line.generations.contains(&hint.generation)
        {
            return Some(hint);
        }
        if anchor.prefer_previous_projection {
            if let Some(previous_offset) = anchor.offset.0.checked_sub(1)
                && let Some(previous) = line
                    .atoms
                    .iter()
                    .find(|atom| atom.offset.0 == previous_offset)
            {
                return Some(GenPos::new(
                    previous.source_span.generation,
                    previous.source_span.end_col,
                ));
            }
            return Some(GenPos::new(*line.generations.first()?, 0));
        }
        if let Some(atom) = line.atoms.iter().find(|atom| atom.offset == anchor.offset) {
            return Some(GenPos::new(
                atom.source_span.generation,
                match anchor.affinity {
                    Affinity::Before => atom.source_span.start_col,
                    Affinity::After => atom.source_span.end_col,
                },
            ));
        }
        if anchor.offset.0 != line.atom_start.0.checked_add(line.atoms.len())? {
            return None;
        }
        let generation = *line.generations.last()?;
        let col = match anchor.affinity {
            Affinity::Before => line.atoms.last().map_or(0, |atom| atom.source_span.end_col),
            Affinity::After => self.cols.saturating_sub(1),
        };
        Some(GenPos::new(generation, col))
    }

    pub(crate) fn resolve_cursor(&self, anchor: ContentAnchor) -> Option<ProjectedPosition> {
        let line = self.line(anchor.line_id)?;
        if let Some(hint) = anchor.projection_hint
            && hint.col < self.cols
            && line.generations.contains(&hint.generation)
        {
            return Some(ProjectedPosition {
                pos: hint,
                pending_wrap: false,
            });
        }
        if anchor.prefer_previous_projection {
            if let Some(previous_offset) = anchor.offset.0.checked_sub(1)
                && let Some(previous) = line
                    .atoms
                    .iter()
                    .find(|atom| atom.offset.0 == previous_offset)
            {
                return Some(ProjectedPosition {
                    pos: GenPos::new(
                        previous.source_span.generation,
                        previous
                            .source_span
                            .end_col
                            .saturating_add(1)
                            .min(self.cols.saturating_sub(1)),
                    ),
                    pending_wrap: false,
                });
            }
            return Some(ProjectedPosition {
                pos: GenPos::new(*line.generations.first()?, 0),
                pending_wrap: false,
            });
        }
        let (generation, insertion_col) =
            if let Some(atom) = line.atoms.iter().find(|atom| atom.offset == anchor.offset) {
                let col = match anchor.affinity {
                    Affinity::Before => atom.source_span.start_col,
                    Affinity::After => atom.source_span.end_col.saturating_add(1),
                };
                (atom.source_span.generation, col)
            } else if anchor.offset.0 == line.atom_start.0.checked_add(line.atoms.len())? {
                let generation = *line.generations.last()?;
                let col = line
                    .atoms
                    .last()
                    .map_or(0, |atom| atom.source_span.end_col.saturating_add(1));
                (generation, col)
            } else {
                return None;
            };
        let pending_wrap = insertion_col >= self.cols;
        Some(ProjectedPosition {
            pos: GenPos::new(generation, insertion_col.min(self.cols.saturating_sub(1))),
            pending_wrap,
        })
    }

    pub(crate) fn line_cells(&self, line_id: LogicalLineId) -> Option<Vec<crate::screen::Cell>> {
        Some(
            self.line(line_id)?
                .atoms
                .iter()
                .map(|atom| atom.cell)
                .collect(),
        )
    }

    pub(crate) fn atom_count_in_cell_range(
        &self,
        line_id: LogicalLineId,
        generation: u64,
        start_col: usize,
        end_col: usize,
    ) -> Option<usize> {
        Some(
            self.line(line_id)?
                .atoms
                .iter()
                .filter(|atom| {
                    atom.source_span.generation == generation
                        && atom.source_span.end_col >= start_col
                        && atom.source_span.start_col < end_col
                })
                .count(),
        )
    }

    pub(crate) fn atom_range_in_cell_range(
        &self,
        line_id: LogicalLineId,
        generation: u64,
        start_col: usize,
        end_col: usize,
    ) -> Option<Option<(usize, usize)>> {
        let line = self.line(line_id)?;
        let mut offsets = line
            .atoms
            .iter()
            .filter(|atom| {
                atom.source_span.generation == generation
                    && atom.source_span.end_col >= start_col
                    && atom.source_span.start_col < end_col
            })
            .map(|atom| atom.offset.0);
        let Some(first) = offsets.next() else {
            return Some(None);
        };
        let last = offsets.next_back().unwrap_or(first);
        Some(Some((first, last - first + 1)))
    }

    pub(crate) fn history_start(&self) -> u64 {
        self.history_start
    }

    pub(crate) fn first_generation(&self, line_id: LogicalLineId) -> Option<u64> {
        self.line(line_id)?.generations.first().copied()
    }

    pub(crate) fn atom_start(&self, line_id: LogicalLineId) -> Option<LogicalAtomOffset> {
        Some(self.line(line_id)?.atom_start)
    }

    pub(crate) fn atom_spans(&self, line_id: LogicalLineId) -> Option<Vec<(u64, usize, usize)>> {
        Some(
            self.line(line_id)?
                .atoms
                .iter()
                .map(|atom| {
                    (
                        atom.source_span.generation,
                        atom.source_span.start_col,
                        atom.source_span.end_col,
                    )
                })
                .collect(),
        )
    }

    pub(crate) fn line_ids(&self) -> impl Iterator<Item = LogicalLineId> + '_ {
        self.lines.iter().map(|line| line.line_id)
    }
    pub(crate) fn contains_generation(&self, line_id: LogicalLineId, generation: u64) -> bool {
        self.line(line_id)
            .is_some_and(|line| line.generations.contains(&generation))
    }

    pub(crate) fn contains_line(&self, line_id: LogicalLineId) -> bool {
        self.line(line_id).is_some()
    }

    pub(crate) fn oldest_anchor(&self) -> Option<ContentAnchor> {
        let line = self.lines.first()?;
        Some(ContentAnchor {
            line_id: line.line_id,
            offset: line.atom_start,
            affinity: Affinity::Before,
            projection_hint: None,
            prefer_previous_projection: false,
        })
    }

    pub(crate) fn view_offset_for(&self, anchor: ContentAnchor) -> Option<usize> {
        let generation = self.resolve_selection(anchor)?.generation;
        let live_top = self.history_start.checked_add(self.scroll_count as u64)?;
        usize::try_from(live_top.saturating_sub(generation))
            .ok()
            .map(|offset| offset.min(self.scroll_count))
    }

    fn line(&self, line_id: LogicalLineId) -> Option<&ProjectedLine> {
        let index = *self.line_indices.get(&line_id)?;
        self.lines.get(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchor(offset: usize, affinity: Affinity) -> ContentAnchor {
        ContentAnchor {
            line_id: LogicalLineId(7),
            offset: LogicalAtomOffset(offset),
            affinity,
            projection_hint: None,
            prefer_previous_projection: false,
        }
    }

    #[test]
    fn insertion_obeys_equal_boundary_affinity() {
        let mutation = AnchorMutation::Insert {
            line_id: LogicalLineId(7),
            at: LogicalAtomOffset(2),
            count: 3,
        };
        assert_eq!(
            mutation.apply(anchor(2, Affinity::Before)),
            Some(anchor(2, Affinity::Before))
        );
        assert_eq!(
            mutation.apply(anchor(2, Affinity::After)),
            Some(anchor(5, Affinity::After))
        );
        assert_eq!(
            mutation.apply(anchor(4, Affinity::Before)),
            Some(anchor(7, Affinity::Before))
        );
        assert_eq!(
            mutation.apply(ContentAnchor {
                line_id: LogicalLineId(8),
                ..anchor(4, Affinity::Before)
            }),
            Some(ContentAnchor {
                line_id: LogicalLineId(8),
                ..anchor(4, Affinity::Before)
            })
        );
    }

    #[test]
    fn deletion_collapses_inside_and_shifts_later_boundaries() {
        let mutation = AnchorMutation::Delete {
            line_id: LogicalLineId(7),
            start: LogicalAtomOffset(2),
            end: LogicalAtomOffset(5),
        };
        assert_eq!(
            mutation.apply(anchor(1, Affinity::After)),
            Some(anchor(1, Affinity::After))
        );
        assert_eq!(
            mutation.apply(anchor(3, Affinity::After)),
            Some(anchor(2, Affinity::After))
        );
        assert_eq!(
            mutation.apply(anchor(5, Affinity::Before)),
            Some(anchor(2, Affinity::Before))
        );
        assert_eq!(
            mutation.apply(anchor(8, Affinity::Before)),
            Some(anchor(5, Affinity::Before))
        );
    }

    #[test]
    fn ordered_batch_adjusts_then_invalidates() {
        let mut batch = AnchorMutationBatch::default();
        batch.push(AnchorMutation::Insert {
            line_id: LogicalLineId(7),
            at: LogicalAtomOffset(0),
            count: 1,
        });
        batch.push(AnchorMutation::InvalidateLine(LogicalLineId(7)));
        assert_eq!(batch.apply(anchor(2, Affinity::Before)), None);
    }

    #[test]
    #[should_panic(expected = "content-anchor insertion offset overflow")]
    fn insertion_overflow_reports_invariant_failure() {
        let mutation = AnchorMutation::Insert {
            line_id: LogicalLineId(7),
            at: LogicalAtomOffset(0),
            count: 1,
        };
        let _ = mutation.apply(anchor(usize::MAX, Affinity::Before));
    }

    #[test]
    fn projection_round_trips_wide_cells_and_blank_line_boundaries() {
        use crate::screen::Cell;

        let mut normal = NormalBuf::new(2, 4);
        normal.write_meaningful_cell(
            0,
            0,
            Cell {
                ch: '界',
                ..Cell::default()
            },
        );
        normal.write_meaningful_cell(
            0,
            1,
            Cell {
                wide_continuation: true,
                ..Cell::default()
            },
        );

        let projection = ContentProjection::build(&normal).unwrap();
        let lead = projection
            .to_anchor(GenPos::new(0, 0), Affinity::Before)
            .unwrap();
        let continuation = projection
            .to_anchor(GenPos::new(0, 1), Affinity::After)
            .unwrap();
        assert_eq!(lead.line_id, continuation.line_id);
        assert_eq!(lead.offset, continuation.offset);
        assert_eq!(projection.resolve_selection(lead), Some(GenPos::new(0, 0)));
        assert_eq!(
            projection.resolve_selection(continuation),
            Some(GenPos::new(0, 1))
        );

        let blank_start = projection
            .to_anchor(GenPos::new(1, 0), Affinity::Before)
            .unwrap();
        let blank_end = projection
            .to_anchor(GenPos::new(1, 3), Affinity::After)
            .unwrap();
        assert_eq!(blank_start.offset, LogicalAtomOffset(0));
        assert_eq!(blank_end.offset, LogicalAtomOffset(0));
        assert_eq!(
            projection.resolve_selection(blank_start),
            Some(GenPos::new(1, 0))
        );
        assert_eq!(
            projection.resolve_selection(blank_end),
            Some(GenPos::new(1, 3))
        );
    }

    #[test]
    fn generated_wide_edge_padding_round_trips_on_current_geometry() {
        let mut screen = crate::Screen::new(2, 3);
        screen.write_char('a');
        screen.write_char('b');
        screen.write_char('界');
        let before = screen.reader().content_projection().unwrap();
        let padding = GenPos::new(0, 2);
        let anchor = before
            .to_anchor(padding, Affinity::Before)
            .expect("generated padding anchor");
        assert_eq!(anchor.offset, LogicalAtomOffset(2));
        assert_eq!(before.resolve_selection(anchor), Some(padding));

        screen.set_cursor_position(1, 1);
        screen.insert_chars(1);
        let (mutations, projection) = screen
            .finish_anchor_mutations(Some(&before))
            .expect("projection after insertion");
        let adjusted = mutations.apply(anchor).expect("padding boundary survives");

        assert_eq!(adjusted.offset, LogicalAtomOffset(3));
        assert_eq!(projection.resolve_selection(adjusted), Some(padding));

        let mut screen = crate::Screen::new(2, 3);
        screen.write_char('a');
        screen.write_char('b');
        screen.write_char('界');
        let before = screen.reader().content_projection().unwrap();
        let wide = before
            .to_anchor(GenPos::new(1, 0), Affinity::After)
            .expect("wide glyph anchor");

        screen.set_cursor_position(1, 3);
        screen.insert_chars(1);
        let (mutations, projection) = screen
            .finish_anchor_mutations(Some(&before))
            .expect("projection after padding insertion");
        let adjusted = mutations.apply(wide).expect("wide glyph survives");

        assert_eq!(adjusted.offset, wide.offset);
        assert_eq!(
            projection.resolve_selection(adjusted),
            Some(GenPos::new(1, 1))
        );
    }

    #[test]
    fn equal_atom_count_width_change_requires_reprojection() {
        let mut screen = crate::Screen::new(1, 4);
        screen.write_char('a');
        screen.write_char('b');
        let (_, before) = screen.finish_anchor_mutations(None).unwrap();
        let anchor = before
            .to_anchor(GenPos::new(0, 1), Affinity::After)
            .unwrap();

        screen.set_cursor_position(1, 2);
        screen.write_char('界');
        let (mutations, projection) = screen
            .finish_anchor_mutations(Some(&before))
            .expect("projection after width-changing overwrite");

        assert!(mutations.affects_line(anchor.line_id));
        let adjusted = mutations.apply(anchor).expect("anchor survives overwrite");
        assert_eq!(
            projection.resolve_selection(adjusted),
            Some(GenPos::new(0, 2))
        );
    }

    #[test]
    fn projection_rejects_evicted_identity_after_ring_slot_reuse() {
        let mut normal = NormalBuf::new(1, 2);
        let projection = ContentProjection::build(&normal).unwrap();
        let stale = projection
            .to_anchor(GenPos::new(0, 0), Affinity::Before)
            .unwrap();
        for _ in 0..=normal.max_scrollback() {
            normal.scroll_up_full_screen(1, crate::screen::Cell::default());
        }
        let current = ContentProjection::build(&normal).unwrap();
        assert_eq!(current.resolve_selection(stale), None);
    }

    #[test]
    fn prefix_eviction_invalidates_evicted_atoms_without_rebasing_survivors() {
        let mutation = AnchorMutation::EvictPrefix {
            line_id: LogicalLineId(7),
            end: LogicalAtomOffset(2),
        };
        assert_eq!(mutation.apply(anchor(1, Affinity::Before)), None);
        assert_eq!(
            mutation.apply(anchor(2, Affinity::Before)),
            Some(anchor(2, Affinity::Before))
        );
        assert_eq!(
            mutation.apply(anchor(5, Affinity::After)),
            Some(anchor(5, Affinity::After))
        );
    }
}
