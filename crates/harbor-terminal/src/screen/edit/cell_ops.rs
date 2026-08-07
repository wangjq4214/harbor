//! Cell-level mutations: erase, insert, delete, scroll, and DEC rectangle ops.
//!
//! `CellOps` is a stateless namespace — all methods are associated functions
//! that take `&mut PenState`, `&mut NormalBuf`, and `&mut CursorEngine` (or
//! `&CursorEngine` for read-only cursor access).

use crate::normal_buf::NormalBuf;
use harbor_parser::Params;
use harbor_types::Cell;
use unicode_width::UnicodeWidthChar;

use super::super::cursor::CursorEngine;
use super::pen_state::PenState;

/// A rectangular region in display coordinates (0-based, inclusive).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Rect {
    pub(crate) top: usize,
    pub(crate) left: usize,
    pub(crate) bottom: usize,
    pub(crate) right: usize,
}

/// Stateless namespace for cell-level mutation operations.
pub(crate) struct CellOps;

impl CellOps {
    // ── wide-glyph helpers ────────────────────────────────────────

    /// Returns the complete valid wide-glyph range containing `col`.
    pub(crate) fn wide_range(normal: &NormalBuf, row: usize, col: usize) -> Option<(usize, usize)> {
        let cols = normal.cols();
        let cell = normal.cell(row, col);
        if cell.wide_continuation {
            let base = col.checked_sub(1)?;
            return (UnicodeWidthChar::width(normal.cell(row, base).ch).unwrap_or(0) == 2)
                .then_some((base, col));
        }
        (UnicodeWidthChar::width(cell.ch).unwrap_or(0) == 2
            && col + 1 < cols
            && normal.cell(row, col + 1).wide_continuation)
            .then_some((col, col + 1))
    }

    /// Expands an exclusive row range to include complete wide glyphs without
    /// crossing the supplied inclusive bounds, and marks every touched cell dirty.
    fn normalize_touched_range(
        normal: &mut NormalBuf,
        row: usize,
        start: usize,
        end: usize,
        left: usize,
        right: usize,
    ) -> (usize, usize) {
        let mut start = start.clamp(left, right + 1);
        let mut end = end.clamp(left, right + 1);
        if start >= end {
            return (start, end);
        }
        let mut col = start;
        while col < end {
            if let Some((base, continuation)) = Self::wide_range(normal, row, col)
                && base >= left
                && continuation <= right
            {
                start = start.min(base);
                end = end.max(continuation + 1);
            }
            col += 1;
        }
        normal.mark_range_dirty(row, start, end);
        (start, end)
    }

    /// Removes malformed wide-cell fragments in a bounded row region.
    fn normalize_row_region(
        pen_state: &PenState,
        normal: &mut NormalBuf,
        row: usize,
        left: usize,
        right: usize,
    ) {
        let mut col = left;
        while col <= right {
            if let Some((_, continuation)) = Self::wide_range(normal, row, col) {
                col = continuation + 1;
                continue;
            }
            let cell = normal.cell(row, col);
            if cell.wide_continuation || UnicodeWidthChar::width(cell.ch).unwrap_or(0) == 2 {
                *normal.cell_mut(row, col) = pen_state.erase_cell();
            }
            col += 1;
        }
    }

    fn has_boundary_wide(
        normal: &NormalBuf,
        row: usize,
        start: usize,
        end: usize,
        left: usize,
        right: usize,
    ) -> bool {
        (start..end).any(|col| {
            Self::wide_range(normal, row, col).is_some_and(|(base, continuation)| {
                (base < left || continuation > right) && base < end && continuation >= start
            })
        })
    }

    fn erase_row_range(
        pen_state: &PenState,
        normal: &mut NormalBuf,
        row: usize,
        (start, end): (usize, usize),
        (left, right): (usize, usize),
        selective: bool,
    ) {
        let (start, end) = Self::normalize_touched_range(normal, row, start, end, left, right);
        let erase = pen_state.erase_cell();
        let mut col = start;
        while col < end {
            if let Some((base, continuation)) = Self::wide_range(normal, row, col) {
                if base < left || continuation > right {
                    // Never erase only the in-bound half of a boundary glyph.
                    col = continuation + 1;
                    continue;
                }
                if !selective
                    || (!normal.cell(row, base).protected
                        && !normal.cell(row, continuation).protected)
                {
                    *normal.cell_mut(row, base) = erase;
                    *normal.cell_mut(row, continuation) = erase;
                }
                col = continuation + 1;
            } else {
                if !selective || !normal.cell(row, col).protected {
                    *normal.cell_mut(row, col) = erase;
                }
                col += 1;
            }
        }
        Self::normalize_row_region(pen_state, normal, row, left, right);
    }

