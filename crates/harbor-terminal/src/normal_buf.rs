use crate::damage::{DamageTracker, DirtyRange};
use crate::screen::Cell;
use harbor_config::HistoryLimits;

/// Stable identity of retained content belonging to one logical terminal line.
///
/// IDs are local to one `NormalBuf`; they are never derived from ring positions
/// and are never reused by that buffer.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LogicalLineId(pub u64);

/// Metadata carried atomically with one physical ring row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RowMetadata {
    pub(crate) logical_line_id: LogicalLineId,
    /// Absolute cell offset of this physical row within its logical line.
    pub(crate) logical_start: usize,
    /// Absolute logical-atom offset of this physical row within its logical line.
    pub(crate) logical_atom_start: usize,
    pub(crate) meaningful_extent: usize,
    pub(crate) soft_wrapped: bool,
    pub(crate) head_truncated: bool,
}

/// Compact retained-content provenance for one cell.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct CellState(u8);

impl CellState {
    const EXPLICIT: u8 = 1 << 0;
    const STYLE_VISIBLE: u8 = 1 << 1;

    fn explicit(cell: &Cell) -> Self {
        Self(Self::EXPLICIT | Self::style_visible_bit(cell))
    }

    pub(crate) fn erase(cell: &Cell) -> Self {
        Self(Self::style_visible_bit(cell))
    }

    fn fresh_fill(cell: &Cell) -> Self {
        if cell.ch != ' ' || cell.wide_continuation || cell.hyperlink.is_some() {
            Self::explicit(cell)
        } else {
            Self::erase(cell)
        }
    }

    fn with_recomputed_style(self, cell: &Cell) -> Self {
        Self((self.0 & Self::EXPLICIT) | Self::style_visible_bit(cell))
    }

    fn style_visible_bit(cell: &Cell) -> u8 {
        if cell.is_visibly_meaningful_blank() {
            Self::STYLE_VISIBLE
        } else {
            0
        }
    }

    pub(crate) fn is_meaningful(self) -> bool {
        self.0 != 0
    }

    pub(crate) fn is_explicit(self) -> bool {
        self.0 & Self::EXPLICIT != 0
    }
}

/// Semantic view of one retained ring row in generation order.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RetainedRow<'a> {
    pub(crate) generation: u64,
    pub(crate) metadata: RowMetadata,
    pub(crate) cells: &'a [Cell],
    pub(crate) cell_state: &'a [CellState],
}

/// Ring-buffer backed scrollback buffer.
///
/// Storage is a single `Vec<Cell>` of `total_rows * cols` elements.
/// Visible rows occupy `visible_rows` consecutive rows in the ring starting
/// at `visible_start`.  When the viewport is at the live bottom
/// (`view_offset == 0`), the ring head advances O(1) on full-screen scroll
/// — no cell copies, just a pointer bump and blank-fill of the newly
/// exposed row(s).
#[derive(Clone, Debug)]
pub struct NormalBuf {
    /// Ring buffer: `total_rows * cols` cells — accessible via helper methods.
    cells: Vec<Cell>,
    /// Compact provenance and style visibility state parallel to `cells`.
    cell_state: Vec<CellState>,
    /// One metadata record per physical ring row.
    row_metadata: Vec<RowMetadata>,
    /// Read-model generation and semantic revisions, independent of renderer damage.
    content_generation: u64,
    content_revision: u64,
    row_versions: Vec<u64>,
    /// Visible projection cache; never uses or consumes damage as freshness.
    snapshot_cache: std::cell::RefCell<crate::snapshot::SnapshotCache>,
    /// Next never-before-issued logical line identity.
    next_logical_line_id: u64,
    /// Ring capacity in rows = `max_scrollback + visible_rows`.
    total_rows: usize,
    /// Viewport height (visible row count).
    visible_rows: usize,
    /// Number of columns.
    cols: usize,
    /// Ring index of the first visible display row.
    visible_start: usize,
    /// Number of saved scrollback rows (0 ..= max_scrollback).
    scroll_count: usize,
    /// View offset from live bottom: 0 = bottom (live), >0 = scrolled back.
    view_offset: usize,
    /// Damage tracker.
    damage_tracker: DamageTracker,
    /// Maximum physical scrollback row count.
    max_scrollback: usize,
    max_history_bytes: usize,
    history_bytes: usize,
    /// Monotonically increasing scrollback generation base.
    /// Incremented when ring-buffer wraparound evicts old rows.
    history_start: u64,
}

// Buffer equality describes storage/content, not read-model provenance. Detached
// preparations of the same input must stay deterministic despite fresh identities.
impl PartialEq for NormalBuf {
    fn eq(&self, other: &Self) -> bool {
        self.cells == other.cells
            && self.cell_state == other.cell_state
            && self.row_metadata == other.row_metadata
            && self.next_logical_line_id == other.next_logical_line_id
            && self.total_rows == other.total_rows
            && self.visible_rows == other.visible_rows
            && self.cols == other.cols
            && self.visible_start == other.visible_start
            && self.scroll_count == other.scroll_count
            && self.view_offset == other.view_offset
            && self.damage_tracker == other.damage_tracker
            && self.max_scrollback == other.max_scrollback
            && self.max_history_bytes == other.max_history_bytes
            && self.history_bytes == other.history_bytes
            && self.history_start == other.history_start
    }
}
impl Eq for NormalBuf {}
impl NormalBuf {
    pub(crate) fn snapshot_cells(&self) -> crate::SnapshotCells {
        self.snapshot_cache.borrow_mut().project(self)
    }

    fn touch_ring_row(&mut self, ring_row: usize) {
        self.content_revision = self
            .content_revision
            .checked_add(1)
            .expect("terminal content revision exhausted");
        self.row_versions[ring_row] = self.content_revision;
    }

    pub(crate) fn content_generation(&self) -> u64 {
        self.content_generation
    }
    pub(crate) fn content_revision(&self) -> u64 {
        self.content_revision
    }

