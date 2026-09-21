//! Character writing: maps a decoded char onto the grid using the current
//! pen state and cursor position.  Handles autowrap, margin clamping,
//! insert-mode shifts, and wide-glyph pairing.
//!
//! `CellWriter` is a stateless namespace — all methods are associated
//! functions that take `&mut PenState`, `&mut NormalBuf`, and
//! `&mut CursorEngine`.

use crate::normal_buf::NormalBuf;
use unicode_width::UnicodeWidthChar;

use super::super::cursor::CursorEngine;
use super::cell_ops::CellOps;
use super::pen_state::{PenState, SingleShift, map_dec_graphics};

/// Stateless namespace for writing characters to the grid.
pub(crate) struct CellWriter;

impl CellWriter {
    // ── write_char ────────────────────────────────────────────────

    /// Decodes one parser-delivered printable character, writes it at the
    /// cursor, and advances by its terminal cell width.
    pub(crate) fn write_char(
        pen_state: &mut PenState,
        normal: &mut NormalBuf,
        cursor: &mut CursorEngine,
        ch: char,
    ) {
        // 1. Decode the character through the active charset and measure width.
        let (ch, width) = Self::decode_char(pen_state, ch);
        if Self::write_decoded_char(pen_state, normal, cursor, ch, width) {
            pen_state.charsets.last_char = Some(ch);
        }
    }

    /// Writes a character produced by an internal screen operation.
    ///
    /// Internal writes, such as tab-fill cells and REP output, are already
    /// resolved display characters. They must not consume a pending SS2/SS3
    /// invocation or replace the last parser-delivered graphic character.
    pub(crate) fn write_internal_char(
        pen_state: &mut PenState,
        normal: &mut NormalBuf,
        cursor: &mut CursorEngine,
        ch: char,
    ) {
        let width = UnicodeWidthChar::width(ch).unwrap_or(0).min(2);
        Self::write_decoded_char(pen_state, normal, cursor, ch, width);
    }

    fn write_decoded_char(
        pen_state: &mut PenState,
        normal: &mut NormalBuf,
        cursor: &mut CursorEngine,
        ch: char,
        width: usize,
    ) -> bool {
        if width == 0 {
            return false;
        }

        let (left_limit, right_limit) = if cursor.margins.enabled {
            (cursor.margins.left, cursor.margins.right)
        } else {
            (0, normal.cols().saturating_sub(1))
        };

        // 2. Prepare: handle pending wrap, clamp, and insert-mode shift.
        if !Self::prepare_position(
            normal,
            cursor,
            pen_state,
            ch,
            width,
            (left_limit, right_limit),
        ) {
            return false;
        };

        // 3. Commit the glyph to the grid.
        Self::commit_cell(
            pen_state,
            normal,
            cursor,
            ch,
            width,
            (left_limit, right_limit),
        );

        // 4. Advance cursor and set pending_wrap.
        Self::advance_cursor(cursor, width, (left_limit, right_limit));
        true
    }

    // ── decomposed helpers ────────────────────────────────────────

    /// Decodes a character through the active charset and returns the mapped
    /// character plus its terminal display width (0, 1, or 2).
    fn decode_char(pen_state: &mut PenState, ch: char) -> (char, usize) {
        let active_set = match pen_state.charsets.single_shift.take() {
            Some(SingleShift::G2) => pen_state.charsets.g2,
            Some(SingleShift::G3) => pen_state.charsets.g3,
            None => {
                if pen_state.charsets.active == 0 {
                    pen_state.charsets.g0
                } else {
                    pen_state.charsets.g1
                }
            }
        };
        let ch = if active_set == b'0' {
            map_dec_graphics(ch)
        } else {
            ch
        };
        let width = UnicodeWidthChar::width(ch).unwrap_or(0).min(2);
        (ch, width)
    }