    // ── margin-rect scroll helpers ────────────────────────────────

    pub(crate) fn scroll_margin_rect_up(
        pen_state: &PenState,
        normal: &mut NormalBuf,
        cursor: &CursorEngine,
        top: usize,
        bottom: usize,
        n: usize,
    ) {
        let height = bottom - top + 1;
        if n < height {
            for dst_row in top..=(bottom - n) {
                let src_row = dst_row + n;
                for col in cursor.margins.left..=cursor.margins.right {
                    let cell = *normal.cell(src_row, col);
                    *normal.cell_mut(dst_row, col) = cell;
                }
            }
        }
        let blank = pen_state.erase_cell();
        for row in (bottom + 1 - n)..=bottom {
            for col in cursor.margins.left..=cursor.margins.right {
                *normal.cell_mut(row, col) = blank;
            }
        }
        for row in top..=bottom {
            Self::normalize_row_region(
                pen_state,
                normal,
                row,
                cursor.margins.left,
                cursor.margins.right,
            );
        }
    }

    pub(crate) fn scroll_margin_rect_down(
        pen_state: &PenState,
        normal: &mut NormalBuf,
        cursor: &CursorEngine,
        top: usize,
        bottom: usize,
        n: usize,
    ) {
        let height = bottom - top + 1;
        if n < height {
            for dst_row in ((top + n)..=bottom).rev() {
                let src_row = dst_row - n;
                for col in cursor.margins.left..=cursor.margins.right {
                    let cell = *normal.cell(src_row, col);
                    *normal.cell_mut(dst_row, col) = cell;
                }
            }
        }
        let blank = pen_state.erase_cell();
        for row in top..(top + n) {
            for col in cursor.margins.left..=cursor.margins.right {
                *normal.cell_mut(row, col) = blank;
            }
        }
        for row in top..=bottom {
            Self::normalize_row_region(
                pen_state,
                normal,
                row,
                cursor.margins.left,
                cursor.margins.right,
            );
        }
    }

    // ── erase display / line / chars ──────────────────────────────

    pub(crate) fn erase_display(
        pen_state: &mut PenState,
        normal: &mut NormalBuf,
        cursor: &mut CursorEngine,
        mode: usize,
    ) {
        if cursor.margins.enabled
            && (cursor.cursor.x < cursor.margins.left || cursor.cursor.x > cursor.margins.right)
        {
            return;
        }
        cursor.modes.pending_wrap = false;
        let (left, right) = if cursor.margins.enabled {
            (cursor.margins.left, cursor.margins.right)
        } else {
            (0, normal.cols() - 1)
        };
        match mode {
            0 => {
                Self::erase_row_range(
                    pen_state,
                    normal,
                    cursor.cursor.y,
                    (cursor.cursor.x, right + 1),
                    (left, right),
                    false,
                );
                for row in cursor.cursor.y + 1..normal.rows() {
                    Self::erase_row_range(
                        pen_state,
                        normal,
                        row,
                        (left, right + 1),
                        (left, right),
                        false,
                    );
                }
            }
            1 => {
                for row in 0..cursor.cursor.y {
                    Self::erase_row_range(
                        pen_state,
                        normal,
                        row,
                        (left, right + 1),
                        (left, right),
                        false,
                    );
                }
                Self::erase_row_range(
                    pen_state,
                    normal,
                    cursor.cursor.y,
                    (left, cursor.cursor.x + 1),
                    (left, right),
                    false,
                );
            }
            2 => {
                for row in 0..normal.rows() {
                    Self::erase_row_range(
                        pen_state,
                        normal,
                        row,
                        (left, right + 1),
                        (left, right),
                        false,
                    );
                }
                cursor.home_cursor();
            }
            _ => {}
        }
    }

