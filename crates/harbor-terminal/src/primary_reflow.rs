//! Detached primary-screen geometry reflow.
//!
//! Preparation decodes retained logical content, repacks width, establishes the
//! live viewport according to the producer's history policy, evicts capacity
//! overflow, and materializes an owned `NormalBuf` in a fresh generation epoch.

use std::borrow::Borrow;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;

use crate::content_anchor::{Affinity, ContentAnchor, derive_anchor};
use crate::logical_content::{
    self, DecodeError, LogicalAtomOffset, LogicalGlyph, LogicalLine, SourceSpan,
};
use crate::normal_buf::{CellState, LogicalLineId, NormalBuf, RowMetadata};
use crate::screen::Cell;
use crate::selection_model::GenPos;

mod projection;
use projection::ProjectedCell;
pub(crate) use projection::ReflowCell;

type ProjectedRow<'a> = ReflowedRow<ProjectedCell<'a>>;

const MIN_REFLOW_COLS: usize = 2;

/// Which rows the producer includes in its own resize calculations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReflowViewport {
    /// Reclaim retained history when the reflowed content fits.
    PullHistory,
    /// ConPTY has no scrollback: reflow its live rows independently of history.
    /// Final source-row tail styles fit producer geometry without extra
    /// rows; overflow is discarded to match producer-relative redraw coordinates.
    PreserveLiveTop,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ReflowPosition {
    pub(crate) row: usize,
    pub(crate) col: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProjectedInsertion {
    pub(crate) position: GenPos,
    pub(crate) pending_wrap: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PreparedProjection<T> {
    Absent,
    Projected(T),
    Invalid,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReflowedRow<C = Cell> {
    pub(crate) cells: Vec<C>,
    pub(crate) cell_state: Vec<CellState>,
    pub(crate) metadata: RowMetadata,
}

#[cfg(test)]
impl ReflowedRow {
    fn blank(
        cols: usize,
        line_id: LogicalLineId,
        logical_start: usize,
        logical_atom_start: usize,
        soft_wrapped: bool,
        head_truncated: bool,
    ) -> Result<Self, PreparationError> {
        Self::blank_with(
            cols,
            line_id,
            logical_start,
            logical_atom_start,
            soft_wrapped,
            head_truncated,
        )
    }
}

impl<C: Clone + Default> ReflowedRow<C> {
    fn blank_with(
        cols: usize,
        line_id: LogicalLineId,
        logical_start: usize,
        logical_atom_start: usize,
        soft_wrapped: bool,
        head_truncated: bool,
    ) -> Result<Self, PreparationError> {
        let mut cells = Vec::new();
        cells
            .try_reserve_exact(cols)
            .map_err(|_| PreparationError::AllocationFailed)?;
        cells.resize(cols, C::default());
        let mut cell_state = Vec::new();
        cell_state
            .try_reserve_exact(cols)
            .map_err(|_| PreparationError::AllocationFailed)?;
        cell_state.resize(cols, CellState::default());
        Ok(Self {
            cells,
            cell_state,
            metadata: RowMetadata {
                logical_line_id: line_id,
                logical_start,
                logical_atom_start,
                meaningful_extent: 0,
                soft_wrapped,
                head_truncated,
            },
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ReflowedAtom {
    atom_offset: LogicalAtomOffset,
    source_span: SourceSpan,
    // Coordinates before history eviction; retained coordinates are derived on projection.
    position: ReflowPosition,
    end_col: usize,
}

/// Primary reflow keeps ordinary tail spaces through cursor insertion points.
#[derive(Clone, Copy, Debug)]
pub(crate) enum CursorRetention {
    Position(GenPos),
    Anchor(ContentAnchor),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ReflowedLine {
    source_generations: Vec<u64>,
    // Packed-row range before eviction, retained-row range afterward. A zero
    // count means the line was completely evicted, including empty lines.
    first_row: usize,
    row_count: usize,
    atom_start: LogicalAtomOffset,
    atoms: Vec<ReflowedAtom>,
}

impl ReflowedLine {
    fn retained_first_row(&self) -> Option<usize> {
        (self.row_count != 0).then_some(self.first_row)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PreparedPrimaryResize {
    source_cols: usize,
    target_rows: usize,
    cols: usize,
    lines: HashMap<LogicalLineId, ReflowedLine>,
    normal: NormalBuf,
    dropped_rows: usize,
    pub(crate) live_cursor: PreparedProjection<ProjectedInsertion>,
    pub(crate) saved_cursor: PreparedProjection<ProjectedInsertion>,
    pub(crate) review: PreparedProjection<GenPos>,
}

impl PreparedPrimaryResize {
    #[cfg(test)]
    pub(crate) fn prepare_geometry(
        normal: &NormalBuf,
        requested_rows: usize,
        requested_cols: usize,
    ) -> Result<Self, PreparationError> {
        Self::prepare_geometry_with_cursor_floor(normal, requested_rows, requested_cols, None)
    }

    #[cfg(test)]
    pub(crate) fn prepare_geometry_with_cursor_floor(
        normal: &NormalBuf,
        requested_rows: usize,
        requested_cols: usize,
        cursor_floor: Option<usize>,
    ) -> Result<Self, PreparationError> {
        Self::prepare_geometry_with_viewport(
            normal,
            requested_rows,
            requested_cols,
            cursor_floor,
            ReflowViewport::PullHistory,
            &[],
        )
    }

    #[cfg(test)]
    pub(crate) fn prepare_geometry_with_viewport(
        normal: &NormalBuf,
        requested_rows: usize,
        requested_cols: usize,
        cursor_floor: Option<usize>,
        viewport: ReflowViewport,
        retained_cursors: &[CursorRetention],
    ) -> Result<Self, PreparationError> {
        Self::prepare_geometry_with_limits(
            normal,
            requested_rows,
            requested_cols,
            cursor_floor,
            viewport,
            retained_cursors,
            normal.history_limits(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare_geometry_with_limits(
        normal: &NormalBuf,
        requested_rows: usize,
        requested_cols: usize,
        cursor_floor: Option<usize>,
        viewport: ReflowViewport,
        retained_cursors: &[CursorRetention],
        limits: harbor_config::HistoryLimits,
    ) -> Result<Self, PreparationError> {
        let target_rows = requested_rows.max(1);
        let cols = requested_cols.max(MIN_REFLOW_COLS);
        let active_count = normal.active_retained_row_count(cursor_floor);
        let logical_lines = logical_content::project_lines_active(normal, active_count)?;
        // Every full row holds at least floor(cols / 2) complete atoms,
        // even for width-two text. A producer boundary can split one extra row.
        let projected_row_bound = logical_lines
            .iter()
            .try_fold(0usize, |total, line| {
                total.checked_add(line.glyphs.len().div_ceil(cols / 2).max(1))
            })
            .and_then(|bound| {
                bound.checked_add(usize::from(viewport == ReflowViewport::PreserveLiveTop))
            })
            .ok_or(PreparationError::ArithmeticOverflow)?;
        let mut rows = Vec::new();
        rows.try_reserve_exact(projected_row_bound)
            .map_err(|_| PreparationError::AllocationFailed)?;
        let mut lines = HashMap::new();
        lines
            .try_reserve(logical_lines.len())
            .map_err(|_| PreparationError::AllocationFailed)?;

        let source_live_top = normal
            .history_start()
            .checked_add(
                u64::try_from(normal.scroll_count())
                    .map_err(|_| PreparationError::ArithmeticOverflow)?,
            )
            .ok_or(PreparationError::ArithmeticOverflow)?;
        let live_boundary =
            (viewport == ReflowViewport::PreserveLiveTop).then_some(source_live_top);

        for mut line in logical_lines {
            let first_row = rows.len();
            let atoms = pack_line(&mut line, cols, live_boundary, retained_cursors, &mut rows)?;
            // ConPTY's first live row can belong to a history-spanning logical
            // line with no retained atoms in that row. Keep its empty physical
            // boundary row so history cannot be pulled into the producer viewport.
            if live_boundary.is_some_and(|top| {
                line.generations.contains(&top)
                    && line.generations.iter().any(|&generation| generation < top)
                    && atoms.iter().all(|atom| atom.source_span.generation < top)
            }) {
                let last = rows.last().ok_or(PreparationError::Invariant(
                    "missing history row at live boundary",
                ))?;
                let logical_start = last
                    .metadata
                    .logical_start
                    .checked_add(last.metadata.meaningful_extent)
                    .ok_or(PreparationError::ArithmeticOverflow)?;
                let atom_start = line
                    .atom_start
                    .0
                    .checked_add(atoms.len())
                    .ok_or(PreparationError::ArithmeticOverflow)?;
                rows.try_reserve(1)
                    .map_err(|_| PreparationError::AllocationFailed)?;
                rows.push(ProjectedRow::blank_with(
                    cols,
                    line.line_id,
                    logical_start,
                    atom_start,
                    true,
                    false,
                )?);
            }
            let line_id = line.line_id;
            let atom_start = line.atom_start;
            let source_generations = line.generations;
            let row_count = rows
                .len()
                .checked_sub(first_row)
                .ok_or(PreparationError::ArithmeticOverflow)?;
            if lines
                .insert(
                    line_id,
                    ReflowedLine {
                        source_generations,
                        first_row,
                        row_count,
                        atom_start,
                        atoms,
                    },
                )
                .is_some()
            {
                return Err(PreparationError::Invariant(
                    "logical line identity was reused",
                ));
            }
        }

        // Preserve ConPTY's live/history boundary even when it splits a logical
        // line. Pulling history into newly available space would put our cursor
        // on a different row from the producer's next absolute cursor command.
        let live_top = if viewport == ReflowViewport::PreserveLiveTop {
            lines
                .values()
                .find_map(|line| {
                    line.source_generations.contains(&source_live_top).then(|| {
                        line.atoms
                            .iter()
                            .find(|atom| atom.source_span.generation >= source_live_top)
                            // No retained live atom: the final row is the empty
                            // boundary row, not the historical start of the line.
                            .map_or(line.first_row + line.row_count - 1, |atom| {
                                atom.position.row
                            })
                    })
                })
                .ok_or(PreparationError::Invariant(
                    "source live viewport is missing",
                ))?
        } else {
            0
        };
        // Only unwritten rows below active content are filled; history remains
        // available for scrolling, selection and copy in either policy.
        let blank_count = target_rows.saturating_sub(rows.len() - live_top);
        rows.try_reserve_exact(blank_count)
            .map_err(|_| PreparationError::AllocationFailed)?;
        lines
            .try_reserve(blank_count)
            .map_err(|_| PreparationError::AllocationFailed)?;
        let mut next_logical_line_id = normal.next_logical_line_id();
        let filled_rows = rows
            .len()
            .checked_add(blank_count)
            .ok_or(PreparationError::ArithmeticOverflow)?;
        while rows.len() < filled_rows {
            let line_id = LogicalLineId(next_logical_line_id);
            next_logical_line_id = next_logical_line_id
                .checked_add(1)
                .ok_or(PreparationError::ArithmeticOverflow)?;
            let first_row = rows.len();
            rows.push(ProjectedRow::blank_with(cols, line_id, 0, 0, false, false)?);
            lines.insert(
                line_id,
                ReflowedLine {
                    source_generations: Vec::new(),
                    first_row,
                    row_count: 1,
                    atom_start: LogicalAtomOffset(0),
                    atoms: Vec::new(),
                },
            );
        }

        let dropped_rows = evict_history_overflow(&mut rows, &mut lines, target_rows, limits)?;

        let generation_base = normal
            .newest_generation_exclusive()
            .ok_or(PreparationError::ArithmeticOverflow)?;
        let final_generation_count =
            u64::try_from(rows.len()).map_err(|_| PreparationError::ArithmeticOverflow)?;
        generation_base
            .checked_add(final_generation_count)
            .ok_or(PreparationError::ArithmeticOverflow)?;
        let detached = NormalBuf::from_reflowed_rows(
            &rows,
            target_rows,
            cols,
            limits,
            next_logical_line_id,
            generation_base,
            0,
        )?;

        let prepared = Self {
            source_cols: normal.cols(),
            target_rows,
            cols,
            lines,
            normal: detached,
            dropped_rows,
            live_cursor: PreparedProjection::Invalid,
            saved_cursor: PreparedProjection::Absent,
            review: PreparedProjection::Absent,
        };
        // Keep validation after detached-ring construction so preparation errors
        // retain their original precedence, then discard the read-only projections.
        prepared.validate(rows.len())?;
        Ok(prepared)
    }

    #[cfg(test)]
    pub(crate) fn cols(&self) -> usize {
        self.cols
    }

    #[cfg(test)]
    pub(crate) fn rows(&self) -> Vec<crate::normal_buf::RetainedRow<'_>> {
        self.normal.retained_rows().collect()
    }

    #[cfg(test)]
    pub(crate) fn target_rows(&self) -> usize {
        self.target_rows
    }

    #[cfg(test)]
    pub(crate) fn dropped_rows(&self) -> usize {
        self.dropped_rows
    }

    pub(crate) fn normal(&self) -> &NormalBuf {
        &self.normal
    }

    pub(crate) fn into_screen_parts(
        self,
    ) -> (
        NormalBuf,
        PreparedProjection<ProjectedInsertion>,
        PreparedProjection<ProjectedInsertion>,
        PreparedProjection<GenPos>,
    ) {
        (
            self.normal,
            self.live_cursor,
            self.saved_cursor,
            self.review,
        )
    }

    pub(crate) fn source_anchor(
        &self,
        position: GenPos,
        affinity: Affinity,
    ) -> Option<ContentAnchor> {
        let (line_id, line) = self
            .lines
            .iter()
            .find(|(_, line)| line.source_generations.contains(&position.generation))?;
        derive_anchor(
            *line_id,
            line.atom_start,
            self.source_cols,
            position,
            affinity,
            line.atoms.len(),
            |index| {
                let atom = &line.atoms[index];
                (atom.atom_offset, atom.source_span)
            },
        )
    }

    pub(crate) fn source_cursor_anchor(
        &self,
        position: GenPos,
        pending_wrap: bool,
    ) -> Option<ContentAnchor> {
        // Pending wrap is the insertion point after the cell under the cursor,
        // which can be on an earlier physical row of a longer logical line.
        self.source_anchor(
            position,
            if pending_wrap {
                Affinity::After
            } else {
                Affinity::Before
            },
        )
    }

    pub(crate) fn project_selection(&self, anchor: ContentAnchor) -> Option<GenPos> {
        let line = self.lines.get(&anchor.line_id)?;
        if line.atoms.is_empty() {
            let row = line.retained_first_row()?;
            return self.final_gen_pos(ReflowPosition {
                row,
                col: anchor
                    .projection_hint_col()
                    .unwrap_or(0)
                    .min(self.cols.saturating_sub(1)),
            });
        }
        if anchor.prefers_previous_projection() {
            let previous_offset = anchor.offset.0.checked_sub(1)?;
            if let Some(atom) = line
                .atoms
                .iter()
                .find(|atom| atom.atom_offset.0 == previous_offset)
            {
                let position = self.retained_atom_position(atom)?;
                return self.final_gen_pos(ReflowPosition {
                    row: position.row,
                    col: atom.end_col,
                });
            }
            return line
                .retained_first_row()
                .and_then(|row| self.final_gen_pos(ReflowPosition { row, col: 0 }));
        }
        if let Some(atom) = line
            .atoms
            .iter()
            .find(|atom| atom.atom_offset == anchor.offset)
        {
            let position = self.retained_atom_position(atom)?;
            return self.final_gen_pos(match anchor.affinity {
                Affinity::Before => position,
                Affinity::After => ReflowPosition {
                    row: position.row,
                    col: atom.end_col,
                },
            });
        }
        let end_offset = line.atom_start.0.checked_add(line.atoms.len())?;
        if anchor.offset.0 != end_offset {
            return None;
        }
        let atom = line
            .atoms
            .iter()
            .rev()
            .find(|atom| atom.position.row >= self.dropped_rows)?;
        let position = self.retained_atom_position(atom)?;
        self.final_gen_pos(match anchor.affinity {
            Affinity::Before => ReflowPosition {
                row: position.row,
                col: atom.end_col,
            },
            Affinity::After => ReflowPosition {
                row: line.retained_first_row()? + line.row_count.saturating_sub(1),
                col: self.cols.saturating_sub(1),
            },
        })
    }

    pub(crate) fn project_cursor(&self, anchor: ContentAnchor) -> Option<ProjectedInsertion> {
        let line = self.lines.get(&anchor.line_id)?;
        if line.atoms.is_empty() {
            let position = self.final_gen_pos(ReflowPosition {
                row: line.retained_first_row()?,
                col: anchor
                    .projection_hint_col()
                    .unwrap_or(0)
                    .min(self.cols.saturating_sub(1)),
            })?;
            return Some(ProjectedInsertion {
                position,
                pending_wrap: false,
            });
        }
        if anchor.prefers_previous_projection() {
            let previous_offset = anchor.offset.0.checked_sub(1)?;
            if let Some(atom) = line
                .atoms
                .iter()
                .find(|atom| atom.atom_offset.0 == previous_offset)
            {
                return self.insertion_after(atom);
            }
            let position = self.final_gen_pos(ReflowPosition {
                row: line.retained_first_row()?,
                col: 0,
            })?;
            return Some(ProjectedInsertion {
                position,
                pending_wrap: false,
            });
        }
        if let Some(atom) = line
            .atoms
            .iter()
            .find(|atom| atom.atom_offset == anchor.offset)
        {
            return match anchor.affinity {
                Affinity::Before => Some(ProjectedInsertion {
                    position: self.final_gen_pos(self.retained_atom_position(atom)?)?,
                    pending_wrap: false,
                }),
                Affinity::After => self.insertion_after(atom),
            };
        }
        let end_offset = line.atom_start.0.checked_add(line.atoms.len())?;
        if anchor.offset.0 == end_offset {
            let atom = line
                .atoms
                .iter()
                .rev()
                .find(|atom| atom.position.row >= self.dropped_rows)?;
            let mut insertion = self.insertion_after(atom)?;
            // An insertion point beyond the last written atom can be parked in
            // unwritten cells; keep that gap when the target row has room.
            if let Some(hint) = anchor.projection_hint()
                && hint.generation == atom.source_span.generation
                && let Some(source_after) = atom.source_span.end_col.checked_add(1)
                && hint.col > source_after
            {
                let gap = hint.col - source_after;
                insertion.position.col = insertion
                    .position
                    .col
                    .saturating_add(gap)
                    .min(self.cols.saturating_sub(1));
                insertion.pending_wrap = false;
            }
            return Some(insertion);
        }
        None
    }

    pub(crate) fn attach_screen_anchors(
        mut self,
        live_cursor: ContentAnchor,
        saved_cursor: PreparedProjection<ContentAnchor>,
        review: PreparedProjection<ContentAnchor>,
    ) -> Result<Self, PreparationError> {
        let live_cursor = self
            .project_cursor(live_cursor)
            .ok_or(PreparationError::UnresolvedLiveCursor)?;
        let history_rows = u64::try_from(self.normal.scroll_count())
            .map_err(|_| PreparationError::ArithmeticOverflow)?;
        let live_top = self
            .normal
            .history_start()
            .checked_add(history_rows)
            .ok_or(PreparationError::ArithmeticOverflow)?;
        let target_rows =
            u64::try_from(self.target_rows).map_err(|_| PreparationError::ArithmeticOverflow)?;
        let live_end = live_top
            .checked_add(target_rows)
            .ok_or(PreparationError::ArithmeticOverflow)?;
        let is_live =
            |position: GenPos| position.generation >= live_top && position.generation < live_end;
        if !is_live(live_cursor.position) {
            return Err(PreparationError::UnresolvedLiveCursor);
        }
        self.live_cursor = PreparedProjection::Projected(live_cursor);
        self.saved_cursor = match saved_cursor {
            PreparedProjection::Projected(anchor) => self
                .project_cursor(anchor)
                .filter(|projection| is_live(projection.position))
                .map_or(PreparedProjection::Invalid, PreparedProjection::Projected),
            PreparedProjection::Absent => PreparedProjection::Absent,
            PreparedProjection::Invalid => PreparedProjection::Invalid,
        };
        self.review = match review {
            PreparedProjection::Projected(anchor) => self.project_selection(anchor).map_or_else(
                || self.oldest_retained_projection(),
                PreparedProjection::Projected,
            ),
            PreparedProjection::Absent => PreparedProjection::Absent,
            PreparedProjection::Invalid => self.oldest_retained_projection(),
        };
        if matches!(
            self.review,
            PreparedProjection::Projected(position) if position.generation >= live_top
        ) {
            self.review = PreparedProjection::Projected(GenPos::new(live_top, 0));
        }
        let view_offset = match self.review {
            PreparedProjection::Projected(position) if position.generation < live_top => {
                usize::try_from(live_top - position.generation)
                    .map_err(|_| PreparationError::ArithmeticOverflow)?
            }
            _ => 0,
        };
        self.normal.set_view_offset(view_offset);
        Ok(self)
    }

    fn oldest_retained_projection(&self) -> PreparedProjection<GenPos> {
        PreparedProjection::Projected(GenPos::new(self.normal.history_start(), 0))
    }

    fn final_gen_pos(&self, position: ReflowPosition) -> Option<GenPos> {
        if position.row >= self.normal.retained_rows().len() || position.col >= self.cols {
            return None;
        }
        let row = u64::try_from(position.row).ok()?;
        Some(GenPos::new(
            self.normal.history_start().checked_add(row)?,
            position.col,
        ))
    }

    fn retained_atom_position(&self, atom: &ReflowedAtom) -> Option<ReflowPosition> {
        Some(ReflowPosition {
            row: atom.position.row.checked_sub(self.dropped_rows)?,
            col: atom.position.col,
        })
    }

    fn insertion_after(&self, atom: &ReflowedAtom) -> Option<ProjectedInsertion> {
        let position = self.retained_atom_position(atom)?;
        let insertion_col = atom.end_col.saturating_add(1);
        Some(ProjectedInsertion {
            position: self.final_gen_pos(ReflowPosition {
                row: position.row,
                col: insertion_col.min(self.cols.saturating_sub(1)),
            })?,
            pending_wrap: insertion_col >= self.cols,
        })
    }

    fn validate(&self, row_count: usize) -> Result<(), PreparationError> {
        if self.normal.rows() != self.target_rows
            || self.normal.cols() != self.cols
            || self.normal.scroll_count().checked_add(self.target_rows) != Some(row_count)
        {
            return Err(PreparationError::Invariant(
                "detached ring placement disagrees with prepared rows",
            ));
        }
        let mut previous: Option<(RowMetadata, usize)> = None;
        for row in self.normal.retained_rows() {
            if row.cells.len() != self.cols || row.cell_state.len() != self.cols {
                return Err(PreparationError::Invariant("projected row width mismatch"));
            }
            if row.metadata.meaningful_extent > self.cols {
                return Err(PreparationError::Invariant(
                    "projected row extent exceeds width",
                ));
            }
            let mut col = 0usize;
            let mut atom_count = 0usize;
            while col < row.metadata.meaningful_extent {
                let cell = &row.cells[col];
                if cell.wide_continuation {
                    return Err(PreparationError::Invariant("orphan projected continuation"));
                }
                atom_count = atom_count
                    .checked_add(1)
                    .ok_or(PreparationError::ArithmeticOverflow)?;
                if cell.grid_width() == 2 {
                    if col + 1 >= row.metadata.meaningful_extent
                        || !row.cells[col + 1].wide_continuation
                    {
                        return Err(PreparationError::Invariant("split projected wide glyph"));
                    }
                    col += 2;
                } else {
                    col += 1;
                }
            }
            if row.cell_state[row.metadata.meaningful_extent..]
                .iter()
                .any(|state| state.is_meaningful())
            {
                return Err(PreparationError::Invariant(
                    "generated padding became meaningful",
                ));
            }
            if let Some((prior, prior_atom_count)) = previous {
                if row.metadata.soft_wrapped {
                    let expected_cell_start = prior
                        .logical_start
                        .checked_add(prior.meaningful_extent)
                        .ok_or(PreparationError::ArithmeticOverflow)?;
                    let expected_atom_start = prior
                        .logical_atom_start
                        .checked_add(prior_atom_count)
                        .ok_or(PreparationError::ArithmeticOverflow)?;
                    if row.metadata.logical_line_id != prior.logical_line_id
                        || row.metadata.logical_start != expected_cell_start
                        || row.metadata.logical_atom_start != expected_atom_start
                    {
                        return Err(PreparationError::Invariant(
                            "projected continuation metadata is inconsistent",
                        ));
                    }
                }
            } else if row.metadata.soft_wrapped && !row.metadata.head_truncated {
                return Err(PreparationError::Invariant(
                    "projected leading continuation is not truncated",
                ));
            }
            previous = Some((row.metadata, atom_count));
        }
        Ok(())
    }
}

fn cursor_retained_glyph_count<C>(line: &LogicalLine<C>, cursors: &[CursorRetention]) -> usize {
    // Keep the atom under each cursor as well as its preceding spaces, so a
    // "before" anchor still has an on-grid position after narrowing.
    cursors.iter().fold(0, |end, cursor| {
        let needed = match cursor {
            CursorRetention::Position(position)
                if line.generations.contains(&position.generation) =>
            {
                line.glyphs
                    .iter()
                    .position(|logical| {
                        logical.source_span.generation > position.generation
                            || (logical.source_span.generation == position.generation
                                && logical.source_span.end_col >= position.col)
                    })
                    .map_or(line.glyphs.len(), |index| index + 1)
            }
            CursorRetention::Anchor(anchor) if anchor.line_id == line.line_id => anchor
                .offset
                .0
                .saturating_sub(line.atom_start.0)
                .saturating_add(1)
                .min(line.glyphs.len()),
            _ => 0,
        };
        end.max(needed)
    })
}

/// Drops history rows beyond the row/byte budget and rebases retained line evidence.
///
/// Only rows before the live viewport are candidates; a cut that splits a logical
/// line marks the surviving head truncated instead of dropping the whole line.
fn evict_history_overflow<C: ReflowCell>(
    rows: &mut Vec<ReflowedRow<C>>,
    lines: &mut HashMap<LogicalLineId, ReflowedLine>,
    target_rows: usize,
    limits: harbor_config::HistoryLimits,
) -> Result<usize, PreparationError> {
    let budget = limits
        .max_rows
        .checked_add(target_rows)
        .ok_or(PreparationError::ArithmeticOverflow)?;
    let mut dropped_rows = rows.len().saturating_sub(budget);
    let history_end = rows.len().saturating_sub(target_rows);
    let mut bytes: usize = rows[dropped_rows..history_end]
        .iter()
        .map(|row| NormalBuf::row_source_bytes(&row.cells, row.metadata.meaningful_extent))
        .sum();
    while bytes > limits.max_bytes && dropped_rows < history_end {
        let row = &rows[dropped_rows];
        bytes -= NormalBuf::row_source_bytes(&row.cells, row.metadata.meaningful_extent);
        dropped_rows += 1;
    }

    if dropped_rows > rows.len().saturating_sub(target_rows) {
        return Err(PreparationError::Invariant(
            "capacity eviction entered live viewport",
        ));
    }
    let partial_head = dropped_rows > 0
        && dropped_rows < rows.len()
        && rows[dropped_rows - 1].metadata.logical_line_id
            == rows[dropped_rows].metadata.logical_line_id;
    if dropped_rows > 0 {
        rows.drain(0..dropped_rows);
    }
    if partial_head {
        let head = rows
            .first_mut()
            .ok_or(PreparationError::Invariant("capacity removed every row"))?;
        head.metadata.head_truncated = true;
    }

    for line in lines.values_mut() {
        let end = line
            .first_row
            .checked_add(line.row_count)
            .ok_or(PreparationError::ArithmeticOverflow)?;
        let retained_start = line.first_row.max(dropped_rows);
        if retained_start < end {
            line.first_row = retained_start - dropped_rows;
            line.row_count = end - retained_start;
        } else {
            line.first_row = 0;
            line.row_count = 0;
        }
    }
    Ok(dropped_rows)
}

fn pack_line<'a>(
    line: &mut LogicalLine<&'a Cell>,
    cols: usize,
    live_boundary: Option<u64>,
    retained_cursors: &[CursorRetention],
    output: &mut Vec<ProjectedRow<'a>>,
) -> Result<Vec<ReflowedAtom>, PreparationError> {
    let mut logical_start = line.cell_start;
    let mut logical_atom_start = line.atom_start.0;
    let mut row = ProjectedRow::blank_with(
        cols,
        line.line_id,
        logical_start,
        logical_atom_start,
        line.head_truncated,
        line.head_truncated,
    )?;
    let mut atoms = Vec::new();
    // Only explicit, undecorated spaces at the *logical* end are disposable.
    let visible_end = line
        .glyphs
        .iter()
        .rposition(|logical| {
            logical.glyph.cell != &Cell::default() || !logical.glyph.cell_state.is_explicit()
        })
        .map_or(0, |index| index + 1);
    let cursor_floor = cursor_retained_glyph_count(line, retained_cursors);
    let retained_count = visible_end.max(cursor_floor);
    // The full retained extent still owns the styles we can paint. ConPTY's
    // live geometry ends at text/cursors/already-wrapped prefixes, not styles.
    // History remains fully reflowed and the live-top split remains explicit.
    let geometry_count = live_boundary.map_or(retained_count, |top| {
        // An already soft-wrapped source prefix is producer geometry even if
        // its last cells are spaces. Only the FINAL source row has a trimmable
        // tail; erasing the prefix's footprint disagrees with ConPTY reflow.
        let last_source_row = line.generations.last().copied();
        let geometry_end = line
            .glyphs
            .iter()
            .rposition(|logical| {
                let cell = &logical.glyph.cell;
                logical.source_span.generation < top
                    || last_source_row.is_some_and(|last| logical.source_span.generation < last)
                    || cell.ch != ' '
                    || !cell.suffix.is_empty()
                    || cell.protected
                    || cell.hyperlink.is_some()
            })
            .map_or(0, |index| index + 1);
        geometry_end.max(cursor_floor)
    });
    // A wrapForced prefix can contain ordinary blanks which the generic
    // retention policy trims. Keep that producer footprint in live reflow too.
    let retained_count = retained_count.max(geometry_count);
    atoms
        .try_reserve_exact(retained_count)
        .map_err(|_| PreparationError::AllocationFailed)?;

    let mut glyphs = line.glyphs.drain(..retained_count);
    while let Some(logical) = glyphs.next() {
        let expected_offset = line
            .atom_start
            .0
            .checked_add(atoms.len())
            .ok_or(PreparationError::ArithmeticOverflow)?;
        if logical.atom_offset.0 != expected_offset {
            return Err(PreparationError::Invariant(
                "non-monotonic logical atom offset",
            ));
        }
        let glyph = &logical.glyph;
        validate_glyph(glyph)?;
        let width = usize::from(glyph.width);
        let used = row.metadata.meaningful_extent;
        let remaining = cols
            .checked_sub(used)
            .ok_or(PreparationError::ArithmeticOverflow)?;
        let starts_live_viewport = live_boundary.is_some_and(|top| {
            logical.source_span.generation == top
                && atoms
                    .last()
                    .is_some_and(|atom: &ReflowedAtom| atom.source_span.generation < top)
        });
        // Keep trailing styles only while they fit. Never wrap just to paint
        // a suffix ConPTY excludes from its cooked-input resize geometry.
        // A history/live boundary still starts a distinct producer row.
        if atoms.len() >= geometry_count && width > remaining && !starts_live_viewport {
            break;
        }
        if width > remaining || starts_live_viewport {
            logical_start = logical_start
                .checked_add(row.metadata.meaningful_extent)
                .ok_or(PreparationError::ArithmeticOverflow)?;
            logical_atom_start = logical.atom_offset.0;
            output.push(row);
            row = ProjectedRow::blank_with(
                cols,
                line.line_id,
                logical_start,
                logical_atom_start,
                true,
                false,
            )?;
        }

        // Only ordinary single-column scalars participate. The run ends at a
        // source-row boundary (hence never crosses the ConPTY live boundary),
        // a target edge, or a complex glyph. Cursor-bearing lines stay scalar.
        if cursor_floor == 0 && is_simple_glyph(glyph) {
            let run_len = 1 + glyphs
                .as_slice()
                .iter()
                .take(cols - row.metadata.meaningful_extent - 1)
                .take_while(|next| {
                    next.source_span.generation == logical.source_span.generation
                        && is_simple_glyph(&next.glyph)
                })
                .count();
            let col = row.metadata.meaningful_extent;
            // Prove the largest offset once before the unchecked inner additions.
            expected_offset
                .checked_add(run_len - 1)
                .ok_or(PreparationError::ArithmeticOverflow)?;
            for (index, next) in std::iter::once(logical)
                .chain(glyphs.by_ref().take(run_len - 1))
                .enumerate()
            {
                if next.atom_offset.0 != expected_offset + index {
                    return Err(PreparationError::Invariant(
                        "non-monotonic logical atom offset",
                    ));
                }
                row.cells[col + index] = ProjectedCell::Source(next.glyph.cell);
                row.cell_state[col + index] = next.glyph.cell_state;
                atoms.push(ReflowedAtom {
                    atom_offset: next.atom_offset,
                    source_span: next.source_span,
                    position: ReflowPosition {
                        row: output.len(),
                        col: col + index,
                    },
                    end_col: col + index,
                });
            }
            row.metadata.meaningful_extent += run_len;
            continue;
        }

        let col = row.metadata.meaningful_extent;
        row.cell_state[col] = glyph.cell_state;
        if width == 2 {
            row.cells[col + 1] = ProjectedCell::Continuation(glyph.cell);
            row.cell_state[col + 1] = glyph
                .continuation_state
                .ok_or(PreparationError::Invariant("wide glyph provenance missing"))?;
        }
        row.cells[col] = ProjectedCell::Source(logical.glyph.cell);
        row.metadata.meaningful_extent = col
            .checked_add(width)
            .ok_or(PreparationError::ArithmeticOverflow)?;
        let position = ReflowPosition {
            row: output.len(),
            col,
        };
        atoms.push(ReflowedAtom {
            atom_offset: logical.atom_offset,
            source_span: logical.source_span,
            position,
            end_col: col + width - 1,
        });
    }

    output.push(row);
    Ok(atoms)
}

/// ASCII gives a cheap, exact width proof; style is copied without reinterpreting
/// it. Complex text, protected cells and hyperlink identities stay on the scalar
/// path. Also retain the scalar validator's blank-provenance check.
fn is_simple_glyph<C: Borrow<Cell>>(glyph: &LogicalGlyph<C>) -> bool {
    let cell = glyph.cell.borrow();
    glyph.width == 1
        && cell.width == 1
        && cell.ch.is_ascii()
        && !cell.wide_continuation
        && !cell.isolated_mark
        && cell.suffix.is_empty()
        && !cell.protected
        && cell.hyperlink.is_none()
        && glyph.meaningful_blank == (cell.ch == ' ' && glyph.cell_state.is_meaningful())
}

fn validate_glyph<C: Borrow<Cell>>(glyph: &LogicalGlyph<C>) -> Result<(), PreparationError> {
    let cell = glyph.cell.borrow();
    if !matches!(glyph.width, 1 | 2) {
        return Err(PreparationError::Invariant("unsupported glyph width"));
    }
    if glyph.width != cell.grid_width() {
        return Err(PreparationError::Invariant(
            "glyph width disagrees with payload",
        ));
    }
    if glyph.meaningful_blank != (cell.ch == ' ' && glyph.cell_state.is_meaningful()) {
        return Err(PreparationError::Invariant(
            "blank provenance disagrees with payload",
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PreparationError {
    Decode(DecodeError),
    ArithmeticOverflow,
    AllocationFailed,
    Invariant(&'static str),
    UnresolvedLiveCursor,
}

impl From<DecodeError> for PreparationError {
    fn from(error: DecodeError) -> Self {
        Self::Decode(error)
    }
}

impl fmt::Display for PreparationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(error) => error.fmt(formatter),
            Self::ArithmeticOverflow => {
                formatter.write_str("primary resize preparation arithmetic overflow")
            }
            Self::AllocationFailed => {
                formatter.write_str("primary resize preparation allocation failed")
            }
            Self::Invariant(message) => {
                write!(formatter, "primary resize preparation invariant: {message}")
            }
            Self::UnresolvedLiveCursor => formatter.write_str("live cursor anchor did not resolve"),
        }
    }
}

impl Error for PreparationError {}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod perf;

#[cfg(test)]
mod reference;

#[cfg(test)]
pub(crate) mod allocation_probe;

#[cfg(test)]
mod fast_path_tests;