    /// Handles pending autowrap, margin boundary checks, and insert-mode
    /// shifting. Returns `None` when the write should be suppressed.
    fn prepare_position(
        normal: &mut NormalBuf,
        cursor: &mut CursorEngine,
        pen_state: &mut PenState,
        _ch: char,
        width: usize,
        (left_limit, right_limit): (usize, usize),
    ) -> bool {
        // Handle pending wrap if autowrap is on.
        if cursor.modes.autowrap && cursor.modes.pending_wrap {
            let mut source = normal.live_row_metadata(cursor.cursor.y);
            let mut source_atom_count = normal.live_row_logical_atom_count(cursor.cursor.y);
            cursor.carriage_return();
            if Self::newline_inner(pen_state, normal, cursor) {
                if !cursor.margins.enabled && cursor.cursor.y > 0 {
                    let source_row = cursor.cursor.y - 1;
                    source = normal.live_row_metadata(source_row);
                    source_atom_count = normal.live_row_logical_atom_count(source_row);
                }
                normal.continue_logical_line(cursor.cursor.y, source, source_atom_count);
            }
            cursor.modes.pending_wrap = false;
        }

        // Ignore writes outside active horizontal bounds.
        if cursor.cursor.x < left_limit || cursor.cursor.x > right_limit {
            return false;
        }
        if !cursor.modes.autowrap && cursor.cursor.x == right_limit {
            cursor.cursor.x = right_limit;
        }

        // If a wide character cannot fit, wrap only when DECAWM is enabled.
        if width == 2 && cursor.cursor.x + 1 > right_limit {
            if !cursor.modes.autowrap {
                return false;
            }
            let mut source = normal.live_row_metadata(cursor.cursor.y);
            let mut source_atom_count = normal.live_row_logical_atom_count(cursor.cursor.y);
            cursor.carriage_return();
            if Self::newline_inner(pen_state, normal, cursor) {
                if !cursor.margins.enabled && cursor.cursor.y > 0 {
                    let source_row = cursor.cursor.y - 1;
                    source = normal.live_row_metadata(source_row);
                    source_atom_count = normal.live_row_logical_atom_count(source_row);
                }
                normal.continue_logical_line(cursor.cursor.y, source, source_atom_count);
            }
            cursor.modes.pending_wrap = false;
        }

        let start_x = cursor.cursor.x;

        // Do not split a glyph crossing an active margin.
        if (start_x..start_x + width).any(|col| {
            CellOps::wide_range(normal, cursor.cursor.y, col)
                .is_some_and(|(base, continuation)| base < left_limit || continuation > right_limit)
        }) {
            return false;
        }

        if cursor.modes.insert {
            let insert_col = CellOps::wide_range(normal, cursor.cursor.y, start_x)
                .map_or(start_x, |(base, _)| base);
            if !CellOps::insert_chars(pen_state, normal, cursor, width) {
                return false;
            }
            cursor.cursor.x = insert_col;
        }

        true
    }

    /// Clears the target cell(s) and writes the glyph to the grid.
    fn commit_cell(
        pen_state: &PenState,
        normal: &mut NormalBuf,
        cursor: &CursorEngine,
        ch: char,
        width: usize,
        (left_limit, right_limit): (usize, usize),
    ) {
        let start_x = cursor.cursor.x;
        let start_col = if start_x > 0 && normal.cell(cursor.cursor.y, start_x).wide_continuation {
            start_x - 1
        } else {
            start_x
        };
        let end_col = (start_x + width + 1).min(right_limit + 1);
        normal.mark_range_dirty(cursor.cursor.y, start_col, end_col);

        Self::clear_cell_for_write(
            normal,
            cursor.cursor.y,
            cursor.cursor.x,
            left_limit,
            right_limit,
        );
        if width == 2 && cursor.cursor.x < right_limit {
            Self::clear_cell_for_write(
                normal,
                cursor.cursor.y,
                cursor.cursor.x + 1,
                left_limit,
                right_limit,
            );
        }

        let cell = crate::Cell {
            ch,
            wide_continuation: false,
            fg: pen_state.pen.fg,
            bg: pen_state.pen.bg,
            attrs: pen_state.pen.attrs,
            protected: pen_state.pen.protected,
            hyperlink: pen_state.active_hyperlink,
        };
        normal.write_meaningful_cell(cursor.cursor.y, cursor.cursor.x, cell);

        if width == 2 && cursor.cursor.x < right_limit {
            normal.write_meaningful_cell(
                cursor.cursor.y,
                cursor.cursor.x + 1,
                crate::Cell {
                    ch: ' ',
                    wide_continuation: true,
                    fg: pen_state.pen.fg,
                    bg: pen_state.pen.bg,
                    attrs: pen_state.pen.attrs,
                    protected: pen_state.pen.protected,
                    hyperlink: pen_state.active_hyperlink,
                },
            );
        }
    }

    /// Advances the cursor horizontally and sets `pending_wrap` when the
    /// cursor reaches the right margin with autowrap on.
    fn advance_cursor(
        cursor: &mut CursorEngine,
        width: usize,
        (_left_limit, right_limit): (usize, usize),
    ) {
        cursor.cursor.x += width;
        if cursor.cursor.x > right_limit {
            cursor.cursor.x = right_limit;
            if cursor.modes.autowrap {
                cursor.modes.pending_wrap = true;
            }
        }
    }

    // ── helpers for write_char ────────────────────────────────────

    /// Internal helper: handles the line-feed / index portion of newline for write_char.
    /// Returns whether the cursor moved or the region scrolled.
    fn newline_inner(
        pen_state: &mut PenState,
        normal: &mut NormalBuf,
        cursor: &mut CursorEngine,
    ) -> bool {
        let before = cursor.cursor.y;
        if cursor.index_needs_scroll() {
            CellOps::scroll_region_up_one_inner(pen_state, normal, cursor);
            true
        } else {
            cursor.index_advance(normal);
            cursor.cursor.y != before
        }
    }

    /// Clears the target cell and its joined cell, provided the complete glyph
    /// is inside the active horizontal bounds.
    pub(crate) fn clear_cell_for_write(
        normal: &mut NormalBuf,
        row: usize,
        col: usize,
        left: usize,
        right: usize,
    ) {
        if let Some((base, continuation)) = CellOps::wide_range(normal, row, col) {
            if base < left || continuation > right {
                return;
            }
            normal.erase_cell(row, base, crate::Cell::default());
            normal.erase_cell(row, continuation, crate::Cell::default());
            return;
        }

        normal.erase_cell(row, col, crate::Cell::default());
    }
}