    pub(crate) fn erase_line(
        pen_state: &mut PenState,
        normal: &mut NormalBuf,
        cursor: &mut CursorEngine,
        mode: usize,
    ) {
        if cursor.margins.enabled
            && (cursor.cursor.x < cursor.margins.left || cursor.cursor.x > cursor.margins.right)
        {
            return;
        }
        cursor.modes.pending_wrap = false;
        let (left, right) = if cursor.margins.enabled {
            (cursor.margins.left, cursor.margins.right)
        } else {
            (0, normal.cols() - 1)
        };
        match mode {
            0 => Self::erase_row_range(
                pen_state,
                normal,
                cursor.cursor.y,
                (cursor.cursor.x, right + 1),
                (left, right),
                false,
            ),
            1 => Self::erase_row_range(
                pen_state,
                normal,
                cursor.cursor.y,
                (left, cursor.cursor.x + 1),
                (left, right),
                false,
            ),
            2 => Self::erase_row_range(
                pen_state,
                normal,
                cursor.cursor.y,
                (left, right + 1),
                (left, right),
                false,
            ),
            _ => return,
        }
        normal.mark_range_dirty(
            cursor.cursor.y,
            cursor.cursor.x.saturating_sub(1).max(left),
            right + 1,
        );
    }

    pub(crate) fn erase_chars(
        pen_state: &mut PenState,
        normal: &mut NormalBuf,
        cursor: &mut CursorEngine,
        n: usize,
    ) {
        if cursor.margins.enabled
            && (cursor.cursor.x < cursor.margins.left || cursor.cursor.x > cursor.margins.right)
        {
            return;
        }
        cursor.modes.pending_wrap = false;
        let right = if cursor.margins.enabled {
            cursor.margins.right
        } else {
            normal.cols() - 1
        };
        let left = if cursor.margins.enabled {
            cursor.margins.left
        } else {
            0
        };
        let end = (cursor.cursor.x + n.max(1)).min(right + 1);
        Self::erase_row_range(
            pen_state,
            normal,
            cursor.cursor.y,
            (cursor.cursor.x, end),
            (left, right),
            false,
        );
        normal.mark_range_dirty(
            cursor.cursor.y,
            cursor.cursor.x.saturating_sub(1).max(left),
            (end + 1).min(right + 1),
        );
    }

    // ── selective erase ───────────────────────────────────────────

    pub(crate) fn selective_erase_display(
        pen_state: &mut PenState,
        normal: &mut NormalBuf,
        cursor: &mut CursorEngine,
        mode: usize,
    ) {
        if cursor.margins.enabled
            && (cursor.cursor.x < cursor.margins.left || cursor.cursor.x > cursor.margins.right)
        {
            return;
        }
        let (left, right) = if cursor.margins.enabled {
            (cursor.margins.left, cursor.margins.right)
        } else {
            (0, normal.cols() - 1)
        };
        match mode {
            0 => {
                Self::erase_row_range(
                    pen_state,
                    normal,
                    cursor.cursor.y,
                    (cursor.cursor.x, right + 1),
                    (left, right),
                    true,
                );
                for row in cursor.cursor.y + 1..normal.rows() {
                    Self::erase_row_range(
                        pen_state,
                        normal,
                        row,
                        (left, right + 1),
                        (left, right),
                        true,
                    );
                }
            }
            1 => {
                for row in 0..cursor.cursor.y {
                    Self::erase_row_range(
                        pen_state,
                        normal,
                        row,
                        (left, right + 1),
                        (left, right),
                        true,
                    );
                }
                Self::erase_row_range(
                    pen_state,
                    normal,
                    cursor.cursor.y,
                    (left, cursor.cursor.x + 1),
                    (left, right),
                    true,
                );
            }
            2 => {
                for row in 0..normal.rows() {
                    Self::erase_row_range(
                        pen_state,
                        normal,
                        row,
                        (left, right + 1),
                        (left, right),
                        true,
                    );
                }
            }
            _ => {}
        }
    }

