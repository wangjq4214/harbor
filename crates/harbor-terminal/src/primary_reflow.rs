//! Detached primary-screen geometry reflow.
//!
//! Preparation decodes retained logical content, repacks width, establishes the
//! target-height live suffix, evicts exact capacity overflow, assigns a fresh
//! generation epoch, and materializes an owned `NormalBuf` without mutating the source.

use std::collections::HashMap;
use std::error::Error;
use std::fmt;

use unicode_width::UnicodeWidthChar;

use crate::content_anchor::{Affinity, ContentAnchor};
use crate::logical_content::{
    self, DecodeError, LogicalAtomOffset, LogicalGlyph, LogicalLine, SourceSpan,
};
use crate::normal_buf::{CellState, LogicalLineId, NormalBuf, RowMetadata};
use crate::screen::Cell;
use crate::selection_model::GenPos;

const MIN_REFLOW_COLS: usize = 2;

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
pub(crate) struct ReflowedRow {
    pub(crate) cells: Vec<Cell>,
    pub(crate) cell_state: Vec<CellState>,
    pub(crate) metadata: RowMetadata,
}

impl ReflowedRow {
    fn blank(
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
        cells.resize(cols, Cell::default());
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
    source_position: ReflowPosition,
    position: Option<ReflowPosition>,
    end_col: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ReflowedLine {
    source_generations: Vec<u64>,
    first_row: usize,
    row_count: usize,
    retained_first_row: Option<usize>,
    retained_row_count: usize,
    atom_start: LogicalAtomOffset,
    atoms: Vec<ReflowedAtom>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PreparedPrimaryResize {
    source_cols: usize,
    target_rows: usize,
    cols: usize,
    rows: Vec<ReflowedRow>,
    lines: HashMap<LogicalLineId, ReflowedLine>,
    normal: NormalBuf,
    dropped_rows: usize,
    pub(crate) live_cursor: PreparedProjection<ProjectedInsertion>,
    pub(crate) saved_cursor: PreparedProjection<ProjectedInsertion>,
    pub(crate) review: PreparedProjection<GenPos>,
}

#[cfg(test)]
pub(crate) type PreparedPrimaryWidthReflow = PreparedPrimaryResize;

impl PreparedPrimaryResize {
    pub(crate) fn prepare_geometry(
        normal: &NormalBuf,
        requested_rows: usize,
        requested_cols: usize,
    ) -> Result<Self, PreparationError> {
        let target_rows = requested_rows.max(1);
        let cols = requested_cols.max(MIN_REFLOW_COLS);
        let logical_lines = logical_content::decode_lines(normal)?;
        let projected_row_bound = logical_lines
            .iter()
            .try_fold(0usize, |total, line| {
                total.checked_add(line.glyphs.len().max(1))
            })
            .ok_or(PreparationError::ArithmeticOverflow)?;
        let mut rows = Vec::new();
        rows.try_reserve_exact(projected_row_bound)
            .map_err(|_| PreparationError::AllocationFailed)?;
        let mut lines = HashMap::new();
        lines
            .try_reserve(logical_lines.len())
            .map_err(|_| PreparationError::AllocationFailed)?;

        for line in logical_lines {
            let first_row = rows.len();
            let atoms = pack_line(&line, cols, &mut rows)?;
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
                        retained_first_row: Some(first_row),
                        retained_row_count: row_count,
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

        let blank_count = target_rows.saturating_sub(rows.len());
        rows.try_reserve_exact(blank_count)
            .map_err(|_| PreparationError::AllocationFailed)?;
        lines
            .try_reserve(blank_count)
            .map_err(|_| PreparationError::AllocationFailed)?;
        let mut next_logical_line_id = normal.next_logical_line_id();
        while rows.len() < target_rows {
            let line_id = LogicalLineId(next_logical_line_id);
            next_logical_line_id = next_logical_line_id
                .checked_add(1)
                .ok_or(PreparationError::ArithmeticOverflow)?;
            let first_row = rows.len();
            rows.push(ReflowedRow::blank(cols, line_id, 0, 0, false, false)?);
            lines.insert(
                line_id,
                ReflowedLine {
                    source_generations: Vec::new(),
                    first_row,
                    row_count: 1,
                    retained_first_row: Some(first_row),
                    retained_row_count: 1,
                    atom_start: LogicalAtomOffset(0),
                    atoms: Vec::new(),
                },
            );
        }

        let budget = normal
            .max_scrollback()
            .checked_add(target_rows)
            .ok_or(PreparationError::ArithmeticOverflow)?;
        let dropped_rows = rows.len().saturating_sub(budget);
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
                line.retained_first_row = Some(retained_start - dropped_rows);
                line.retained_row_count = end - retained_start;
            } else {
                line.retained_first_row = None;
                line.retained_row_count = 0;
            }
            for atom in &mut line.atoms {
                atom.position =
                    (atom.source_position.row >= dropped_rows).then(|| ReflowPosition {
                        row: atom.source_position.row - dropped_rows,
                        col: atom.source_position.col,
                    });
            }
        }

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
            normal.max_scrollback(),
            next_logical_line_id,
            generation_base,
            0,
        )?;

        let prepared = Self {
            source_cols: normal.cols(),
            target_rows,
            cols,
            rows,
            lines,
            normal: detached,
            dropped_rows,
            live_cursor: PreparedProjection::Invalid,
            saved_cursor: PreparedProjection::Absent,
            review: PreparedProjection::Absent,
        };
        prepared.validate()?;
        Ok(prepared)
    }

    #[allow(dead_code)]
    pub(crate) fn cols(&self) -> usize {
        self.cols
    }

    #[allow(dead_code)]
    pub(crate) fn rows(&self) -> &[ReflowedRow] {
        &self.rows
    }

    #[allow(dead_code)]
    pub(crate) fn target_rows(&self) -> usize {
        self.target_rows
    }

    #[allow(dead_code)]
    pub(crate) fn dropped_rows(&self) -> usize {
        self.dropped_rows
    }

    #[allow(dead_code)]
    pub(crate) fn normal(&self) -> &NormalBuf {
        &self.normal
    }

    #[allow(dead_code)]
    pub(crate) fn into_normal(self) -> NormalBuf {
        self.normal
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
        if position.col >= self.source_cols {
            return None;
        }
        let (line_id, line) = self
            .lines
            .iter()
            .find(|(_, line)| line.source_generations.contains(&position.generation))?;
        let covering = line.atoms.iter().find(|atom| {
            atom.source_span.generation == position.generation
                && position.col >= atom.source_span.start_col
                && position.col <= atom.source_span.end_col
        });
        let (offset, projection_hint, prefer_previous_projection) = if let Some(atom) = covering {
            (atom.atom_offset, None, false)
        } else {
            let next = line.atoms.iter().find(|atom| {
                atom.source_span.generation > position.generation
                    || (atom.source_span.generation == position.generation
                        && atom.source_span.start_col > position.col)
            });
            let end_offset = line.atom_start.0.checked_add(line.atoms.len())?;
            (
                next.map_or(LogicalAtomOffset(end_offset), |atom| atom.atom_offset),
                Some(position),
                next.is_some(),
            )
        };
        Some(ContentAnchor::from_logical_parts(
            *line_id,
            offset,
            affinity,
            projection_hint,
            prefer_previous_projection,
        ))
    }

    pub(crate) fn source_cursor_anchor(
        &self,
        position: GenPos,
        pending_wrap: bool,
    ) -> Option<ContentAnchor> {
        let (line_id, line) = self
            .lines
            .iter()
            .find(|(_, line)| line.source_generations.contains(&position.generation))?;
        if pending_wrap {
            let offset = line
                .atoms
                .last()
                .map_or(line.atom_start, |atom| atom.atom_offset);
            return Some(ContentAnchor::from_logical_parts(
                *line_id,
                offset,
                Affinity::After,
                None,
                false,
            ));
        }
        self.source_anchor(position, Affinity::Before)
    }

    pub(crate) fn project_selection(&self, anchor: ContentAnchor) -> Option<GenPos> {
        let line = self.lines.get(&anchor.line_id)?;
        if line.atoms.is_empty() {
            let row = line.retained_first_row?;
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
                let position = atom.position?;
                return self.final_gen_pos(ReflowPosition {
                    row: position.row,
                    col: atom.end_col,
                });
            }
            return line
                .retained_first_row
                .and_then(|row| self.final_gen_pos(ReflowPosition { row, col: 0 }));
        }
        if let Some(atom) = line
            .atoms
            .iter()
            .find(|atom| atom.atom_offset == anchor.offset)
        {
            let position = atom.position?;
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
            .find(|atom| atom.position.is_some())?;
        let position = atom.position?;
        self.final_gen_pos(match anchor.affinity {
            Affinity::Before => ReflowPosition {
                row: position.row,
                col: atom.end_col,
            },
            Affinity::After => ReflowPosition {
                row: line.retained_first_row? + line.retained_row_count.saturating_sub(1),
                col: self.cols.saturating_sub(1),
            },
        })
    }

    pub(crate) fn project_cursor(&self, anchor: ContentAnchor) -> Option<ProjectedInsertion> {
        let line = self.lines.get(&anchor.line_id)?;
        if line.atoms.is_empty() {
            let position = self.final_gen_pos(ReflowPosition {
                row: line.retained_first_row?,
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
                return self.insertion_after(*atom);
            }
            let position = self.final_gen_pos(ReflowPosition {
                row: line.retained_first_row?,
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
                    position: self.final_gen_pos(atom.position?)?,
                    pending_wrap: false,
                }),
                Affinity::After => self.insertion_after(*atom),
            };
        }
        let end_offset = line.atom_start.0.checked_add(line.atoms.len())?;
        if anchor.offset.0 == end_offset {
            return line
                .atoms
                .iter()
                .rev()
                .find(|atom| atom.position.is_some())
                .copied()
                .and_then(|atom| self.insertion_after(atom));
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
        if position.row >= self.rows.len() || position.col >= self.cols {
            return None;
        }
        let row = u64::try_from(position.row).ok()?;
        Some(GenPos::new(
            self.normal.history_start().checked_add(row)?,
            position.col,
        ))
    }

    fn insertion_after(&self, atom: ReflowedAtom) -> Option<ProjectedInsertion> {
        let position = atom.position?;
        let insertion_col = atom.end_col.saturating_add(1);
        Some(ProjectedInsertion {
            position: self.final_gen_pos(ReflowPosition {
                row: position.row,
                col: insertion_col.min(self.cols.saturating_sub(1)),
            })?,
            pending_wrap: insertion_col >= self.cols,
        })
    }

    fn validate(&self) -> Result<(), PreparationError> {
        if self.normal.rows() != self.target_rows
            || self.normal.cols() != self.cols
            || self.normal.scroll_count().checked_add(self.target_rows) != Some(self.rows.len())
        {
            return Err(PreparationError::Invariant(
                "detached ring placement disagrees with prepared rows",
            ));
        }
        let mut previous: Option<(RowMetadata, usize)> = None;
        for row in &self.rows {
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
                let cell = row.cells[col];
                if cell.wide_continuation {
                    return Err(PreparationError::Invariant("orphan projected continuation"));
                }
                atom_count = atom_count
                    .checked_add(1)
                    .ok_or(PreparationError::ArithmeticOverflow)?;
                if UnicodeWidthChar::width(cell.ch).unwrap_or(0) == 2 {
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

fn pack_line(
    line: &LogicalLine,
    cols: usize,
    output: &mut Vec<ReflowedRow>,
) -> Result<Vec<ReflowedAtom>, PreparationError> {
    let mut logical_start = line.cell_start;
    let mut logical_atom_start = line.atom_start.0;
    let mut row = ReflowedRow::blank(
        cols,
        line.line_id,
        logical_start,
        logical_atom_start,
        line.head_truncated,
        line.head_truncated,
    )?;
    let mut atoms = Vec::new();
    atoms
        .try_reserve_exact(line.glyphs.len())
        .map_err(|_| PreparationError::AllocationFailed)?;

    for logical in &line.glyphs {
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
        let glyph = logical.glyph;
        validate_glyph(glyph)?;
        let width = usize::from(glyph.width);
        let used = row.metadata.meaningful_extent;
        let remaining = cols
            .checked_sub(used)
            .ok_or(PreparationError::ArithmeticOverflow)?;
        if width > remaining {
            logical_start = logical_start
                .checked_add(row.metadata.meaningful_extent)
                .ok_or(PreparationError::ArithmeticOverflow)?;
            logical_atom_start = logical.atom_offset.0;
            output.push(row);
            row = ReflowedRow::blank(
                cols,
                line.line_id,
                logical_start,
                logical_atom_start,
                true,
                false,
            )?;
        }

        let col = row.metadata.meaningful_extent;
        row.cells[col] = glyph.cell;
        row.cell_state[col] = glyph.cell_state;
        if width == 2 {
            row.cells[col + 1] = continuation_cell(glyph.cell);
            row.cell_state[col + 1] = glyph
                .continuation_state
                .ok_or(PreparationError::Invariant("wide glyph provenance missing"))?;
        }
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
            source_position: position,
            position: Some(position),
            end_col: col + width - 1,
        });
    }

    output.push(row);
    Ok(atoms)
}

fn validate_glyph(glyph: LogicalGlyph) -> Result<(), PreparationError> {
    if !matches!(glyph.width, 1 | 2) {
        return Err(PreparationError::Invariant("unsupported glyph width"));
    }
    if (glyph.width == 2) != (UnicodeWidthChar::width(glyph.cell.ch).unwrap_or(0) == 2) {
        return Err(PreparationError::Invariant(
            "glyph width disagrees with payload",
        ));
    }
    if glyph.meaningful_blank != (glyph.cell.ch == ' ' && glyph.cell_state.is_meaningful()) {
        return Err(PreparationError::Invariant(
            "blank provenance disagrees with payload",
        ));
    }
    Ok(())
}

fn continuation_cell(base: Cell) -> Cell {
    Cell {
        ch: ' ',
        wide_continuation: true,
        fg: base.fg,
        bg: base.bg,
        attrs: base.attrs,
        protected: base.protected,
        hyperlink: base.hyperlink,
    }
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
mod tests {
    use std::num::NonZeroU32;

    use crate::content_anchor::{Affinity, ContentProjection};
    use crate::model::{CellAttrs, HyperlinkId};
    use crate::selection_model::GenPos;

    use super::*;

    fn write(normal: &mut NormalBuf, row: usize, col: usize, cell: Cell) {
        normal.write_meaningful_cell(row, col, cell);
    }

    #[test]
    fn normalizes_width_and_moves_complete_wide_atoms_past_edge_padding() {
        let mut normal = NormalBuf::new(1, 4);
        write(
            &mut normal,
            0,
            0,
            Cell {
                ch: 'a',
                ..Cell::default()
            },
        );
        let hyperlink = HyperlinkId::from_nonzero(NonZeroU32::new(7).unwrap());
        let wide = Cell {
            ch: '界',
            protected: true,
            hyperlink: Some(hyperlink),
            ..Cell::default()
        };
        write(&mut normal, 0, 1, wide);
        write(&mut normal, 0, 2, continuation_cell(wide));
        write(
            &mut normal,
            0,
            3,
            Cell {
                ch: 'b',
                ..Cell::default()
            },
        );

        let prepared =
            PreparedPrimaryWidthReflow::prepare_geometry(&normal, normal.rows(), 1).unwrap();

        assert_eq!(prepared.cols(), 2);
        assert_eq!(prepared.rows().len(), 3);
        assert_eq!(prepared.rows()[0].metadata.meaningful_extent, 1);
        assert_eq!(prepared.rows()[0].cells[1], Cell::default());
        assert!(!prepared.rows()[0].cell_state[1].is_meaningful());
        assert_eq!(prepared.rows()[1].cells[0], wide);
        assert_eq!(prepared.rows()[1].cells[1], continuation_cell(wide));
        assert_eq!(prepared.rows()[1].metadata.logical_start, 1);
        assert_eq!(prepared.rows()[2].metadata.logical_start, 3);
        assert!(prepared.rows()[1].metadata.soft_wrapped);
        assert!(prepared.rows()[2].metadata.soft_wrapped);
    }

    #[test]
    fn preserves_meaningful_trailing_blanks_and_hyperlink_payload() {
        let mut normal = NormalBuf::new(1, 4);
        let hyperlink = HyperlinkId::from_nonzero(NonZeroU32::new(9).unwrap());
        for (col, cell) in [
            Cell {
                ch: 'x',
                ..Cell::default()
            },
            Cell::default(),
            Cell {
                hyperlink: Some(hyperlink),
                ..Cell::default()
            },
        ]
        .into_iter()
        .enumerate()
        {
            write(&mut normal, 0, col, cell);
        }

        let prepared =
            PreparedPrimaryWidthReflow::prepare_geometry(&normal, normal.rows(), 2).unwrap();

        assert_eq!(prepared.rows().len(), 2);
        assert_eq!(prepared.rows()[0].cells[1].ch, ' ');
        assert!(prepared.rows()[0].cell_state[1].is_meaningful());
        assert_eq!(prepared.rows()[1].cells[0].hyperlink, Some(hyperlink));
        assert!(prepared.rows()[1].cell_state[0].is_meaningful());
        assert_eq!(prepared.rows()[1].metadata.logical_start, 2);
    }

    #[test]
    fn preserves_explicit_empty_lines_and_is_deterministic() {
        let mut normal = NormalBuf::new(3, 4);
        for (col, ch) in "abc".chars().enumerate() {
            write(
                &mut normal,
                0,
                col,
                Cell {
                    ch,
                    ..Cell::default()
                },
            );
        }
        write(
            &mut normal,
            2,
            0,
            Cell {
                ch: 'z',
                ..Cell::default()
            },
        );

        let first =
            PreparedPrimaryWidthReflow::prepare_geometry(&normal, normal.rows(), 2).unwrap();
        let second =
            PreparedPrimaryWidthReflow::prepare_geometry(&normal, normal.rows(), 2).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.rows().len(), 4);
        assert_eq!(first.rows()[2].metadata.meaningful_extent, 0);
        assert_ne!(
            first.rows()[1].metadata.logical_line_id,
            first.rows()[2].metadata.logical_line_id
        );
        assert_ne!(
            first.rows()[2].metadata.logical_line_id,
            first.rows()[3].metadata.logical_line_id
        );
    }

    #[test]
    fn cursor_projection_derives_pending_wrap_from_new_width() {
        let mut normal = NormalBuf::new(1, 4);
        for (col, ch) in "abcd".chars().enumerate() {
            write(
                &mut normal,
                0,
                col,
                Cell {
                    ch,
                    ..Cell::default()
                },
            );
        }
        let projection = ContentProjection::build(&normal).unwrap();
        let pending = projection.cursor_anchor(GenPos::new(0, 3), true).unwrap();
        let selection = projection
            .to_anchor(GenPos::new(0, 2), Affinity::Before)
            .unwrap();

        let width_three =
            PreparedPrimaryWidthReflow::prepare_geometry(&normal, normal.rows(), 3).unwrap();
        let width_two =
            PreparedPrimaryWidthReflow::prepare_geometry(&normal, normal.rows(), 2).unwrap();

        assert_eq!(
            width_three.project_cursor(pending),
            Some(ProjectedInsertion {
                position: GenPos::new(2, 1),
                pending_wrap: false,
            })
        );
        assert_eq!(
            width_two.project_cursor(pending),
            Some(ProjectedInsertion {
                position: GenPos::new(2, 1),
                pending_wrap: true,
            })
        );
        assert_eq!(
            width_two.project_selection(selection),
            Some(GenPos::new(2, 0))
        );
    }

    fn logical_payload(prepared: &PreparedPrimaryWidthReflow) -> Vec<(LogicalLineId, Vec<Cell>)> {
        let mut lines: Vec<(LogicalLineId, Vec<Cell>)> = Vec::new();
        for row in prepared.rows() {
            if lines
                .last()
                .is_none_or(|(line_id, _)| *line_id != row.metadata.logical_line_id)
            {
                lines.push((row.metadata.logical_line_id, Vec::new()));
            }
            let cells = &mut lines.last_mut().unwrap().1;
            for cell in &row.cells[..row.metadata.meaningful_extent] {
                if !cell.wide_continuation {
                    cells.push(*cell);
                }
            }
        }
        lines
    }

    #[test]
    fn narrow_and_wide_preparations_preserve_logical_payload_without_padding() {
        let mut normal = NormalBuf::new(1, 8);
        let mut attrs = CellAttrs::default();
        attrs.set(CellAttrs::BOLD);
        let hyperlink = HyperlinkId::from_nonzero(NonZeroU32::new(11).unwrap());
        let cells = [
            Cell {
                ch: 'a',
                fg: harbor_config::Color::Indexed(4),
                attrs,
                ..Cell::default()
            },
            Cell {
                ch: ' ',
                bg: harbor_config::Color::Indexed(2),
                ..Cell::default()
            },
            Cell {
                ch: '界',
                hyperlink: Some(hyperlink),
                ..Cell::default()
            },
            Cell {
                ch: 'b',
                protected: true,
                ..Cell::default()
            },
        ];
        write(&mut normal, 0, 0, cells[0]);
        write(&mut normal, 0, 1, cells[1]);
        write(&mut normal, 0, 2, cells[2]);
        write(&mut normal, 0, 3, continuation_cell(cells[2]));
        write(&mut normal, 0, 4, cells[3]);

        let narrow =
            PreparedPrimaryWidthReflow::prepare_geometry(&normal, normal.rows(), 3).unwrap();
        let wide = PreparedPrimaryWidthReflow::prepare_geometry(&normal, normal.rows(), 8).unwrap();

        assert_eq!(logical_payload(&narrow), logical_payload(&wide));
        assert_eq!(logical_payload(&wide)[0].1, cells);
        assert_eq!(narrow.rows()[0].metadata.meaningful_extent, 2);
        assert_eq!(narrow.rows()[0].cells[2], Cell::default());
    }

    #[test]
    fn zero_width_normalizes_and_unallocatable_width_fails_cleanly() {
        let normal = NormalBuf::new(1, 2);
        assert_eq!(
            PreparedPrimaryWidthReflow::prepare_geometry(&normal, normal.rows(), 0)
                .unwrap()
                .cols(),
            2
        );
        assert_eq!(
            PreparedPrimaryWidthReflow::prepare_geometry(&normal, normal.rows(), usize::MAX,),
            Err(PreparationError::AllocationFailed)
        );
    }
    #[test]
    fn geometry_preparation_grows_with_distinct_blank_lines_and_full_damage() {
        let normal = NormalBuf::new_for_test(2, 3, 2);

        let prepared = PreparedPrimaryResize::prepare_geometry(&normal, 4, 3).unwrap();

        assert_eq!(prepared.target_rows(), 4);
        assert_eq!(prepared.rows().len(), 4);
        assert_eq!(prepared.normal().rows(), 4);
        assert_eq!(prepared.normal().scroll_count(), 0);
        assert_ne!(
            prepared.rows()[2].metadata.logical_line_id,
            prepared.rows()[3].metadata.logical_line_id
        );
        assert_eq!(prepared.normal().dirty_ranges().len(), 4);
        assert!(
            prepared
                .normal()
                .dirty_ranges()
                .iter()
                .all(|range| range.start_col == 0 && range.end_col == 3)
        );
    }

    #[test]
    fn capacity_evicts_exact_oldest_rows_and_reflow_is_repeatable() {
        let mut normal = NormalBuf::new_for_test(3, 4, 2);
        let mut ch = b'a';
        for row in 0..3 {
            for col in 0..4 {
                write(
                    &mut normal,
                    row,
                    col,
                    Cell {
                        ch: ch as char,
                        ..Cell::default()
                    },
                );
                ch += 1;
            }
            if row > 0 {
                let source = normal.live_row_metadata(row - 1);
                normal.continue_logical_line(row, source);
            }
        }
        let source_before = normal.clone();

        let prepared = PreparedPrimaryResize::prepare_geometry(&normal, 2, 2).unwrap();

        assert_eq!(prepared.dropped_rows(), 2);
        assert_eq!(prepared.rows().len(), 4);
        assert_eq!(prepared.normal().scroll_count(), 2);
        assert_eq!(prepared.normal().history_start(), 3);
        assert!(prepared.normal().cell_at_generation(0, 0).is_none());
        assert!(prepared.rows()[0].metadata.head_truncated);
        assert_eq!(prepared.rows()[0].metadata.logical_start, 4);
        assert_eq!(prepared.rows()[0].metadata.logical_atom_start, 4);
        assert_eq!(normal, source_before, "preparation must not mutate source");

        let prefix = prepared
            .source_anchor(GenPos::new(0, 0), Affinity::Before)
            .unwrap();
        let suffix = prepared
            .source_anchor(GenPos::new(2, 0), Affinity::Before)
            .unwrap();
        assert_eq!(prepared.project_selection(prefix), None);
        assert_eq!(prepared.project_selection(suffix), Some(GenPos::new(5, 0)));

        let live_cursor = prepared
            .source_cursor_anchor(GenPos::new(2, 3), false)
            .unwrap();
        let anchored = prepared
            .clone()
            .attach_screen_anchors(
                live_cursor,
                PreparedProjection::Projected(prefix),
                PreparedProjection::Projected(prefix),
            )
            .unwrap();
        assert_eq!(anchored.saved_cursor, PreparedProjection::Invalid);
        assert_eq!(
            anchored.review,
            PreparedProjection::Projected(GenPos::new(3, 0))
        );
        assert_eq!(anchored.normal().view_offset(), 2);

        let repeated = PreparedPrimaryResize::prepare_geometry(prepared.normal(), 2, 4).unwrap();
        assert!(repeated.rows()[0].metadata.head_truncated);
        assert_eq!(repeated.rows()[0].metadata.logical_start, 4);
        assert_eq!(repeated.rows()[0].metadata.logical_atom_start, 4);
        assert_eq!(repeated.normal().history_start(), 7);
    }

    #[test]
    fn preparation_preserves_semantic_order_after_source_ring_wrap() {
        let mut normal = NormalBuf::new_for_test(2, 4, 2);
        for index in 0..5 {
            write(
                &mut normal,
                0,
                0,
                Cell {
                    ch: (b'a' + index) as char,
                    ..Cell::default()
                },
            );
            normal.scroll_up_full_screen(1, Cell::default());
        }
        let source_order: Vec<_> = normal.retained_rows().map(|row| row.cells[0].ch).collect();

        let prepared = PreparedPrimaryResize::prepare_geometry(&normal, 2, 2).unwrap();
        let prepared_order: Vec<_> = prepared.rows().iter().map(|row| row.cells[0].ch).collect();

        assert_eq!(prepared_order, source_order);
        assert_eq!(normal.history_start(), 3);
        assert_eq!(prepared.normal().history_start(), 7);
        assert!(prepared.normal().cell_at_generation(6, 0).is_none());
    }

    #[test]
    fn height_shrink_rejects_a_live_cursor_outside_the_final_suffix() {
        let normal = NormalBuf::new_for_test(3, 2, 2);
        let prepared = PreparedPrimaryResize::prepare_geometry(&normal, 1, 2).unwrap();
        let cursor = prepared
            .source_cursor_anchor(GenPos::new(0, 0), false)
            .unwrap();

        assert_eq!(
            prepared.attach_screen_anchors(
                cursor,
                PreparedProjection::Absent,
                PreparedProjection::Absent,
            ),
            Err(PreparationError::UnresolvedLiveCursor)
        );
        assert_eq!(normal.rows(), 3);
    }

    #[test]
    fn height_shrink_invalidates_saved_cursor_that_moves_into_history() {
        let normal = NormalBuf::new_for_test(3, 2, 2);
        let prepared = PreparedPrimaryResize::prepare_geometry(&normal, 2, 2).unwrap();
        let live_cursor = prepared
            .source_cursor_anchor(GenPos::new(2, 0), false)
            .unwrap();
        let saved_cursor = prepared
            .source_cursor_anchor(GenPos::new(0, 0), false)
            .unwrap();

        let prepared = prepared
            .attach_screen_anchors(
                live_cursor,
                PreparedProjection::Projected(saved_cursor),
                PreparedProjection::Absent,
            )
            .unwrap();

        assert_eq!(prepared.saved_cursor, PreparedProjection::Invalid);
        assert_eq!(
            prepared.live_cursor,
            PreparedProjection::Projected(ProjectedInsertion {
                position: GenPos::new(5, 0),
                pending_wrap: false,
            })
        );
    }

    #[test]
    fn generation_range_overflow_fails_before_materialization() {
        let mut normal = NormalBuf::new_for_test(1, 2, 0);
        normal.set_history_start_for_test(u64::MAX - 1);

        assert_eq!(
            PreparedPrimaryResize::prepare_geometry(&normal, 2, 2),
            Err(PreparationError::ArithmeticOverflow)
        );
        assert_eq!(normal.history_start(), u64::MAX - 1);
    }

    #[test]
    fn accepts_head_truncated_input_without_mutating_source() {
        let mut normal = NormalBuf::new(1, 2);
        normal.set_head_truncated(0, true);
        let before = normal.row_metadata(0);

        let prepared =
            PreparedPrimaryWidthReflow::prepare_geometry(&normal, normal.rows(), 4).unwrap();
        assert!(prepared.rows()[0].metadata.head_truncated);
        assert_eq!(normal.row_metadata(0), before);
    }
}