    /// Constant-time retained row access; no preceding row decoding or allocation.
    pub(crate) fn retained_row(&self, generation: u64) -> Option<RetainedRow<'_>> {
        let ring_row = self.ring_row_at_generation(generation)?;
        let start = ring_row * self.cols;
        Some(RetainedRow {
            generation,
            metadata: self.row_metadata[ring_row],
            cells: &self.cells[start..start + self.cols],
            cell_state: &self.cell_state[start..start + self.cols],
        })
    }

    pub(crate) fn row_version(&self, generation: u64) -> Option<u64> {
        Some(self.row_versions[self.ring_row_at_generation(generation)?])
    }

    pub fn new(rows: usize, cols: usize) -> Self {
        Self::with_history_limits(rows, cols, HistoryLimits::default())
    }

    pub fn with_history_limits(rows: usize, cols: usize, limits: HistoryLimits) -> Self {
        assert!(limits.is_valid(), "invalid history limits");
        let max_scrollback = limits.max_rows;
        let rows = rows.max(1);
        let cols = cols.max(1);
        let total_rows = max_scrollback
            .checked_add(rows)
            .expect("terminal row count overflow");
        let cell_count = total_rows
            .checked_mul(cols)
            .expect("terminal cell count overflow");
        let next_logical_line_id =
            u64::try_from(total_rows).expect("terminal row count exceeds logical ID space");
        let row_metadata = (0..total_rows)
            .map(|id| RowMetadata {
                logical_line_id: LogicalLineId(id as u64),
                logical_start: 0,
                logical_atom_start: 0,
                meaningful_extent: 0,
                soft_wrapped: false,
                head_truncated: false,
            })
            .collect();
        Self {
            total_rows,
            cells: vec![Cell::default(); cell_count],
            cell_state: vec![CellState::default(); cell_count],
            row_metadata,
            content_generation: crate::read_model::fresh_identity(),
            content_revision: 0,
            row_versions: vec![0; total_rows],
            snapshot_cache: Default::default(),
            next_logical_line_id,
            visible_rows: rows,
            cols,
            visible_start: max_scrollback,
            scroll_count: 0,
            max_scrollback,
            max_history_bytes: limits.max_bytes,
            history_bytes: 0,
            view_offset: 0,
            history_start: 0,
            damage_tracker: DamageTracker::new(rows, cols),
        }
    }

    #[cfg(test)]
    fn with_max_scrollback(rows: usize, cols: usize, max_scrollback: usize) -> Self {
        Self::with_history_limits(
            rows,
            cols,
            HistoryLimits {
                max_rows: max_scrollback,
                ..HistoryLimits::default()
            },
        )
    }
    #[cfg(test)]
    pub(crate) fn new_for_test(rows: usize, cols: usize, max_scrollback: usize) -> Self {
        Self::with_max_scrollback(rows, cols, max_scrollback)
    }

    // ── read-only accessors ─────────────────────────────────────────

    pub fn rows(&self) -> usize {
        self.visible_rows
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn view_offset(&self) -> usize {
        self.view_offset
    }

    pub fn scroll_count(&self) -> usize {
        self.scroll_count
    }

    pub fn is_scrolled_back(&self) -> bool {
        self.view_offset > 0
    }

    pub(crate) fn total_rows(&self) -> usize {
        self.total_rows
    }

    pub(crate) fn visible_start(&self) -> usize {
        self.visible_start
    }

    pub fn history_start(&self) -> u64 {
        self.history_start
    }
    #[cfg(test)]
    pub(crate) fn max_scrollback(&self) -> usize {
        self.max_scrollback
    }
    pub fn history_limits(&self) -> HistoryLimits {
        HistoryLimits {
            max_rows: self.max_scrollback,
            max_bytes: self.max_history_bytes,
        }
    }

    pub fn history_content_bytes(&self) -> usize {
        self.history_bytes
    }

    /// Only retained source cells count: unwritten tail padding and wide continuation
    /// slots have no source bytes. Interior and explicit/styled spaces do count.
    pub(crate) fn row_source_bytes<C: crate::primary_reflow::ReflowCell>(
        cells: &[C],
        extent: usize,
    ) -> usize {
        cells[..extent]
            .iter()
            .map(crate::primary_reflow::ReflowCell::source_bytes)
            .sum()
    }

    fn ring_source_bytes(&self, row: usize) -> usize {
        let start = row * self.cols;
        Self::row_source_bytes(
            &self.cells[start..start + self.cols],
            self.row_metadata[row].meaningful_extent,
        )
    }

    fn evict_oldest_history_row(&mut self) {
        debug_assert!(self.scroll_count > 0);
        let oldest = (self.visible_start + self.total_rows - self.scroll_count) % self.total_rows;
        let next = (oldest + 1) % self.total_rows;
        self.history_bytes -= self.ring_source_bytes(oldest);
        if self.row_metadata[oldest].logical_line_id == self.row_metadata[next].logical_line_id
            && self.row_metadata[next].soft_wrapped
        {
            self.row_metadata[next].head_truncated = true;
            self.touch_ring_row(next);
        }
        // Release evicted suffix allocations as well as removing their identity.
        let start = oldest * self.cols;
        self.cells[start..start + self.cols].fill(Cell::default());
        self.cell_state[start..start + self.cols].fill(CellState::default());
        // Even deleting independent hard lines changes the retained read/search scope.
        self.touch_ring_row(oldest);
        self.scroll_count -= 1;
        self.history_start += 1;
    }

    fn enforce_history_limits(&mut self) {
        while self.scroll_count > 0
            && (self.scroll_count > self.max_scrollback
                || self.history_bytes > self.max_history_bytes)
        {
            self.evict_oldest_history_row();
        }
        self.view_offset = self.view_offset.min(self.scroll_count);
    }

    /// Erases saved rows without moving or replacing the live grid or its identities.
    pub(crate) fn clear_scrollback(&mut self) {
        if self.scroll_count == 0 {
            return;
        }
        while self.scroll_count > 0 {
            self.evict_oldest_history_row();
        }
        debug_assert_eq!(self.history_bytes, 0);
        self.view_offset = 0;
        self.mark_all_dirty();
    }

    /// Detached retention-only rebuild. Geometry and source generations do not change.
    pub(crate) fn prepare_history_limits(
        &self,
        limits: HistoryLimits,
    ) -> Result<Self, crate::primary_reflow::PreparationError> {
        use crate::primary_reflow::{PreparationError, ReflowedRow};
        let mut rows = Vec::new();
        rows.try_reserve_exact(self.scroll_count + self.visible_rows)
            .map_err(|_| PreparationError::AllocationFailed)?;
        for row in self.retained_rows() {
            rows.push(ReflowedRow {
                cells: row.cells.to_vec(),
                cell_state: row.cell_state.to_vec(),
                metadata: row.metadata,
            });
        }
        let mut dropped = self.scroll_count.saturating_sub(limits.max_rows);
        let mut bytes: usize = rows[dropped..self.scroll_count]
            .iter()
            .map(|row| Self::row_source_bytes(&row.cells, row.metadata.meaningful_extent))
            .sum();
        while bytes > limits.max_bytes && dropped < self.scroll_count {
            bytes -= Self::row_source_bytes(
                &rows[dropped].cells,
                rows[dropped].metadata.meaningful_extent,
            );
            dropped += 1;
        }
        if dropped > 0
            && rows[dropped - 1].metadata.logical_line_id == rows[dropped].metadata.logical_line_id
        {
            rows[dropped].metadata.head_truncated = true;
        }
        Self::from_reflowed_rows(
            &rows[dropped..],
            self.visible_rows,
            self.cols,
            limits,
            self.next_logical_line_id,
            self.history_start + dropped as u64,
            self.view_offset,
        )
    }

    pub(crate) fn fill_is_meaningful(cell: Cell) -> bool {
        CellState::fresh_fill(&cell).is_meaningful()
    }

    // ── row/col accessors (for write_char, avoiding manual index math) ──

    fn allocate_logical_line_id(&mut self) -> LogicalLineId {
        let id = LogicalLineId(self.next_logical_line_id);
        self.next_logical_line_id = self
            .next_logical_line_id
            .checked_add(1)
            .expect("logical line ID space exhausted");
        id
    }

    fn fresh_metadata(&mut self) -> RowMetadata {
        RowMetadata {
            logical_line_id: self.allocate_logical_line_id(),
            logical_start: 0,
            logical_atom_start: 0,
            meaningful_extent: 0,
            soft_wrapped: false,
            head_truncated: false,
        }
    }

    fn normalize_wide_row(cells: &mut [Cell], states: &mut [CellState]) {
        let mut col = 0;
        while col < cells.len() {
            if cells[col].wide_continuation {
                cells[col] = Cell::default();
                states[col] = CellState::default();
                col += 1;
            } else if cells[col].grid_width() == 2 {
                if col + 1 < cells.len() && cells[col + 1].wide_continuation {
                    col += 2;
                } else {
                    cells[col] = Cell::default();
                    states[col] = CellState::default();
                    col += 1;
                }
            } else {
                col += 1;
            }
        }
    }

    fn recompute_ring_row_extent(&mut self, ring_row: usize) {
        self.touch_ring_row(ring_row);
        let start = ring_row * self.cols;
        let extent = self.cell_state[start..start + self.cols]
            .iter()
            .rposition(|state| state.is_meaningful())
            .map_or(0, |col| col + 1);
        self.row_metadata[ring_row].meaningful_extent = extent;
    }

    fn logical_atom_count(&self, ring_row: usize) -> usize {
        let start = ring_row * self.cols;
        let extent = self.row_metadata[ring_row].meaningful_extent.min(self.cols);
        self.cells[start..start + extent]
            .iter()
            .filter(|cell| !cell.wide_continuation)
            .count()
    }

    pub(crate) fn live_row_logical_atom_count(&self, display_row: usize) -> usize {
        self.logical_atom_count(self.display_to_ring(display_row))
    }

    pub(crate) fn recompute_row_extent(&mut self, display_row: usize) {
        let ring_row = self.display_to_ring(display_row);
        self.recompute_ring_row_extent(ring_row);
    }

    /// Metadata for a displayed row, including the current scrollback offset.
    pub(crate) fn row_metadata(&self, display_row: usize) -> RowMetadata {
        debug_assert!(display_row < self.visible_rows);
        let top = (self.visible_start + self.total_rows - self.view_offset) % self.total_rows;
        self.row_metadata[(top + display_row) % self.total_rows]
    }

    /// Metadata for the live writable row, independent of the displayed scrollback offset.
    pub(crate) fn live_row_metadata(&self, display_row: usize) -> RowMetadata {
        debug_assert!(display_row < self.visible_rows);
        self.row_metadata[self.display_to_ring(display_row)]
    }

    pub(crate) fn cell_state(&self, display_row: usize, col: usize) -> CellState {
        debug_assert!(display_row < self.visible_rows);
        debug_assert!(col < self.cols);
        let top = (self.visible_start + self.total_rows - self.view_offset) % self.total_rows;
        let ring_row = (top + display_row) % self.total_rows;
        self.cell_state[ring_row * self.cols + col]
    }

    #[cfg(test)]
    pub(crate) fn cell_is_meaningful(&self, display_row: usize, col: usize) -> bool {
        self.cell_state(display_row, col).is_meaningful()
    }

    /// Restores identity and offsets for a contiguous live soft-wrapped chain.
    /// The replaced row remains a hard boundary while continuations are rebased
    /// onto its new identity.
    pub(crate) fn repair_following_soft_chain(&mut self, display_row: usize) {
        let mut next_row = display_row + 1;
        if next_row >= self.visible_rows {
            return;
        }

        let mut source = self.live_row_metadata(display_row);
        while next_row < self.visible_rows && self.live_row_metadata(next_row).soft_wrapped {
            let ring_row = self.display_to_ring(next_row);
            let logical_start = source
                .logical_start
                .checked_add(source.meaningful_extent)
                .expect("logical line cell offset overflow");
            let logical_atom_start = source
                .logical_atom_start
                .checked_add(self.logical_atom_count(self.display_to_ring(next_row - 1)))
                .expect("logical line atom offset overflow");
            self.touch_ring_row(ring_row);
            let metadata = &mut self.row_metadata[ring_row];
            metadata.logical_line_id = source.logical_line_id;
            metadata.logical_start = logical_start;
            metadata.logical_atom_start = logical_atom_start;
            metadata.soft_wrapped = true;
            metadata.head_truncated = false;
            source = *metadata;
            next_row += 1;
        }
    }

    /// Gives an actually-entered row an independent hard-line identity.
    pub(crate) fn begin_hard_line(&mut self, display_row: usize) {
        let ring_row = self.display_to_ring(display_row);
        let extent = self.row_metadata[ring_row].meaningful_extent;
        let mut metadata = self.fresh_metadata();
        metadata.meaningful_extent = extent;
        self.row_metadata[ring_row] = metadata;
        self.touch_ring_row(ring_row);
        self.repair_following_soft_chain(display_row);
    }

    /// Severs a continuation whose predecessor did not move with it.
    /// Existing hard-line identities remain stable for content anchors.
    pub(crate) fn sever_soft_wrap(&mut self, display_row: usize) {
        if self.live_row_metadata(display_row).soft_wrapped {
            self.begin_hard_line(display_row);
        }
    }

    /// Severs the unchanged row after a region whose last row was replaced.
    pub(crate) fn sever_soft_wrap_after(&mut self, display_row: usize) {
        if let Some(next_row) = display_row
            .checked_add(1)
            .filter(|row| *row < self.visible_rows)
        {
            self.sever_soft_wrap(next_row);
        }
    }

    /// Binds an actually-entered row to the source logical line after autowrap.
    pub(crate) fn continue_logical_line(
        &mut self,
        display_row: usize,
        source: RowMetadata,
        source_atom_count: usize,
    ) {
        let ring_row = self.display_to_ring(display_row);
        self.touch_ring_row(ring_row);
        self.row_metadata[ring_row] = RowMetadata {
            logical_line_id: source.logical_line_id,
            logical_start: source
                .logical_start
                .checked_add(source.meaningful_extent)
                .expect("logical line offset overflow"),
            logical_atom_start: source
                .logical_atom_start
                .checked_add(source_atom_count)
                .expect("logical line atom offset overflow"),
            meaningful_extent: self.row_metadata[ring_row].meaningful_extent,
            soft_wrapped: true,
            head_truncated: false,
        };
        self.repair_following_soft_chain(display_row);
    }

    pub(crate) fn write_cell(
        &mut self,
        display_row: usize,
        col: usize,
        cell: Cell,
        state: CellState,
    ) {
        debug_assert!(display_row < self.visible_rows);
        debug_assert!(col < self.cols);
        let ring_row = self.display_to_ring(display_row);
        let index = ring_row * self.cols + col;
        self.cells[index] = cell;
        self.cell_state[index] = state;
        self.recompute_ring_row_extent(ring_row);
        self.mark_range_dirty(display_row, col, col + 1);
    }

    pub(crate) fn write_meaningful_cell(&mut self, display_row: usize, col: usize, cell: Cell) {
        let state = CellState::explicit(&cell);
        self.write_cell(display_row, col, cell, state);
    }

    pub(crate) fn erase_cell(&mut self, display_row: usize, col: usize, cell: Cell) {
        let state = CellState::erase(&cell);
        self.write_cell(display_row, col, cell, state);
    }

    pub(crate) fn copy_cell(
        &mut self,
        src_row: usize,
        src_col: usize,
        dst_row: usize,
        dst_col: usize,
    ) {
        let src_ring_row = self.display_to_ring(src_row);
        let src_index = src_ring_row * self.cols + src_col;
        let cell = self.cells[src_index].clone();
        let state = self.cell_state[src_index];
        self.write_cell(dst_row, dst_col, cell, state);
    }

    pub(crate) fn mutate_cell_semantics(
        &mut self,
        display_row: usize,
        col: usize,
        mutate: impl FnOnce(&mut Cell),
    ) {
        let ring_row = self.display_to_ring(display_row);
        let index = ring_row * self.cols + col;
        mutate(&mut self.cells[index]);
        self.cell_state[index] = self.cell_state[index].with_recomputed_style(&self.cells[index]);
        self.recompute_ring_row_extent(ring_row);
        self.mark_range_dirty(display_row, col, col + 1);
    }

    /// Fills a contiguous cell range with erase-state content and provenance.
    pub(crate) fn fill_linear_range_with(&mut self, start: usize, end: usize, cell: Cell) {
        self.cell_state[start..end].fill(CellState::erase(&cell));
        self.cells[start..end].fill(cell);
        if start < end {
            for ring_row in (start / self.cols)..=((end - 1) / self.cols) {
                self.recompute_ring_row_extent(ring_row);
            }
        }
    }

    /// Copies cells and their retained-content state within the ring buffer.
    pub(crate) fn copy_linear_range(&mut self, src_start: usize, src_end: usize, dst: usize) {
        let source = self.cells[src_start..src_end].to_vec();
        self.cells[dst..dst + source.len()].clone_from_slice(&source);
        self.cell_state.copy_within(src_start..src_end, dst);
        let len = src_end - src_start;
        if len > 0 {
            for ring_row in (dst / self.cols)..=((dst + len - 1) / self.cols) {
                self.recompute_ring_row_extent(ring_row);
            }
        }
    }

    /// Moves complete rows atomically, including cells, provenance, and metadata.
    /// Both source and destination ranges may wrap around the physical ring.
    pub(crate) fn copy_ring_rows(&mut self, src_start: usize, src_end: usize, dst: usize) {
        debug_assert!(src_start < self.total_rows);
        debug_assert!(src_end < self.total_rows);
        debug_assert!(dst < self.total_rows);
        let row_count = if src_start <= src_end {
            src_end - src_start
        } else {
            self.total_rows - src_start + src_end
        };
        let snapshot: Vec<_> = (0..row_count)
            .map(|offset| {
                let ring_row = (src_start + offset) % self.total_rows;
                let start = ring_row * self.cols;
                (
                    self.cells[start..start + self.cols].to_vec(),
                    self.cell_state[start..start + self.cols].to_vec(),
                    self.row_metadata[ring_row],
                )
            })
            .collect();

        for (offset, (cells, states, metadata)) in snapshot.into_iter().enumerate() {
            let ring_row = (dst + offset) % self.total_rows;
            let start = ring_row * self.cols;
            self.cells[start..start + self.cols].clone_from_slice(&cells);
            self.cell_state[start..start + self.cols].copy_from_slice(&states);
            self.row_metadata[ring_row] = metadata;
            self.touch_ring_row(ring_row);
        }
    }

    /// Returns the text content of a display row as a string.
    pub fn row_text(&self, row: usize) -> String {
        assert!(row < self.visible_rows, "terminal row out of bounds");
        let top = (self.visible_start + self.total_rows - self.view_offset) % self.total_rows;
        let ring_row = (top + row) % self.total_rows;
        let start = ring_row * self.cols;
        self.cells[start..start + self.cols]
            .iter()
            .map(|cell| cell.ch)
            .collect()
    }
    /// Maps a display row (0-based visible row) to its ring-buffer index.
    ///
    /// Caller must ensure `view_offset == 0` (live view) when calling this
    /// for *writing* — scrollback rows must not be mutated.
    #[inline]
    pub fn display_to_ring(&self, display_row: usize) -> usize {
        (self.visible_start + display_row) % self.total_rows
    }

    /// Returns a reference to the cell at `(display_row, col)`.
    pub fn cell(&self, display_row: usize, col: usize) -> &Cell {
        debug_assert!(display_row < self.visible_rows);
        debug_assert!(col < self.cols);
        let top = (self.visible_start + self.total_rows - self.view_offset) % self.total_rows;
        let actual_row = (top + display_row) % self.total_rows;
        &self.cells[actual_row * self.cols + col]
    }

    /// Updates bounded input-tail bookkeeping without changing source content,
    /// row versions or damage. Dropped extensions cannot affect content readers.
    pub(crate) fn discard_text_extension(&mut self, row: usize, col: usize, ch: char) {
        let index = self.display_to_ring(row) * self.cols + col;
        self.cells[index].discard_text_extension(ch);
        // Refresh this projected Cell without invalidating semantic content readers.
        let generation = self.history_start + self.scroll_count as u64 + row as u64;
        self.snapshot_cache.get_mut().invalidate_row(generation);
    }

    /// Returns a mutable reference to the cell at `(display_row, col)`.
    ///
    /// Safe to call only when `view_offset == 0` (writing to live view).
    pub fn cell_mut(&mut self, display_row: usize, col: usize) -> &mut Cell {
        debug_assert!(display_row < self.visible_rows);
        debug_assert!(col < self.cols);
        self.mark_range_dirty(display_row, col, col + 1);
        let actual_row = self.display_to_ring(display_row);
        self.touch_ring_row(actual_row);
        &mut self.cells[actual_row * self.cols + col]
    }

    /// Iterates every retained ring-buffer cell, including scrollback.
    pub(crate) fn retained_cells(&self) -> impl Iterator<Item = &Cell> {
        self.cells.iter()
    }

    pub(crate) fn display_row_has_meaningful_content(&self, row: usize) -> bool {
        if row >= self.visible_rows {
            return false;
        }
        let ring_row = self.display_to_ring(row);
        let meta = self.row_metadata[ring_row];
        meta.meaningful_extent > 0 || meta.soft_wrapped
    }

    pub(crate) fn last_meaningful_display_row(&self) -> Option<usize> {
        (0..self.visible_rows)
            .rev()
            .find(|&row| self.display_row_has_meaningful_content(row))
    }

    pub(crate) fn active_retained_row_count(&self, cursor_floor: Option<usize>) -> usize {
        let last_meaningful = self.last_meaningful_display_row();
        let active_visible = match (cursor_floor, last_meaningful) {
            (Some(c), Some(m)) => c.max(m) + 1,
            (Some(c), None) => c + 1,
            (None, Some(m)) => m + 1,
            (None, None) => self.visible_rows,
        };
        self.scroll_count + active_visible.min(self.visible_rows)
    }

    pub(crate) fn retained_rows_bounded(
        &self,
        count: usize,
    ) -> impl ExactSizeIterator<Item = RetainedRow<'_>> {
        let retained_count = count.min(self.scroll_count + self.visible_rows);
        (0..retained_count).map(move |offset| {
            let ring_row = (self.visible_start + self.total_rows - self.scroll_count + offset)
                % self.total_rows;
            let start = ring_row * self.cols;
            RetainedRow {
                generation: self.history_start + offset as u64,
                metadata: self.row_metadata[ring_row],
                cells: &self.cells[start..start + self.cols],
                cell_state: &self.cell_state[start..start + self.cols],
            }
        })
    }

    /// Iterates all retained rows, oldest to newest, without exposing ring coordinates.
    pub(crate) fn retained_rows(&self) -> impl ExactSizeIterator<Item = RetainedRow<'_>> {
        self.retained_rows_bounded(self.scroll_count + self.visible_rows)
    }

    pub(crate) fn next_logical_line_id(&self) -> u64 {
        self.next_logical_line_id
    }

    pub(crate) fn newest_generation_exclusive(&self) -> Option<u64> {
        let retained = self.scroll_count.checked_add(self.visible_rows)?;
        self.history_start
            .checked_add(u64::try_from(retained).ok()?)
    }

    pub(crate) fn prepare_rectangular_resize(
        &self,
        requested_rows: usize,
        requested_cols: usize,
    ) -> Result<Self, crate::primary_reflow::PreparationError> {
        use crate::primary_reflow::{PreparationError, ReflowedRow};

        let rows = requested_rows.max(1);
        let cols = requested_cols.max(2);
        let copy_rows = self.visible_rows.min(rows);
        let copy_cols = self.cols.min(cols);
        let mut prepared_rows = Vec::new();
        prepared_rows
            .try_reserve_exact(rows)
            .map_err(|_| PreparationError::AllocationFailed)?;
        let mut next_logical_line_id = self.next_logical_line_id;

        for source in self.retained_rows().skip(self.scroll_count).take(copy_rows) {
            let mut cells = Vec::new();
            cells
                .try_reserve_exact(cols)
                .map_err(|_| PreparationError::AllocationFailed)?;
            cells.resize(cols, Cell::default());
            cells[..copy_cols].clone_from_slice(&source.cells[..copy_cols]);

            let mut cell_state = Vec::new();
            cell_state
                .try_reserve_exact(cols)
                .map_err(|_| PreparationError::AllocationFailed)?;
            cell_state.resize(cols, CellState::default());
            cell_state[..copy_cols].copy_from_slice(&source.cell_state[..copy_cols]);
            Self::normalize_wide_row(&mut cells, &mut cell_state);

            let mut metadata = source.metadata;
            metadata.meaningful_extent = cell_state
                .iter()
                .rposition(|state| state.is_meaningful())
                .map_or(0, |col| col + 1);
            prepared_rows.push(ReflowedRow {
                cells,
                cell_state,
                metadata,
            });
        }

        while prepared_rows.len() < rows {
            let line_id = LogicalLineId(next_logical_line_id);
            next_logical_line_id = next_logical_line_id
                .checked_add(1)
                .ok_or(PreparationError::ArithmeticOverflow)?;
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
            prepared_rows.push(ReflowedRow {
                cells,
                cell_state,
                metadata: RowMetadata {
                    logical_line_id: line_id,
                    logical_start: 0,
                    logical_atom_start: 0,
                    meaningful_extent: 0,
                    soft_wrapped: false,
                    head_truncated: false,
                },
            });
        }

        let mut previous: Option<(RowMetadata, usize)> = None;
        for row in &mut prepared_rows {
            let atom_count = row.cells[..row.metadata.meaningful_extent]
                .iter()
                .filter(|cell| !cell.wide_continuation)
                .count();
            if row.metadata.soft_wrapped {
                if let Some((prior, prior_atom_count)) = previous
                    && row.metadata.logical_line_id == prior.logical_line_id
                {
                    row.metadata.logical_start = prior
                        .logical_start
                        .checked_add(prior.meaningful_extent)
                        .ok_or(PreparationError::ArithmeticOverflow)?;
                    row.metadata.logical_atom_start = prior
                        .logical_atom_start
                        .checked_add(prior_atom_count)
                        .ok_or(PreparationError::ArithmeticOverflow)?;
                } else {
                    row.metadata.head_truncated = true;
                }
            }
            previous = Some((row.metadata, atom_count));
        }

        let history_start = self
            .newest_generation_exclusive()
            .ok_or(PreparationError::ArithmeticOverflow)?;
        let mut detached = Self::from_reflowed_rows(
            &prepared_rows,
            rows,
            cols,
            HistoryLimits {
                max_rows: 0,
                max_bytes: 0,
            },
            next_logical_line_id,
            history_start,
            0,
        )?;
        detached.mark_all_dirty();
        Ok(detached)
    }

    pub(crate) fn from_reflowed_rows<C: crate::primary_reflow::ReflowCell>(
        retained: &[crate::primary_reflow::ReflowedRow<C>],
        visible_rows: usize,
        cols: usize,
        limits: HistoryLimits,
        mut next_logical_line_id: u64,
        history_start: u64,
        view_offset: usize,
    ) -> Result<Self, crate::primary_reflow::PreparationError> {
        use crate::primary_reflow::PreparationError;
        let max_scrollback = limits.max_rows;

        let total_rows = max_scrollback
            .checked_add(visible_rows)
            .ok_or(PreparationError::ArithmeticOverflow)?;
        if visible_rows == 0
            || cols == 0
            || retained.len() < visible_rows
            || retained.len() > total_rows
        {
            return Err(PreparationError::Invariant(
                "invalid detached ring dimensions",
            ));
        }
        let cell_count = total_rows
            .checked_mul(cols)
            .ok_or(PreparationError::ArithmeticOverflow)?;
        let retained_generation_count =
            u64::try_from(retained.len()).map_err(|_| PreparationError::ArithmeticOverflow)?;
        history_start
            .checked_add(retained_generation_count)
            .ok_or(PreparationError::ArithmeticOverflow)?;
        let mut cells = Vec::new();
        cells
            .try_reserve_exact(cell_count)
            .map_err(|_| PreparationError::AllocationFailed)?;
        cells.resize(cell_count, Cell::default());
        let mut cell_state = Vec::new();
        cell_state
            .try_reserve_exact(cell_count)
            .map_err(|_| PreparationError::AllocationFailed)?;
        cell_state.resize(cell_count, CellState::default());

        let mut row_metadata = Vec::new();
        row_metadata
            .try_reserve_exact(total_rows)
            .map_err(|_| PreparationError::AllocationFailed)?;
        for _ in 0..total_rows {
            let id = LogicalLineId(next_logical_line_id);
            next_logical_line_id = next_logical_line_id
                .checked_add(1)
                .ok_or(PreparationError::ArithmeticOverflow)?;
            row_metadata.push(RowMetadata {
                logical_line_id: id,
                logical_start: 0,
                logical_atom_start: 0,
                meaningful_extent: 0,
                soft_wrapped: false,
                head_truncated: false,
            });
        }

        let scroll_count = retained.len() - visible_rows;
        let visible_start = max_scrollback;
        let first_ring_row = visible_start
            .checked_add(total_rows)
            .and_then(|value| value.checked_sub(scroll_count))
            .ok_or(PreparationError::ArithmeticOverflow)?
            % total_rows;
        for (offset, row) in retained.iter().enumerate() {
            if row.cells.len() != cols || row.cell_state.len() != cols {
                return Err(PreparationError::Invariant("detached row width mismatch"));
            }
            let ring_row = (first_ring_row + offset) % total_rows;
            let start = ring_row * cols;
            for (target, source) in cells[start..start + cols].iter_mut().zip(&row.cells) {
                *target = source.materialize();
            }
            cell_state[start..start + cols].copy_from_slice(&row.cell_state);
            row_metadata[ring_row] = row.metadata;
        }

        let mut row_versions = Vec::new();
        row_versions
            .try_reserve_exact(total_rows)
            .map_err(|_| PreparationError::AllocationFailed)?;
        row_versions.resize(total_rows, 0);
        Ok(Self {
            snapshot_cache: Default::default(),
            cells,
            cell_state,
            row_metadata,
            content_generation: crate::read_model::fresh_identity(),
            content_revision: 0,
            row_versions,
            next_logical_line_id,
            total_rows,
            visible_rows,
            cols,
            visible_start,
            scroll_count,
            view_offset: view_offset.min(scroll_count),
            damage_tracker: DamageTracker::new(visible_rows, cols),
            max_scrollback,
            max_history_bytes: limits.max_bytes,
            history_bytes: retained[..scroll_count]
                .iter()
                .map(|row| Self::row_source_bytes(&row.cells, row.metadata.meaningful_extent))
                .sum(),
            history_start,
        })
    }

    /// Returns an iterator over all visible cells as `(display_row, col, ch)`.
    pub fn cells(&self) -> CellsIter<'_> {
        let top = (self.visible_start + self.total_rows - self.view_offset) % self.total_rows;
        CellsIter {
            cells: &self.cells,
            total_rows: self.total_rows,
            cols: self.cols,
            visible_rows: self.visible_rows,
            top,
            row: 0,
            col: 0,
        }
    }

    /// Returns dirty display-row indices.
    ///
    /// When `view_offset > 0` (scrolled back), every visible row is
    /// considered dirty.
    pub fn dirty_rows(&self) -> Vec<usize> {
        let mut rows: Vec<usize> = self.dirty_ranges().into_iter().map(|r| r.row).collect();
        rows.dedup();
        rows
    }

    pub fn dirty_ranges(&self) -> Vec<DirtyRange> {
        if self.view_offset > 0 {
            (0..self.visible_rows)
                .map(|row| DirtyRange {
                    row,
                    start_col: 0,
                    end_col: self.cols,
                })
                .collect()
        } else {
            self.damage_tracker.dirty_ranges()
        }
    }

    /// Resets all dirty flags to false.
    pub fn clear_dirty(&mut self) {
        self.damage_tracker.clear();
    }

    pub fn mark_row_dirty(&mut self, display_row: usize) {
        self.damage_tracker.mark_row_dirty(display_row);
    }

    pub fn mark_rows_dirty(&mut self, start_row: usize, end_row: usize) {
        self.damage_tracker.mark_rows_dirty(start_row, end_row);
    }

    pub fn mark_range_dirty(&mut self, display_row: usize, start_col: usize, end_col: usize) {
        self.damage_tracker
            .mark_range_dirty(display_row, start_col, end_col);
    }

    pub fn mark_all_dirty(&mut self) {
        self.damage_tracker.mark_all_dirty();
    }

    /// Maps a retained generation to its ring row.
    fn ring_row_at_generation(&self, generation: u64) -> Option<usize> {
        if generation < self.history_start {
            return None;
        }
        let offset = (generation - self.history_start) as usize;
        if offset >= self.scroll_count + self.visible_rows {
            return None;
        }
        Some((self.visible_start + self.total_rows - self.scroll_count + offset) % self.total_rows)
    }

    /// Read a cell by stable generation coordinate.
    /// Returns `None` when the generation has been evicted from the ring buffer.
    pub fn cell_at_generation(&self, generation: u64, col: usize) -> Option<&Cell> {
        debug_assert!(col < self.cols);
        let ring_row = self.ring_row_at_generation(generation)?;
        Some(&self.cells[ring_row * self.cols + col])
    }

    pub fn is_wrapped_at_generation(&self, generation: u64) -> Option<bool> {
        let ring_row = self.ring_row_at_generation(generation)?;
        Some(self.row_metadata[ring_row].soft_wrapped)
    }

    /// Returns whether the given display row is a soft-wrapped continuation
    /// of the logical line from the row above.
    pub fn is_wrapped(&self, display_row: usize) -> bool {
        self.row_metadata(display_row).soft_wrapped
    }

    #[cfg(test)]
    pub(crate) fn set_head_truncated(&mut self, display_row: usize, head_truncated: bool) {
        debug_assert!(display_row < self.visible_rows);
        let ring_row = self.display_to_ring(display_row);
        self.row_metadata[ring_row].head_truncated = head_truncated;
        self.touch_ring_row(ring_row);
    }

    #[cfg(test)]
    pub(crate) fn set_logical_starts(
        &mut self,
        display_row: usize,
        logical_start: usize,
        logical_atom_start: usize,
    ) {
        let ring_row = self.display_to_ring(display_row);
        self.row_metadata[ring_row].logical_start = logical_start;
        self.row_metadata[ring_row].logical_atom_start = logical_atom_start;
        self.touch_ring_row(ring_row);
    }

    #[cfg(test)]
    pub(crate) fn set_history_start_for_test(&mut self, history_start: u64) {
        self.history_start = history_start;
    }

    /// Compatibility projection used by existing callers that only sever wraps.
    #[cfg(test)]
    pub(crate) fn set_wrapped(&mut self, display_row: usize, wrapped: bool) {
        debug_assert!(display_row < self.visible_rows);
        let ring_row = self.display_to_ring(display_row);
        self.touch_ring_row(ring_row);
        self.row_metadata[ring_row].soft_wrapped = wrapped;
        if !wrapped {
            self.row_metadata[ring_row].logical_start = 0;
            self.row_metadata[ring_row].logical_atom_start = 0;
            self.row_metadata[ring_row].head_truncated = false;
        }
    }

    // ── viewport scroll (user scrolling through history) ────────────

    /// Scroll the viewport up by `n` rows (toward older history).
    pub fn scroll_up(&mut self, n: usize) {
        self.view_offset = (self.view_offset + n).min(self.scroll_count);
        self.mark_all_dirty();
    }

    /// Scroll the viewport down by `n` rows (toward live content).
    pub fn scroll_down(&mut self, n: usize) {
        self.view_offset = self.view_offset.saturating_sub(n);
        self.mark_all_dirty();
    }

    /// Snap the viewport back to the live bottom.
    pub fn scroll_to_bottom(&mut self) {
        // A live viewport has not moved: retain any existing local damage.
        if self.view_offset != 0 {
            self.view_offset = 0;
            self.mark_all_dirty();
        }
    }
    pub(crate) fn set_view_offset(&mut self, offset: usize) {
        self.view_offset = offset.min(self.scroll_count);
        self.mark_all_dirty();
    }

    // ── full-screen scroll (O(1) ring-buffer advance) ───────────────

    /// Advance the ring by `n` rows when full-screen scrolling.
    pub fn scroll_up_full_screen(&mut self, n: usize, cell: Cell) {
        tracing::debug!(
            n,
            visible_start = self.visible_start,
            total_rows = self.total_rows,
            scroll_count = self.scroll_count,
            view_offset = self.view_offset,
            "scroll_up_full_screen: advancing ring"
        );

        let n = n.min(self.visible_rows);
        let reviewing = self.view_offset > 0;
        for _ in 0..n {
            self.history_bytes += self.ring_source_bytes(self.visible_start);
            self.scroll_count += 1;
            self.visible_start = (self.visible_start + 1) % self.total_rows;
            self.enforce_history_limits();
            // Blank the newly exposed row after eviction, before its slot is reused.
            self.fill_row_with(self.visible_rows - 1, cell.clone());
        }
        if reviewing {
            self.view_offset = (self.view_offset + n).min(self.scroll_count);
        }
        // TODO: row remapping would reduce this to O(n)
        self.mark_all_dirty();

        tracing::debug!(
            n,
            visible_start = self.visible_start,
            scroll_count = self.scroll_count,
            view_offset = self.view_offset,
            "scroll_up_full_screen: ring advanced"
        );
    }
    // ── resize ──────────────────────────────────────────────────────

    /// Rebuilds the ring buffer for a new viewport size while retaining available scrollback.
    ///
    /// Rows are copied without reflow. Cells, meaning, and row metadata move together.
    pub fn resize(&mut self, rows: usize, cols: usize) {
        let rows = rows.max(1);
        let cols = cols.max(1);
        if self.visible_rows == rows && self.cols == cols {
            return;
        }

        tracing::debug!(
            old_rows = self.visible_rows,
            old_cols = self.cols,
            new_rows = rows,
            new_cols = cols,
            visible_start = self.visible_start,
            total_rows = self.total_rows,
            scroll_count = self.scroll_count,
            "resize: rebuilding ring buffer"
        );

        let old_rows = self.visible_rows;
        let old_cols = self.cols;
        let old_total = self.total_rows;
        let old_visible_start = self.visible_start;
        let old_scroll_count = self.scroll_count;
        let old_history_start = self.history_start;
        let keep_history = old_scroll_count.min(self.max_scrollback);
        let copied_live_rows = old_rows.min(rows);
        let copied_cols = old_cols.min(cols);
        let new_total = self
            .max_scrollback
            .checked_add(rows)
            .expect("terminal row count overflow");
        let new_cell_count = new_total
            .checked_mul(cols)
            .expect("terminal cell count overflow");
        let old_cells = std::mem::take(&mut self.cells);
        let old_cell_state = std::mem::take(&mut self.cell_state);
        let old_metadata = std::mem::take(&mut self.row_metadata);
        let mut new_cells = vec![Cell::default(); new_cell_count];
        let mut new_cell_state = vec![CellState::default(); new_cell_count];
        let mut new_metadata = vec![None; new_total];

        {
            let mut copy_row = |old_ring_row: usize, new_ring_row: usize| {
                let old_start = old_ring_row * old_cols;
                let new_start = new_ring_row * cols;
                new_cells[new_start..new_start + copied_cols]
                    .clone_from_slice(&old_cells[old_start..old_start + copied_cols]);
                new_cell_state[new_start..new_start + copied_cols]
                    .copy_from_slice(&old_cell_state[old_start..old_start + copied_cols]);
                Self::normalize_wide_row(
                    &mut new_cells[new_start..new_start + cols],
                    &mut new_cell_state[new_start..new_start + cols],
                );
                let mut metadata = old_metadata[old_ring_row];
                metadata.meaningful_extent = new_cell_state[new_start..new_start + cols]
                    .iter()
                    .rposition(|state| state.is_meaningful())
                    .map_or(0, |col| col + 1);
                new_metadata[new_ring_row] = Some(metadata);
            };

            // Copy retained history in generation order immediately before the live viewport.
            for history_index in 0..keep_history {
                let old_sequence_index = old_scroll_count - keep_history + history_index;
                let old_ring_row = (old_visible_start + old_total - old_scroll_count
                    + old_sequence_index)
                    % old_total;
                let new_ring_row = (self.max_scrollback - keep_history + history_index) % new_total;
                copy_row(old_ring_row, new_ring_row);
            }

            // Preserve the top-left live rectangle; newly exposed rows remain blank.
            for live_row in 0..copied_live_rows {
                let old_ring_row = (old_visible_start + live_row) % old_total;
                let new_ring_row = (self.max_scrollback + live_row) % new_total;
                copy_row(old_ring_row, new_ring_row);
            }
        }
        for metadata in &mut new_metadata {
            if metadata.is_none() {
                *metadata = Some(self.fresh_metadata());
            }
        }

        self.total_rows = new_total;
        self.cells = new_cells;
        self.cell_state = new_cell_state;
        self.row_metadata = new_metadata
            .into_iter()
            .map(|metadata| metadata.expect("all resized rows initialized"))
            .collect();
        self.visible_rows = rows;
        self.cols = cols;
        self.visible_start = self.max_scrollback;
        self.scroll_count = keep_history;
        self.view_offset = self.view_offset.min(keep_history);
        self.history_start =
            old_history_start.saturating_add((old_scroll_count - keep_history) as u64);
        self.damage_tracker.resize(rows, cols);
        self.content_generation = crate::read_model::fresh_identity();
        self.content_revision = 0;
        self.row_versions = vec![0; new_total];
        self.history_bytes = self
            .retained_rows()
            .take(self.scroll_count)
            .map(|row| Self::row_source_bytes(row.cells, row.metadata.meaningful_extent))
            .sum();
        self.enforce_history_limits();

        tracing::debug!(
            new_visible_start = self.visible_start,
            new_total_rows = self.total_rows,
            scroll_count = self.scroll_count,
            view_offset = self.view_offset,
            history_start = self.history_start,
            "resize: done"
        );
    }

    // ── bulk helpers for Screen's mutation methods ──

    /// Replaces a display row and assigns a fresh independent identity.
    #[inline]
    pub fn fill_row_with(&mut self, display_row: usize, cell: Cell) {
        let ring_row = self.display_to_ring(display_row);

        tracing::debug!(
            display_row,
            ring_row,
            cols = self.cols,
            "fill_row_with: replacing row"
        );

        let start = ring_row * self.cols;
        let state = CellState::fresh_fill(&cell);
        self.cells[start..start + self.cols].fill(cell);
        self.cell_state[start..start + self.cols].fill(state);
        let mut metadata = self.fresh_metadata();
        metadata.meaningful_extent = if state.is_meaningful() { self.cols } else { 0 };
        self.row_metadata[ring_row] = metadata;
        self.touch_ring_row(ring_row);
        self.repair_following_soft_chain(display_row);
    }

    /// Replaces every visible row with fresh independent metadata.
    pub fn fill_all_with(&mut self, cell: Cell) {
        tracing::debug!(
            visible_rows = self.visible_rows,
            "fill_all_with: replacing all visible rows"
        );

        for display_row in 0..self.visible_rows {
            self.fill_row_with(display_row, cell.clone());
        }
    }

    /// RIS replacement of every retained ring slot without rewinding the allocator.
    pub(crate) fn reset_all_retained(&mut self) {
        self.content_generation = crate::read_model::fresh_identity();
        self.content_revision = 0;
        self.row_versions.fill(0);
        self.cells.fill(Cell::default());
        self.cell_state.fill(CellState::default());
        for ring_row in 0..self.total_rows {
            let metadata = self.fresh_metadata();
            self.row_metadata[ring_row] = metadata;
        }
        self.visible_start = self.max_scrollback;
        self.scroll_count = 0;
        self.history_bytes = 0;
        self.view_offset = 0;
        self.history_start = 0;
    }
}