    pub(crate) fn selective_erase_line(
        pen_state: &mut PenState,
        normal: &mut NormalBuf,
        cursor: &mut CursorEngine,
        mode: usize,
    ) {
        if cursor.margins.enabled
            && (cursor.cursor.x < cursor.margins.left || cursor.cursor.x > cursor.margins.right)
        {
            return;
        }
        let (left, right) = if cursor.margins.enabled {
            (cursor.margins.left, cursor.margins.right)
        } else {
            (0, normal.cols() - 1)
        };
        match mode {
            0 => Self::erase_row_range(
                pen_state,
                normal,
                cursor.cursor.y,
                (cursor.cursor.x, right + 1),
                (left, right),
                true,
            ),
            1 => Self::erase_row_range(
                pen_state,
                normal,
                cursor.cursor.y,
                (left, cursor.cursor.x + 1),
                (left, right),
                true,
            ),
            2 => Self::erase_row_range(
                pen_state,
                normal,
                cursor.cursor.y,
                (left, right + 1),
                (left, right),
                true,
            ),
            _ => {}
        }
    }

    // ── insert / delete chars ─────────────────────────────────────

    pub(crate) fn insert_chars(
        pen_state: &mut PenState,
        normal: &mut NormalBuf,
        cursor: &mut CursorEngine,
        n: usize,
    ) -> bool {
        cursor.modes.pending_wrap = false;
        let n = if n == 0 { 1 } else { n };
        let requested_col = cursor.cursor.x;
        let (left, right) = if cursor.margins.enabled {
            (cursor.margins.left, cursor.margins.right)
        } else {
            (0, normal.cols() - 1)
        };
        normal.mark_range_dirty(cursor.cursor.y, requested_col.saturating_sub(1), right + 1);
        if requested_col < left || requested_col > right {
            return false;
        }
        let col = Self::wide_range(normal, cursor.cursor.y, requested_col)
            .map_or(requested_col, |(base, _)| base);
        let n = n.min(right - col + 1);
        if n == 0 {
            return false;
        }
        if Self::has_boundary_wide(normal, cursor.cursor.y, left, right + 1, left, right) {
            return false;
        }
        let ring_row = normal.display_to_ring(cursor.cursor.y);
        let row_start = ring_row * normal.cols();

        let src_start = row_start + col;
        let src_end = row_start + right - n + 1;
        let dst = row_start + col + n;
        if src_start < src_end {
            normal.copy_linear_range(src_start, src_end, dst);
        }

        normal.fill_linear_range_with(row_start + col, row_start + col + n, pen_state.erase_cell());
        Self::normalize_row_region(pen_state, normal, cursor.cursor.y, left, right);
        true
    }

    pub(crate) fn delete_chars(
        pen_state: &mut PenState,
        normal: &mut NormalBuf,
        cursor: &mut CursorEngine,
        n: usize,
    ) {
        cursor.modes.pending_wrap = false;
        let n = if n == 0 { 1 } else { n };
        let col = cursor.cursor.x;
        let (left, right) = if cursor.margins.enabled {
            (cursor.margins.left, cursor.margins.right)
        } else {
            (0, normal.cols() - 1)
        };
        normal.mark_range_dirty(cursor.cursor.y, col.saturating_sub(1), right + 1);
        if col < left || col > right {
            return;
        }
        let n = n.min(right - col + 1);
        if n == 0 {
            return;
        }
        if Self::has_boundary_wide(normal, cursor.cursor.y, left, right + 1, left, right) {
            return;
        }
        let (delete_start, delete_end) =
            Self::normalize_touched_range(normal, cursor.cursor.y, col, col + n, left, right);
        let delete_width = delete_end - delete_start;
        let ring_row = normal.display_to_ring(cursor.cursor.y);
        let row_start = ring_row * normal.cols();
        let src_start = row_start + delete_end;
        let src_end = row_start + right + 1;
        let dst = row_start + delete_start;
        if src_start < src_end {
            normal.copy_linear_range(src_start, src_end, dst);
        }
        let blank_start = row_start + right + 1 - delete_width;
        normal.fill_linear_range_with(
            blank_start,
            blank_start + delete_width,
            pen_state.erase_cell(),
        );
        Self::normalize_row_region(pen_state, normal, cursor.cursor.y, left, right);
    }

    // ── insert / delete lines ─────────────────────────────────────

