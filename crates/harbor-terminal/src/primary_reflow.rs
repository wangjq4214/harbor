//! Detached primary-screen width reflow.
//!
//! This module prepares an ordered physical-row sequence without assigning ring
//! generations or mutating the live `NormalBuf`. Height/capacity placement is a
//! later phase.

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
    pub(crate) position: ReflowPosition,
    pub(crate) pending_wrap: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PreparedSelectionProjection {
    pub(crate) anchor: ReflowPosition,
    pub(crate) cursor: ReflowPosition,
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
        soft_wrapped: bool,
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
                meaningful_extent: 0,
                soft_wrapped,
                head_truncated: false,
            },
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ReflowedAtom {
    source_span: SourceSpan,
    position: ReflowPosition,
    end_col: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ReflowedLine {
    source_generations: Vec<u64>,
    first_row: usize,
    row_count: usize,
    atoms: Vec<ReflowedAtom>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PreparedPrimaryWidthReflow {
    source_cols: usize,
    cols: usize,
    rows: Vec<ReflowedRow>,
    lines: HashMap<LogicalLineId, ReflowedLine>,
    pub(crate) live_cursor: PreparedProjection<ProjectedInsertion>,
    pub(crate) saved_cursor: PreparedProjection<ProjectedInsertion>,
    pub(crate) review: PreparedProjection<ReflowPosition>,
}

impl PreparedPrimaryWidthReflow {
    pub(crate) fn prepare(
        normal: &NormalBuf,
        requested_cols: usize,
    ) -> Result<Self, PreparationError> {
        let cols = requested_cols.max(MIN_REFLOW_COLS);
        let logical_lines = logical_content::decode_lines(normal)?;
        let mut rows = Vec::new();
        let mut lines = HashMap::with_capacity(logical_lines.len());

        for line in logical_lines {
            if line.head_truncated {
                return Err(PreparationError::UnsupportedTruncatedHead {
                    line_id: line.line_id,
                });
            }
            let first_row = rows.len();
            let atoms = pack_line(&line, cols, &mut rows)?;
            let row_count = rows
                .len()
                .checked_sub(first_row)
                .ok_or(PreparationError::ArithmeticOverflow)?;
            if lines
                .insert(
                    line.line_id,
                    ReflowedLine {
                        source_generations: line.generations.clone(),
                        first_row,
                        row_count,
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

        let prepared = Self {
            cols,
            source_cols: normal.cols(),
            rows,
            lines,
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
        let covering = line.atoms.iter().enumerate().find(|(_, atom)| {
            atom.source_span.generation == position.generation
                && position.col >= atom.source_span.start_col
                && position.col <= atom.source_span.end_col
        });
        let (offset, projection_hint, prefer_previous_projection) =
            if let Some((offset, _)) = covering {
                (LogicalAtomOffset(offset), None, false)
            } else {
                let next = line.atoms.iter().enumerate().find(|(_, atom)| {
                    atom.source_span.generation > position.generation
                        || (atom.source_span.generation == position.generation
                            && atom.source_span.start_col > position.col)
                });
                (
                    LogicalAtomOffset(next.map_or(line.atoms.len(), |(offset, _)| offset)),
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
            return Some(ContentAnchor::from_logical_parts(
                *line_id,
                LogicalAtomOffset(line.atoms.len().saturating_sub(1)),
                Affinity::After,
                None,
                false,
            ));
        }
        self.source_anchor(position, Affinity::Before)
    }

    pub(crate) fn project_selection(&self, anchor: ContentAnchor) -> Option<ReflowPosition> {
        let line = self.lines.get(&anchor.line_id)?;
        if line.atoms.is_empty() {
            return Some(ReflowPosition {
                row: line.first_row,
                col: anchor
                    .projection_hint_col()
                    .unwrap_or(0)
                    .min(self.cols.saturating_sub(1)),
            });
        }
        if anchor.prefers_previous_projection() {
            return anchor
                .offset
                .0
                .checked_sub(1)
                .and_then(|offset| line.atoms.get(offset))
                .map(|atom| ReflowPosition {
                    row: atom.position.row,
                    col: atom.end_col,
                })
                .or(Some(ReflowPosition {
                    row: line.first_row,
                    col: 0,
                }));
        }
        if let Some(atom) = line.atoms.get(anchor.offset.0) {
            return Some(match anchor.affinity {
                Affinity::Before => atom.position,
                Affinity::After => ReflowPosition {
                    row: atom.position.row,
                    col: atom.end_col,
                },
            });
        }
        if anchor.offset.0 != line.atoms.len() {
            return None;
        }
        let atom = line.atoms.last()?;
        Some(match anchor.affinity {
            Affinity::Before => ReflowPosition {
                row: atom.position.row,
                col: atom.end_col,
            },
            Affinity::After => ReflowPosition {
                row: line.first_row + line.row_count.saturating_sub(1),
                col: self.cols.saturating_sub(1),
            },
        })
    }

    pub(crate) fn project_cursor(&self, anchor: ContentAnchor) -> Option<ProjectedInsertion> {
        let line = self.lines.get(&anchor.line_id)?;
        if line.atoms.is_empty() {
            return Some(ProjectedInsertion {
                position: ReflowPosition {
                    row: line.first_row,
                    col: anchor
                        .projection_hint_col()
                        .unwrap_or(0)
                        .min(self.cols.saturating_sub(1)),
                },
                pending_wrap: false,
            });
        }
        if anchor.prefers_previous_projection() {
            return anchor
                .offset
                .0
                .checked_sub(1)
                .and_then(|offset| line.atoms.get(offset))
                .map(|atom| self.insertion_after(*atom))
                .or(Some(ProjectedInsertion {
                    position: ReflowPosition {
                        row: line.first_row,
                        col: 0,
                    },
                    pending_wrap: false,
                }));
        }
        if let Some(atom) = line.atoms.get(anchor.offset.0) {
            return Some(match anchor.affinity {
                Affinity::Before => ProjectedInsertion {
                    position: atom.position,
                    pending_wrap: false,
                },
                Affinity::After => self.insertion_after(*atom),
            });
        }
        if anchor.offset.0 == line.atoms.len() {
            return line
                .atoms
                .last()
                .copied()
                .map(|atom| self.insertion_after(atom));
        }
        None
    }

    pub(crate) fn attach_screen_anchors(
        mut self,
        live_cursor: ContentAnchor,
        saved_cursor: PreparedProjection<ContentAnchor>,
        review: PreparedProjection<ContentAnchor>,
    ) -> Result<Self, PreparationError> {
        self.live_cursor = PreparedProjection::Projected(
            self.project_cursor(live_cursor)
                .ok_or(PreparationError::UnresolvedLiveCursor)?,
        );
        self.saved_cursor = match saved_cursor {
            PreparedProjection::Projected(anchor) => self
                .project_cursor(anchor)
                .map_or(PreparedProjection::Invalid, PreparedProjection::Projected),
            PreparedProjection::Absent => PreparedProjection::Absent,
            PreparedProjection::Invalid => PreparedProjection::Invalid,
        };
        self.review = match review {
            PreparedProjection::Projected(anchor) => self
                .project_selection(anchor)
                .map_or(PreparedProjection::Invalid, PreparedProjection::Projected),
            PreparedProjection::Absent => PreparedProjection::Absent,
            PreparedProjection::Invalid => PreparedProjection::Invalid,
        };
        Ok(self)
    }

    fn insertion_after(&self, atom: ReflowedAtom) -> ProjectedInsertion {
        let insertion_col = atom.end_col.saturating_add(1);
        ProjectedInsertion {
            position: ReflowPosition {
                row: atom.position.row,
                col: insertion_col.min(self.cols.saturating_sub(1)),
            },
            pending_wrap: insertion_col >= self.cols,
        }
    }

    fn validate(&self) -> Result<(), PreparationError> {
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
            while col < row.metadata.meaningful_extent {
                let cell = row.cells[col];
                if cell.wide_continuation {
                    return Err(PreparationError::Invariant("orphan projected continuation"));
                }
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
        }
        Ok(())
    }
}

fn pack_line(
    line: &LogicalLine,
    cols: usize,
    output: &mut Vec<ReflowedRow>,
) -> Result<Vec<ReflowedAtom>, PreparationError> {
    let first_row = output.len();
    let mut logical_start = 0usize;
    let mut row = ReflowedRow::blank(cols, line.line_id, logical_start, false)?;
    let mut atoms = Vec::with_capacity(line.glyphs.len());

    for logical in &line.glyphs {
        if logical.atom_offset.0 != atoms.len() {
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
            output.push(row);
            row = ReflowedRow::blank(cols, line.line_id, logical_start, true)?;
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
        atoms.push(ReflowedAtom {
            source_span: logical.source_span,
            position: ReflowPosition {
                row: first_row + output.len().saturating_sub(first_row),
                col,
            },
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
    UnsupportedTruncatedHead { line_id: LogicalLineId },
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
            Self::UnsupportedTruncatedHead { line_id } => {
                write!(formatter, "logical line {} has a truncated head", line_id.0)
            }
            Self::ArithmeticOverflow => {
                formatter.write_str("primary width reflow arithmetic overflow")
            }
            Self::AllocationFailed => formatter.write_str("primary width reflow allocation failed"),
            Self::Invariant(message) => {
                write!(formatter, "primary width reflow invariant: {message}")
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

        let prepared = PreparedPrimaryWidthReflow::prepare(&normal, 1).unwrap();

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

        let prepared = PreparedPrimaryWidthReflow::prepare(&normal, 2).unwrap();

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

        let first = PreparedPrimaryWidthReflow::prepare(&normal, 2).unwrap();
        let second = PreparedPrimaryWidthReflow::prepare(&normal, 2).unwrap();

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

        let width_three = PreparedPrimaryWidthReflow::prepare(&normal, 3).unwrap();
        let width_two = PreparedPrimaryWidthReflow::prepare(&normal, 2).unwrap();

        assert_eq!(
            width_three.project_cursor(pending),
            Some(ProjectedInsertion {
                position: ReflowPosition { row: 1, col: 1 },
                pending_wrap: false,
            })
        );
        assert_eq!(
            width_two.project_cursor(pending),
            Some(ProjectedInsertion {
                position: ReflowPosition { row: 1, col: 1 },
                pending_wrap: true,
            })
        );
        assert_eq!(
            width_two.project_selection(selection),
            Some(ReflowPosition { row: 1, col: 0 })
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

        let narrow = PreparedPrimaryWidthReflow::prepare(&normal, 3).unwrap();
        let wide = PreparedPrimaryWidthReflow::prepare(&normal, 8).unwrap();

        assert_eq!(logical_payload(&narrow), logical_payload(&wide));
        assert_eq!(logical_payload(&wide)[0].1, cells);
        assert_eq!(narrow.rows()[0].metadata.meaningful_extent, 2);
        assert_eq!(narrow.rows()[0].cells[2], Cell::default());
    }

    #[test]
    fn zero_width_normalizes_and_unallocatable_width_fails_cleanly() {
        let normal = NormalBuf::new(1, 2);
        assert_eq!(
            PreparedPrimaryWidthReflow::prepare(&normal, 0)
                .unwrap()
                .cols(),
            2
        );
        assert_eq!(
            PreparedPrimaryWidthReflow::prepare(&normal, usize::MAX),
            Err(PreparationError::AllocationFailed)
        );
    }

    #[test]
    fn rejects_head_truncated_input_without_mutating_source() {
        let mut normal = NormalBuf::new(1, 2);
        normal.set_head_truncated(0, true);
        let before = normal.row_metadata(0);

        assert!(matches!(
            PreparedPrimaryWidthReflow::prepare(&normal, 4),
            Err(PreparationError::UnsupportedTruncatedHead {
                line_id
            }) if line_id == before.logical_line_id
        ));
        assert_eq!(normal.row_metadata(0), before);
    }
}