/// Iterator over visible cells in a `NormalBuf`.
///
/// Yields `(display_row, col, ch)` tuples in row-major order.
pub struct CellsIter<'a> {
    cells: &'a [Cell],
    total_rows: usize,
    cols: usize,
    visible_rows: usize,
    top: usize, // ring index of the first visible display row
    row: usize, // current display row (0 .. visible_rows)
    col: usize, // current column (0 .. cols)
}

impl<'a> Iterator for CellsIter<'a> {
    type Item = (usize, usize, char);

    fn next(&mut self) -> Option<Self::Item> {
        while self.row < self.visible_rows {
            if self.col < self.cols {
                let actual_row = (self.top + self.row) % self.total_rows;
                let ch = self.cells[actual_row * self.cols + self.col].ch;
                let item = (self.row, self.col, ch);
                self.col += 1;
                if self.col == self.cols {
                    self.col = 0;
                    self.row += 1;
                }
                return Some(item);
            }
        }
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = (self.visible_rows - self.row) * self.cols - self.col;
        (remaining, Some(remaining))
    }
}

impl<'a> ExactSizeIterator for CellsIter<'a> {}

// Non-shipping capacity/storage probes; no production history settings are exposed.
#[cfg(test)]
#[path = "history_calibration.rs"]
mod calibration;

#[cfg(test)]
mod tests;