    pub(crate) fn insert_lines(
        pen_state: &mut PenState,
        normal: &mut NormalBuf,
        cursor: &mut CursorEngine,
        n: usize,
    ) {
        let n = if n == 0 { 1 } else { n };
        if cursor.cursor.y < cursor.scroll_region.top
            || cursor.cursor.y > cursor.scroll_region.bottom
        {
            return;
        }
        let max_n = cursor.scroll_region.bottom - cursor.cursor.y + 1;
        let n = n.min(max_n);
        normal.mark_rows_dirty(
            cursor.cursor.y,
            cursor.scroll_region.bottom.saturating_add(1),
        );
        if cursor.margins.enabled {
            Self::scroll_margin_rect_down(
                pen_state,
                normal,
                cursor,
                cursor.cursor.y,
                cursor.scroll_region.bottom,
                n,
            );
            cursor.cursor.x = 0;
            return;
        }
        if n == max_n {
            for row in cursor.cursor.y..=cursor.scroll_region.bottom {
                normal.fill_row_with(row, pen_state.erase_cell());
            }
            cursor.cursor.x = 0;
            return;
        }
        let tr = normal.total_rows();
        let vis = normal.visible_start();
        let c = normal.cols();
        let src_start = ((vis + cursor.cursor.y) % tr) * c;
        let src_end = ((vis + cursor.scroll_region.bottom - n + 1) % tr) * c;
        let dst = ((vis + cursor.cursor.y + n) % tr) * c;
        normal.copy_ring_range(src_start, src_end, dst);
        for i in 0..n {
            normal.fill_row_with(cursor.cursor.y + i, pen_state.erase_cell());
        }
        for row in cursor.cursor.y..=cursor.scroll_region.bottom {
            Self::normalize_row_region(pen_state, normal, row, 0, normal.cols() - 1);
        }
        cursor.cursor.x = 0;
    }

    pub(crate) fn delete_lines(
        pen_state: &mut PenState,
        normal: &mut NormalBuf,
        cursor: &mut CursorEngine,
        n: usize,
    ) {
        let n = if n == 0 { 1 } else { n };
        if cursor.cursor.y < cursor.scroll_region.top
            || cursor.cursor.y > cursor.scroll_region.bottom
        {
            return;
        }
        let max_n = cursor.scroll_region.bottom - cursor.cursor.y + 1;
        let n = n.min(max_n);
        normal.mark_rows_dirty(
            cursor.cursor.y,
            cursor.scroll_region.bottom.saturating_add(1),
        );
        if cursor.margins.enabled {
            Self::scroll_margin_rect_up(
                pen_state,
                normal,
                cursor,
                cursor.cursor.y,
                cursor.scroll_region.bottom,
                n,
            );
            cursor.cursor.x = 0;
            return;
        }
        if n == max_n {
            for row in cursor.cursor.y..=cursor.scroll_region.bottom {
                normal.fill_row_with(row, pen_state.erase_cell());
            }
            cursor.cursor.x = 0;
            return;
        }
        let tr = normal.total_rows();
        let vis = normal.visible_start();
        let c = normal.cols();
        let src_start = ((vis + cursor.cursor.y + n) % tr) * c;
        let src_end = ((vis + cursor.scroll_region.bottom + 1) % tr) * c;
        let dst = ((vis + cursor.cursor.y) % tr) * c;
        normal.copy_ring_range(src_start, src_end, dst);
        for i in 0..n {
            normal.fill_row_with(cursor.scroll_region.bottom - i, pen_state.erase_cell());
        }
        for row in cursor.cursor.y..=cursor.scroll_region.bottom {
            Self::normalize_row_region(pen_state, normal, row, 0, normal.cols() - 1);
        }
        cursor.cursor.x = 0;
    }

    // ── scroll region ─────────────────────────────────────────────

