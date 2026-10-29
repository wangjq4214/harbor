//! Pre-optimization scalar packer and materialized decoder, kept only as test oracles.
use super::*;
use crate::logical_content::{DecodeErrorKind, LogicalAtom, LogicalLineGlyph};

pub(super) fn prepare_geometry_with_limits(
    normal: &NormalBuf,
    requested_rows: usize,
    requested_cols: usize,
    cursor_floor: Option<usize>,
    viewport: ReflowViewport,
    retained_cursors: &[CursorRetention],
    limits: harbor_config::HistoryLimits,
) -> Result<PreparedPrimaryResize, PreparationError> {
    let target_rows = requested_rows.max(1);
    let cols = requested_cols.max(MIN_REFLOW_COLS);
    let active_count = normal.active_retained_row_count(cursor_floor);
    let logical_lines = decode_lines_active(normal, active_count)?;
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

    let source_live_top = normal
        .history_start()
        .checked_add(
            u64::try_from(normal.scroll_count())
                .map_err(|_| PreparationError::ArithmeticOverflow)?,
        )
        .ok_or(PreparationError::ArithmeticOverflow)?;
    let live_boundary = (viewport == ReflowViewport::PreserveLiveTop).then_some(source_live_top);

    for line in logical_lines {
        let first_row = rows.len();
        let atoms = pack_line(&line, cols, live_boundary, retained_cursors, &mut rows)?;
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
            rows.push(ReflowedRow::blank(
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
        rows.push(ReflowedRow::blank(cols, line_id, 0, 0, false, false)?);
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

    let prepared = PreparedPrimaryResize {
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
    // retain their original precedence, then discard the temporary packed rows.
    prepared.validate(rows.len())?;
    Ok(prepared)
}

pub(super) fn decode_lines_active(
    normal: &NormalBuf,
    active_count: usize,
) -> Result<Vec<LogicalLine>, DecodeError> {
    let last_generation = normal
        .retained_rows_bounded(active_count)
        .last()
        .map_or(0, |row| row.generation);
    let atoms = logical_content::decode_active(normal, active_count)?;
    let mut lines = Vec::new();

    for (index, row) in normal.retained_rows_bounded(active_count).enumerate() {
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
            generation: last_generation,
            column: 0,
            kind: DecodeErrorKind::InconsistentLogicalLine,
        });
    }

    Ok(lines)
}

pub(super) fn pack_line(
    line: &LogicalLine,
    cols: usize,
    live_boundary: Option<u64>,
    retained_cursors: &[CursorRetention],
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
    // Only explicit, undecorated spaces at the *logical* end are disposable.
    let visible_end = line
        .glyphs
        .iter()
        .rposition(|logical| {
            logical.glyph.cell != Cell::default() || !logical.glyph.cell_state.is_explicit()
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

    for logical in line.glyphs.iter().take(retained_count) {
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
        row.cells[col] = glyph.cell.clone();
        row.cell_state[col] = glyph.cell_state;
        if width == 2 {
            row.cells[col + 1] = Cell::wide_continuation_of(&glyph.cell);
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
            position,
            end_col: col + width - 1,
        });
    }

    output.push(row);
    Ok(atoms)
}
