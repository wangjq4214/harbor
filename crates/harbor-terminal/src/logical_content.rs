//! Shared interpretation of retained physical rows as logical terminal content.

use std::fmt;

use unicode_width::UnicodeWidthChar;

use crate::model::SelectionBounds;
use crate::normal_buf::{CellState, LogicalLineId, NormalBuf, RetainedRow};
use crate::screen::Cell;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LogicalAtomOffset(pub(crate) usize);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct SourceSpan {
    pub(crate) generation: u64,
    pub(crate) start_col: usize,
    pub(crate) end_col: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LogicalGlyph {
    pub(crate) cell: Cell,
    pub(crate) width: u8,
    pub(crate) meaningful_blank: bool,
    pub(crate) cell_state: CellState,
    pub(crate) continuation_state: Option<CellState>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LogicalLineGlyph {
    pub(crate) atom_offset: LogicalAtomOffset,
    pub(crate) source_span: SourceSpan,
    pub(crate) glyph: LogicalGlyph,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LogicalLine {
    pub(crate) line_id: LogicalLineId,
    pub(crate) head_truncated: bool,
    pub(crate) atom_start: LogicalAtomOffset,
    pub(crate) cell_start: usize,
    pub(crate) generations: Vec<u64>,
    pub(crate) glyphs: Vec<LogicalLineGlyph>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LogicalAtom {
    Glyph {
        line_id: LogicalLineId,
        atom_offset: LogicalAtomOffset,
        source_span: SourceSpan,
        glyph: LogicalGlyph,
    },
    HardBreak {
        after_generation: u64,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DecodeErrorKind {
    OrphanWideContinuation,
    MissingWideContinuation,
    InconsistentSoftWrap,
    InconsistentLogicalLine,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DecodeError {
    pub(crate) generation: u64,
    pub(crate) column: usize,
    pub(crate) kind: DecodeErrorKind,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "logical content decode failed at generation {}, column {}: {:?}",
            self.generation, self.column, self.kind
        )
    }
}

pub(crate) fn decode(normal: &NormalBuf) -> Result<Vec<LogicalAtom>, DecodeError> {
    let rows: Vec<_> = normal.retained_rows().collect();
    let mut atoms = Vec::new();
    let mut previous: Option<RetainedRow<'_>> = None;
    let mut atom_offset = 0usize;

    for row in rows {
        if let Some(prior) = previous {
            if row.metadata.soft_wrapped {
                let expected_start = prior
                    .metadata
                    .logical_start
                    .checked_add(prior.metadata.meaningful_extent);
                if row.metadata.logical_line_id != prior.metadata.logical_line_id
                    || expected_start != Some(row.metadata.logical_start)
                    || row.metadata.logical_atom_start != atom_offset
                {
                    return Err(DecodeError {
                        generation: row.generation,
                        column: 0,
                        kind: DecodeErrorKind::InconsistentSoftWrap,
                    });
                }
            } else {
                atoms.push(LogicalAtom::HardBreak {
                    after_generation: prior.generation,
                });
                atom_offset = row.metadata.logical_atom_start;
            }
        } else {
            if row.metadata.soft_wrapped && !row.metadata.head_truncated {
                return Err(DecodeError {
                    generation: row.generation,
                    column: 0,
                    kind: DecodeErrorKind::InconsistentSoftWrap,
                });
            }
            atom_offset = row.metadata.logical_atom_start;
        }

        decode_row(row, &mut atom_offset, &mut atoms)?;
        previous = Some(row);
    }

    Ok(atoms)
}

fn decode_row(
    row: RetainedRow<'_>,
    atom_offset: &mut usize,
    atoms: &mut Vec<LogicalAtom>,
) -> Result<(), DecodeError> {
    let extent = row.metadata.meaningful_extent.min(row.cells.len());
    let mut col = 0;
    while col < extent {
        let cell = row.cells[col];
        if cell.wide_continuation {
            return Err(DecodeError {
                generation: row.generation,
                column: col,
                kind: DecodeErrorKind::OrphanWideContinuation,
            });
        }

        let width = if UnicodeWidthChar::width(cell.ch).unwrap_or(0) == 2 {
            if col + 1 >= extent || !row.cells[col + 1].wide_continuation {
                return Err(DecodeError {
                    generation: row.generation,
                    column: col,
                    kind: DecodeErrorKind::MissingWideContinuation,
                });
            }
            2
        } else {
            1
        };
        let mut normalized_cell = cell;
        normalized_cell.wide_continuation = false;
        atoms.push(LogicalAtom::Glyph {
            line_id: row.metadata.logical_line_id,
            atom_offset: LogicalAtomOffset(*atom_offset),
            source_span: SourceSpan {
                generation: row.generation,
                start_col: col,
                end_col: col + usize::from(width - 1),
            },
            glyph: LogicalGlyph {
                cell: normalized_cell,
                width,
                meaningful_blank: cell.ch == ' ' && row.cell_state[col].is_meaningful(),
                cell_state: row.cell_state[col],
                continuation_state: (width == 2).then(|| row.cell_state[col + 1]),
            },
        });
        *atom_offset += 1;
        col += usize::from(width);
    }
    Ok(())
}

pub(crate) fn decode_lines(normal: &NormalBuf) -> Result<Vec<LogicalLine>, DecodeError> {
    let rows: Vec<_> = normal.retained_rows().collect();
    let atoms = decode(normal)?;
    let mut lines = Vec::new();

    for (index, row) in rows.iter().enumerate() {
        if index == 0 || !row.metadata.soft_wrapped {
            lines.push(LogicalLine {
                line_id: row.metadata.logical_line_id,
                head_truncated: row.metadata.head_truncated,
                atom_start: LogicalAtomOffset(row.metadata.logical_atom_start),
                cell_start: row.metadata.logical_start,
                generations: vec![row.generation],
                glyphs: Vec::new(),
            });
        } else if let Some(line) = lines.last_mut() {
            line.head_truncated |= row.metadata.head_truncated;
            line.generations.push(row.generation);
        }
    }

    let mut line_index = 0usize;
    for atom in atoms {
        match atom {
            LogicalAtom::HardBreak { after_generation } => {
                line_index = line_index.checked_add(1).ok_or(DecodeError {
                    generation: after_generation,
                    column: 0,
                    kind: DecodeErrorKind::InconsistentLogicalLine,
                })?;
                if line_index >= lines.len() {
                    return Err(DecodeError {
                        generation: after_generation,
                        column: 0,
                        kind: DecodeErrorKind::InconsistentLogicalLine,
                    });
                }
            }
            LogicalAtom::Glyph {
                line_id,
                atom_offset,
                source_span,
                glyph,
            } => {
                let Some(line) = lines.get_mut(line_index) else {
                    return Err(DecodeError {
                        generation: source_span.generation,
                        column: source_span.start_col,
                        kind: DecodeErrorKind::InconsistentLogicalLine,
                    });
                };
                let expected_offset = line.atom_start.0.checked_add(line.glyphs.len());
                if line.line_id != line_id || expected_offset != Some(atom_offset.0) {
                    return Err(DecodeError {
                        generation: source_span.generation,
                        column: source_span.start_col,
                        kind: DecodeErrorKind::InconsistentLogicalLine,
                    });
                }
                line.glyphs.push(LogicalLineGlyph {
                    atom_offset,
                    source_span,
                    glyph,
                });
            }
        }
    }

    if !lines.is_empty() && line_index + 1 != lines.len() {
        return Err(DecodeError {
            generation: rows.last().map_or(0, |row| row.generation),
            column: 0,
            kind: DecodeErrorKind::InconsistentLogicalLine,
        });
    }

    Ok(lines)
}

pub(crate) fn selected_text(
    normal: &NormalBuf,
    bounds: SelectionBounds,
) -> Result<String, DecodeError> {
    if (bounds.start_row, bounds.start_col) > (bounds.end_row, bounds.end_col) {
        return Ok(String::new());
    }

    let retained_count = normal.scroll_count() + normal.rows();
    let first_generation = normal.history_start();
    let last_generation = first_generation + retained_count as u64 - 1;
    let selected_start = bounds.start_row.max(first_generation);
    let selected_end = bounds.end_row.min(last_generation);
    if selected_start > selected_end {
        return Ok(String::new());
    }

    let atoms = decode(normal)?;
    let mut text = String::new();
    let last_col = normal.cols().saturating_sub(1);
    for atom in atoms {
        match atom {
            LogicalAtom::Glyph {
                source_span, glyph, ..
            } if source_span.generation >= selected_start
                && source_span.generation <= selected_end =>
            {
                let start_col = if source_span.generation == bounds.start_row {
                    bounds.start_col
                } else {
                    0
                };
                let end_col = if source_span.generation == bounds.end_row {
                    bounds.end_col.min(last_col)
                } else {
                    last_col
                };
                if source_span.start_col >= start_col && source_span.start_col <= end_col {
                    text.push(glyph.cell.ch);
                }
            }
            LogicalAtom::HardBreak { after_generation }
                if after_generation >= selected_start && after_generation < selected_end =>
            {
                text.push('\n');
            }
            _ => {}
        }
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use harbor_config::Color;

    use super::*;
    use crate::model::{CellAttrs, HyperlinkId};

    fn write(normal: &mut NormalBuf, row: usize, col: usize, cell: Cell) {
        normal.write_meaningful_cell(row, col, cell);
    }

    #[test]
    fn decode_preserves_payload_offsets_and_soft_wrapped_identity() {
        let mut normal = NormalBuf::new(2, 4);
        let mut attrs = CellAttrs::default();
        attrs.set(CellAttrs::BOLD);
        let hyperlink = HyperlinkId::from_nonzero(NonZeroU32::new(7).unwrap());
        let styled = Cell {
            ch: 'A',
            fg: Color::Named(1),
            attrs,
            protected: true,
            hyperlink: Some(hyperlink),
            ..Cell::default()
        };
        write(&mut normal, 0, 0, styled);
        write(&mut normal, 0, 1, Cell::default());
        let source = normal.live_row_metadata(0);
        write(
            &mut normal,
            1,
            0,
            Cell {
                ch: '界',
                ..Cell::default()
            },
        );
        write(
            &mut normal,
            1,
            1,
            Cell {
                wide_continuation: true,
                ..Cell::default()
            },
        );
        normal.continue_logical_line(1, source, normal.live_row_logical_atom_count(0));

        let atoms = decode(&normal).unwrap();

        assert_eq!(atoms.len(), 3);
        let LogicalAtom::Glyph {
            line_id,
            atom_offset,
            source_span,
            glyph,
        } = atoms[0]
        else {
            panic!("expected glyph");
        };
        assert_eq!(line_id, source.logical_line_id);
        assert_eq!(atom_offset, LogicalAtomOffset(0));
        assert_eq!(
            source_span,
            SourceSpan {
                generation: 0,
                start_col: 0,
                end_col: 0,
            }
        );
        assert_eq!(glyph.cell, styled);
        assert_eq!(glyph.width, 1);
        assert!(!glyph.meaningful_blank);

        let LogicalAtom::Glyph {
            atom_offset, glyph, ..
        } = atoms[1]
        else {
            panic!("expected blank glyph");
        };
        assert_eq!(atom_offset, LogicalAtomOffset(1));
        assert_eq!(glyph.cell.ch, ' ');
        assert!(glyph.meaningful_blank);

        let LogicalAtom::Glyph {
            line_id,
            atom_offset,
            source_span,
            glyph,
        } = atoms[2]
        else {
            panic!("expected wide glyph");
        };
        assert_eq!(line_id, source.logical_line_id);
        assert_eq!(atom_offset, LogicalAtomOffset(2));
        assert_eq!(
            source_span,
            SourceSpan {
                generation: 1,
                start_col: 0,
                end_col: 1,
            }
        );
        assert_eq!(glyph.cell.ch, '界');
        assert!(!glyph.cell.wide_continuation);
        assert_eq!(glyph.width, 2);
    }

    #[test]
    fn decode_emits_hard_breaks_for_consecutive_empty_rows() {
        let normal = NormalBuf::new(3, 2);

        assert_eq!(
            decode(&normal).unwrap(),
            vec![
                LogicalAtom::HardBreak {
                    after_generation: 0
                },
                LogicalAtom::HardBreak {
                    after_generation: 1
                },
            ]
        );
    }

    #[test]
    fn decode_rejects_wide_corruption_and_inconsistent_soft_wraps() {
        let mut orphan = NormalBuf::new(1, 2);
        write(
            &mut orphan,
            0,
            0,
            Cell {
                wide_continuation: true,
                ..Cell::default()
            },
        );
        assert_eq!(
            decode(&orphan).unwrap_err(),
            DecodeError {
                generation: 0,
                column: 0,
                kind: DecodeErrorKind::OrphanWideContinuation,
            }
        );

        let mut missing = NormalBuf::new(1, 2);
        write(
            &mut missing,
            0,
            0,
            Cell {
                ch: '界',
                ..Cell::default()
            },
        );
        assert_eq!(
            decode(&missing).unwrap_err().kind,
            DecodeErrorKind::MissingWideContinuation
        );

        let mut inconsistent = NormalBuf::new(2, 2);
        inconsistent.set_wrapped(1, true);
        assert_eq!(
            decode(&inconsistent).unwrap_err().kind,
            DecodeErrorKind::InconsistentSoftWrap
        );
    }

    #[test]
    fn decode_preserves_absolute_atom_offsets_for_truncated_mixed_width_suffix() {
        let mut normal = NormalBuf::new(2, 4);
        write(
            &mut normal,
            0,
            0,
            Cell {
                ch: '界',
                ..Cell::default()
            },
        );
        write(
            &mut normal,
            0,
            1,
            Cell {
                wide_continuation: true,
                ..Cell::default()
            },
        );
        write(
            &mut normal,
            0,
            2,
            Cell {
                ch: 'x',
                ..Cell::default()
            },
        );
        normal.set_wrapped(0, true);
        normal.set_head_truncated(0, true);
        normal.set_logical_starts(0, 5, 3);
        let source = normal.live_row_metadata(0);
        write(
            &mut normal,
            1,
            0,
            Cell {
                ch: 'y',
                ..Cell::default()
            },
        );
        normal.continue_logical_line(1, source, normal.live_row_logical_atom_count(0));

        let line = decode_lines(&normal).unwrap().pop().unwrap();
        assert!(line.head_truncated);
        assert_eq!(line.atom_start, LogicalAtomOffset(3));
        assert_eq!(line.cell_start, 5);
        assert_eq!(
            line.glyphs
                .iter()
                .map(|glyph| glyph.atom_offset)
                .collect::<Vec<_>>(),
            vec![
                LogicalAtomOffset(3),
                LogicalAtomOffset(4),
                LogicalAtomOffset(5)
            ]
        );
        assert_eq!(normal.live_row_metadata(1).logical_atom_start, 5);
    }
    #[test]
    fn physical_selection_uses_wide_lead_and_preserves_empty_line_count() {
        let mut normal = NormalBuf::new(3, 3);
        write(
            &mut normal,
            0,
            0,
            Cell {
                ch: '界',
                ..Cell::default()
            },
        );
        write(
            &mut normal,
            0,
            1,
            Cell {
                wide_continuation: true,
                ..Cell::default()
            },
        );

        let lead_only = SelectionBounds {
            start_row: 0,
            start_col: 0,
            end_row: 0,
            end_col: 0,
        };
        let continuation_only = SelectionBounds {
            start_row: 0,
            start_col: 1,
            end_row: 0,
            end_col: 1,
        };
        let all_rows = SelectionBounds {
            start_row: 0,
            start_col: 0,
            end_row: 2,
            end_col: 2,
        };

        assert_eq!(selected_text(&normal, lead_only).unwrap(), "界");
        assert_eq!(selected_text(&normal, continuation_only).unwrap(), "");
        assert_eq!(selected_text(&normal, all_rows).unwrap(), "界\n\n");
    }

    #[test]
    fn selected_text_decodes_generation_order_after_ring_eviction() {
        let mut normal = NormalBuf::new(3, 2);
        write(
            &mut normal,
            1,
            0,
            Cell {
                ch: 'Z',
                ..Cell::default()
            },
        );
        for _ in 0..=normal.max_scrollback() {
            normal.scroll_up_full_screen(1, Cell::default());
        }
        let first = normal.history_start();

        let text = selected_text(
            &normal,
            SelectionBounds {
                start_row: first,
                start_col: 0,
                end_row: first,
                end_col: 1,
            },
        )
        .unwrap();

        assert_eq!(first, 1);
        assert_eq!(text, "Z");
    }
}