    pub(crate) fn scroll_up_region(
        pen_state: &mut PenState,
        normal: &mut NormalBuf,
        cursor: &mut CursorEngine,
        n: usize,
    ) {
        let n = if n == 0 { 1 } else { n };
        let region_height = cursor.scroll_region.bottom - cursor.scroll_region.top + 1;
        let n = n.min(region_height);
        for row in cursor.scroll_region.top..=cursor.scroll_region.bottom {
            normal.mark_row_dirty(row);
        }
        if cursor.margins.enabled {
            Self::scroll_margin_rect_up(
                pen_state,
                normal,
                cursor,
                cursor.scroll_region.top,
                cursor.scroll_region.bottom,
                n,
            );
            return;
        }
        if n == region_height {
            for row in cursor.scroll_region.top..=cursor.scroll_region.bottom {
                normal.fill_row_with(row, pen_state.erase_cell());
            }
            return;
        }
        let tr = normal.total_rows();
        let vis = normal.visible_start();
        let c = normal.cols();
        let src_start = ((vis + cursor.scroll_region.top + n) % tr) * c;
        let src_end = ((vis + cursor.scroll_region.bottom + 1) % tr) * c;
        let dst = ((vis + cursor.scroll_region.top) % tr) * c;
        normal.copy_ring_range(src_start, src_end, dst);
        for i in 0..n {
            normal.fill_row_with(cursor.scroll_region.bottom - i, pen_state.erase_cell());
        }
        for row in cursor.scroll_region.top..=cursor.scroll_region.bottom {
            Self::normalize_row_region(pen_state, normal, row, 0, normal.cols() - 1);
        }
    }

    pub(crate) fn scroll_down_region(
        pen_state: &mut PenState,
        normal: &mut NormalBuf,
        cursor: &mut CursorEngine,
        n: usize,
    ) {
        let n = if n == 0 { 1 } else { n };
        let region_height = cursor.scroll_region.bottom - cursor.scroll_region.top + 1;
        let n = n.min(region_height);
        for row in cursor.scroll_region.top..=cursor.scroll_region.bottom {
            normal.mark_row_dirty(row);
        }
        if cursor.margins.enabled {
            Self::scroll_margin_rect_down(
                pen_state,
                normal,
                cursor,
                cursor.scroll_region.top,
                cursor.scroll_region.bottom,
                n,
            );
            return;
        }
        if n == region_height {
            for row in cursor.scroll_region.top..=cursor.scroll_region.bottom {
                normal.fill_row_with(row, pen_state.erase_cell());
            }
            return;
        }
        let tr = normal.total_rows();
        let vis = normal.visible_start();
        let c = normal.cols();
        let src_start = ((vis + cursor.scroll_region.top) % tr) * c;
        let src_end = ((vis + cursor.scroll_region.bottom - n + 1) % tr) * c;
        let dst = ((vis + cursor.scroll_region.top + n) % tr) * c;
        normal.copy_ring_range(src_start, src_end, dst);
        for i in 0..n {
            normal.fill_row_with(cursor.scroll_region.top + i, pen_state.erase_cell());
        }
        for row in cursor.scroll_region.top..=cursor.scroll_region.bottom {
            Self::normalize_row_region(pen_state, normal, row, 0, normal.cols() - 1);
        }
    }

    // ── internal scroll_region_up_one ─────────────────────────────

    pub(crate) fn scroll_region_up_one_inner(
        pen_state: &mut PenState,
        normal: &mut NormalBuf,
        cursor: &mut CursorEngine,
    ) {
        normal.mark_rows_dirty(
            cursor.scroll_region.top,
            cursor.scroll_region.bottom.saturating_add(1),
        );
        if cursor.margins.enabled {
            Self::scroll_margin_rect_up(
                pen_state,
                normal,
                cursor,
                cursor.scroll_region.top,
                cursor.scroll_region.bottom,
                1,
            );
        } else if cursor.scroll_region.top == 0 && cursor.scroll_region.bottom == normal.rows() - 1
        {
            normal.scroll_up_full_screen(1, pen_state.erase_cell());
        } else {
            let tr = normal.total_rows();
            let vis = normal.visible_start();
            let c = normal.cols();
            let src_start = ((vis + cursor.scroll_region.top + 1) % tr) * c;
            let src_end = ((vis + cursor.scroll_region.bottom + 1) % tr) * c;
            let dst = ((vis + cursor.scroll_region.top) % tr) * c;
            normal.copy_ring_range(src_start, src_end, dst);
            normal.fill_row_with(cursor.scroll_region.bottom, pen_state.erase_cell());
        }
        if !cursor.margins.enabled {
            for row in cursor.scroll_region.top..=cursor.scroll_region.bottom {
                Self::normalize_row_region(pen_state, normal, row, 0, normal.cols() - 1);
            }
        }
        cursor.cursor.y = cursor.scroll_region.bottom;
    }

    // ── DEC rectangle operations ──────────────────────────────────

    pub(crate) fn decera(
        pen_state: &mut PenState,
        normal: &mut NormalBuf,
        cursor: &CursorEngine,
        params: &Params,
    ) {
        let top = params.get_or(0, 0);
        let left = params.get_or(1, 0);
        let bottom = params.get_or(2, 0);
        let right = params.get_or(3, 0);

        let Some(Rect {
            top: t,
            left: l,
            bottom: b,
            right: r,
        }) = cursor.resolve_rect(normal, top, left, bottom, right)
        else {
            return;
        };
        for row in t..=b {
            Self::erase_row_range(
                pen_state,
                normal,
                row,
                (l, r + 1),
                (0, normal.cols() - 1),
                false,
            );
        }
    }

    pub(crate) fn decsera(
        pen_state: &mut PenState,
        normal: &mut NormalBuf,
        cursor: &CursorEngine,
        params: &Params,
    ) {
        let top = params.get_or(0, 0);
        let left = params.get_or(1, 0);
        let bottom = params.get_or(2, 0);
        let right = params.get_or(3, 0);

        let Some(Rect {
            top: t,
            left: l,
            bottom: b,
            right: r,
        }) = cursor.resolve_rect(normal, top, left, bottom, right)
        else {
            return;
        };
        for row in t..=b {
            Self::erase_row_range(
                pen_state,
                normal,
                row,
                (l, r + 1),
                (0, normal.cols() - 1),
                true,
            );
        }
    }

    pub(crate) fn decfra(
        pen_state: &PenState,
        normal: &mut NormalBuf,
        cursor: &CursorEngine,
        params: &Params,
    ) {
        let ch_val = params.get_or(0, 0);
        let top = params.get_or(1, 0);
        let left = params.get_or(2, 0);
        let bottom = params.get_or(3, 0);
        let right = params.get_or(4, 0);

        let Some(Rect {
            top: t,
            left: l,
            bottom: b,
            right: r,
        }) = cursor.resolve_rect(normal, top, left, bottom, right)
        else {
            return;
        };
        let fill_char = if (32..=126).contains(&ch_val) || (160..=255).contains(&ch_val) {
            (ch_val as u8) as char
        } else {
            ' '
        };

        let cell = Cell {
            ch: fill_char,
            wide_continuation: false,
            fg: pen_state.pen.fg,
            bg: pen_state.pen.bg,
            attrs: pen_state.pen.attrs,
            protected: pen_state.pen.protected,
        };

        for row in t..=b {
            let (start, end) =
                Self::normalize_touched_range(normal, row, l, r + 1, 0, normal.cols() - 1);
            for col in start..end {
                *normal.cell_mut(row, col) = cell;
            }
            Self::normalize_row_region(pen_state, normal, row, 0, normal.cols() - 1);
        }
    }

    pub(crate) fn deccra(
        pen_state: &PenState,
        normal: &mut NormalBuf,
        cursor: &CursorEngine,
        params: &Params,
    ) {
        let src_top = params.get_or(0, 0);
        let src_left = params.get_or(1, 0);
        let src_bottom = params.get_or(2, 0);
        let src_right = params.get_or(3, 0);
        let dest_top = params.get_or(5, 0);
        let dest_left = params.get_or(6, 0);

        let Some(Rect {
            top: st,
            left: sl,
            bottom: sb,
            right: sr,
        }) = cursor.resolve_rect(normal, src_top, src_left, src_bottom, src_right)
        else {
            return;
        };

        let dt_start = if cursor.modes.origin {
            let r = dest_top.saturating_sub(1);
            cursor.scroll_region.top + r
        } else {
            dest_top.saturating_sub(1)
        };
        let dl_start = if cursor.modes.origin {
            let c = dest_left.saturating_sub(1);
            cursor.margins.left + c
        } else {
            dest_left.saturating_sub(1)
        };

        let height = sb - st + 1;
        let width = sr - sl + 1;

        let erase = pen_state.erase_cell();
        let mut temp = Vec::with_capacity(height * width);
        for row in st..=sb {
            for col in sl..=sr {
                let complete = match Self::wide_range(normal, row, col) {
                    Some((base, continuation)) => base >= sl && continuation <= sr,
                    None => {
                        !normal.cell(row, col).wide_continuation
                            && UnicodeWidthChar::width(normal.cell(row, col).ch).unwrap_or(0) != 2
                    }
                };
                temp.push(if complete {
                    *normal.cell(row, col)
                } else {
                    erase
                });
            }
        }

        let (dest_top, dest_bottom, dest_left, dest_right) = if cursor.modes.origin {
            (
                cursor.scroll_region.top,
                cursor.scroll_region.bottom,
                cursor.margins.left,
                cursor.margins.right,
            )
        } else {
            (0, normal.rows() - 1, 0, normal.cols() - 1)
        };
        let Some(dest_row_end) = dt_start.checked_add(height) else {
            return;
        };
        let Some(dest_col_end) = dl_start.checked_add(width) else {
            return;
        };
        let row_start = dt_start.max(dest_top);
        let row_end = dest_row_end.min(dest_bottom + 1);
        let col_start = dl_start.max(dest_left);
        let col_end = dest_col_end.min(dest_right + 1);
        if row_start >= row_end || col_start >= col_end {
            return;
        }

        for dest_row in row_start..row_end {
            let h = dest_row - dt_start;
            Self::erase_row_range(
                pen_state,
                normal,
                dest_row,
                (col_start, col_end),
                (dest_left, dest_right),
                false,
            );
            for dest_col in col_start..col_end {
                let w = dest_col - dl_start;
                if Self::wide_range(normal, dest_row, dest_col).is_some_and(
                    |(base, continuation)| base < dest_left || continuation > dest_right,
                ) {
                    continue;
                }
                let mut cell = temp[h * width + w];
                if cell.wide_continuation || UnicodeWidthChar::width(cell.ch).unwrap_or(0) == 2 {
                    let pair_in_bounds = if cell.wide_continuation {
                        dest_col > col_start && dest_col > dest_left
                    } else {
                        dest_col + 1 < col_end && dest_col < dest_right
                    };
                    if !pair_in_bounds {
                        cell = erase;
                    }
                }
                *normal.cell_mut(dest_row, dest_col) = cell;
            }
            Self::normalize_row_region(pen_state, normal, dest_row, dest_left, dest_right);
        }
    }

    pub(crate) fn deccara(normal: &mut NormalBuf, cursor: &CursorEngine, params: &Params) {
        let top = params.get_or(0, 0);
        let left = params.get_or(1, 0);
        let bottom = params.get_or(2, 0);
        let right = params.get_or(3, 0);

        let Some(Rect {
            top: t,
            left: l,
            bottom: b,
            right: r,
        }) = cursor.resolve_rect(normal, top, left, bottom, right)
        else {
            return;
        };

        for row in t..=b {
            let (start, end) =
                Self::normalize_touched_range(normal, row, l, r + 1, 0, normal.cols() - 1);
            for col in start..end {
                let cell = normal.cell_mut(row, col);
                for code in params.iter_flat().skip(4).flatten() {
                    cell.apply_sgr(code);
                }
            }
        }
    }

    pub(crate) fn decrara(normal: &mut NormalBuf, cursor: &CursorEngine, params: &Params) {
        let top = params.get_or(0, 0);
        let left = params.get_or(1, 0);
        let bottom = params.get_or(2, 0);
        let right = params.get_or(3, 0);

        let Some(Rect {
            top: t,
            left: l,
            bottom: b,
            right: r,
        }) = cursor.resolve_rect(normal, top, left, bottom, right)
        else {
            return;
        };

        for row in t..=b {
            let (start, end) =
                Self::normalize_touched_range(normal, row, l, r + 1, 0, normal.cols() - 1);
            for col in start..end {
                let cell = normal.cell_mut(row, col);
                for code in params.iter_flat().skip(4).flatten() {
                    cell.toggle_sgr(code);
                }
            }
        }
    }
}
