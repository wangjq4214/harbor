//! Bounded, owned retained-content reads, independent of renderer acknowledgement.
//!
//! A call observes one immutable engine borrow. Pages outlive that borrow (and the
//! engine), but ownership is not validity. Strict continuation requires the exact
//! content version. `rebase_content_read` explicitly starts a new consistency point
//! at an unchanged checkpoint; it does NOT certify or concatenate previous pages.
//! Validate/refresh range proofs before publishing or activating them. Untouched
//! rows survive unrelated output, allowing bounded forward progress without a
//! full-history restart. Changed rows are conservatively invalidated, even if the
//! same text is subsequently restored. Resize/reflow/reset replace the generation;
//! no remapping or restoration of discarded tails is promised.
//!
//! Budgets limit rows visited, complete text units copied and UTF-8 payload bytes
//! (source text plus owned OSC 8 URI/id, counted for each returned unit). Fixed-size
//! records are separately bounded by row/unit counts; allocator overhead is not
//! included in payload bytes. Limits are hard maxima, not silent clamps. A unit
//! too large for an empty page returns `UnitTooLarge`, never a split unit or an
//! endlessly empty continuation. No preceding rows/history are decoded or cloned.
//! At most one additional row's metadata is inspected for boundary evidence.
//!
//! Viewport, cursor, palette, drawing, failed/skipped drawing and acknowledgement
//! are NOT content revisions. Semantic cell/provenance/line edits advance the
//! buffer revision; per-row stamps support bounded validation after other edits.
//! Coordinates are the existing logical line/atom/affinity scheme, not byte,
//! scalar, cell or ring offsets. Only retained content is represented, including
//! meaningful interior/style blanks, excluding capacity and generated wide-edge
//! padding. Ordinary-tail and ConPTY styled-overflow resize losses stay lost.
//! Host binding/closed-session policy and query identity are consumer concerns;
//! `SessionId` identifies an engine lifetime, never its current tab position.
//!
//! ```
//! use harbor_terminal::{Terminal, read_model::{ReadBudget, ReadScope}};
//! let mut terminal = Terminal::new_headless(3, 8);
//! terminal.put_str("hello");
//! let page = terminal.read_content(
//!     ReadScope::Visible { start_row: 0, row_count: 1 }, ReadBudget::default()).unwrap();
//! let range = page.fragments[0].range(0..5).unwrap();
//! assert!(terminal.validate_content_range(&range));
//! terminal.put_str("\rX");
//! assert!(!terminal.validate_content_range(&range));
//! ```

use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};

pub use crate::content_anchor::Affinity;
pub use crate::logical_content::LogicalAtomOffset;
pub use crate::normal_buf::LogicalLineId;
use crate::{CellAttrs, Color, Terminal};

pub const MAX_READ_ROWS: usize = 128;
pub const MAX_READ_UNITS: usize = 4096;
pub const MAX_READ_PAYLOAD_BYTES: usize = 256 * 1024;

pub(crate) fn fresh_identity() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .expect("terminal read identity exhausted")
}

/// Process-local engine/session identity. A replacement engine always differs.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct SessionId(pub(crate) u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BufferKind {
    Primary,
    Alternate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContentVersion {
    pub session: SessionId,
    pub buffer: BufferKind,
    /// Changes on reset, replacement and committed resize/reflow.
    pub generation: u64,
    /// Semantic mutation sequence, independent of renderer projection epochs.
    pub revision: u64,
}

impl ContentVersion {
    fn same_generation(self, other: Self) -> bool {
        self.session == other.session
            && self.buffer == other.buffer
            && self.generation == other.generation
    }
}

/// Shared logical insertion boundary, also suitable for future shell positions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LogicalPosition {
    pub line: LogicalLineId,
    pub offset: LogicalAtomOffset,
    pub affinity: Affinity,
}

/// Scope is captured on the first call; a continuation never extends its end.
/// Row indices are request-local selectors, NOT durable content identities.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadScope {
    /// Currently displayed rows, including a scrolled-back viewport.
    Visible { start_row: usize, row_count: usize },
    /// Retained rows, oldest first (history plus live rows).
    Retained { start_row: usize, row_count: usize },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadBudget {
    pub rows: usize,
    pub units: usize,
    pub payload_bytes: usize,
}

impl Default for ReadBudget {
    fn default() -> Self {
        Self {
            rows: 32,
            units: 1024,
            payload_bytes: 64 * 1024,
        }
    }
}

impl ReadBudget {
    fn validate(self) -> Result<(), ReadError> {
        if self.rows == 0
            || self.rows > MAX_READ_ROWS
            || self.units == 0
            || self.units > MAX_READ_UNITS
            || self.payload_bytes == 0
            || self.payload_bytes > MAX_READ_PAYLOAD_BYTES
        {
            return Err(ReadError::InvalidBudget);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadError {
    InvalidBudget,
    InvalidScope,
    StaleContinuation,
    /// Minimum payload budget required for the next complete unit and its link.
    UnitTooLarge {
        required_payload_bytes: usize,
    },
    /// Malformed retained wide-cell or logical-wrap invariants; no partial success.
    InvalidContent,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnedHyperlink {
    pub uri: String,
    pub id: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextStyle {
    pub foreground: Color,
    pub background: Color,
    pub underline_color: Color,
    pub attrs: CellAttrs,
    pub protected: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadUnit {
    /// Raw complete shared text unit; never contains a synthetic dotted circle.
    pub text: String,
    pub cell_width: u8,
    pub meaningful_blank: bool,
    pub style: TextStyle,
    /// Owned, control-free, registry-bounded URI/id, safe after cleanup/expiry.
    pub hyperlink: Option<OwnedHyperlink>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FragmentStart {
    LineStart,
    /// `true` means this retained row is the truncated head. `false` does not
    /// assert that earlier portions of the logical line are still retained.
    Continuation {
        prefix_evicted: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FragmentEnd {
    /// Page ends inside a physical row; not a newline.
    Continued,
    /// Next retained physical row continues this logical line; not a newline.
    SoftWrap,
    /// A real boundary before the next retained logical line (even outside scope).
    HardBreak,
    /// No following retained row. Does not assert a trailing source newline.
    RetainedEnd,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RowProof {
    generation: u64,
    revision: u64,
}

/// One bounded physical-row fragment with logical boundaries. Empty hard lines
/// are explicit fragments, not synthesized text units or page-edge newlines.
#[derive(Clone, Debug)]
pub struct ReadFragment {
    version: ContentVersion,
    line: LogicalLineId,
    atom_start: usize,
    start: FragmentStart,
    end: FragmentEnd,
    units: Vec<ReadUnit>,
    proof: RowProof,
}

impl ReadFragment {
    pub fn line(&self) -> LogicalLineId {
        self.line
    }
    pub fn atom_start(&self) -> LogicalAtomOffset {
        LogicalAtomOffset(self.atom_start)
    }
    pub fn start(&self) -> FragmentStart {
        self.start
    }
    pub fn end(&self) -> FragmentEnd {
        self.end
    }
    pub fn units(&self) -> &[ReadUnit] {
        &self.units
    }

    /// Creates a proven half-open logical range over complete units. A zero-width
    /// range is allowed (including an explicit blank line) for navigation metadata.
    pub fn range(&self, units: Range<usize>) -> Option<ContentRange> {
        if units.start > units.end || units.end > self.units.len() {
            return None;
        }
        Some(ContentRange {
            version: self.version,
            start: LogicalPosition {
                line: self.line,
                offset: LogicalAtomOffset(self.atom_start + units.start),
                affinity: Affinity::Before,
            },
            end: LogicalPosition {
                line: self.line,
                offset: LogicalAtomOffset(self.atom_start + units.end),
                affinity: Affinity::After,
            },
            rows: vec![self.proof],
        })
    }
}

/// Renderer-neutral decoration/navigation range. Private evidence cannot be
/// replaced with a merely resolvable anchor. Validate against the active engine.
#[derive(Clone, Debug)]
pub struct ContentRange {
    version: ContentVersion,
    start: LogicalPosition,
    end: LogicalPosition,
    rows: Vec<RowProof>,
}

impl ContentRange {
    pub fn version(&self) -> ContentVersion {
        self.version
    }
    pub fn start(&self) -> LogicalPosition {
        self.start
    }
    pub fn end(&self) -> LogicalPosition {
        self.end
    }

    /// Joins adjacent same-line ranges at one consistency point, including soft
    /// wraps/pages. At most MAX_READ_ROWS row proofs may be retained per range.
    /// After rebasing reads, explicitly refresh both ranges before joining.
    pub fn join(&self, next: &Self) -> Option<Self> {
        if self.version != next.version
            || self.end.line != next.start.line
            || self.end.offset != next.start.offset
        {
            return None;
        }
        let mut rows = self.rows.clone();
        for proof in &next.rows {
            if rows.last() != Some(proof) {
                rows.push(*proof);
            }
        }
        if rows.len() > MAX_READ_ROWS {
            return None;
        }
        Some(Self {
            version: self.version,
            start: self.start,
            end: next.end,
            rows,
        })
    }
}

/// Neutral data only. Reading/validating never opens a URL/file or moves a cursor.
/// Hosts must authorize payloads and validate the range again at activation.
#[derive(Clone, Debug)]
pub struct ActionTarget {
    pub range: ContentRange,
    pub action: TargetAction,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TargetAction {
    Navigate,
    Uri(String),
    File {
        path: String,
        line: Option<u32>,
        column: Option<u32>,
    },
}

/// Opaque, fixed-size forward checkpoint and fixed scope end, not ring identity.
/// Cloning it creates an independent reader; no engine subscription is registered.
#[derive(Clone, Debug)]
pub struct ReadContinuation {
    version: ContentVersion,
    scope: ReadScope,
    selected_rows: usize,
    scope_clipped: bool,
    next_generation: u64,
    end_generation: u64,
    col: usize,
    atom_offset: usize,
    checkpoint: RowProof,
}

#[derive(Clone, Debug)]
pub struct ReadPage {
    pub version: ContentVersion,
    pub scope: ReadScope,
    /// Total rows selected by this fixed request, not just this page.
    pub selected_rows: usize,
    /// Requested scope extended past currently available rows.
    pub scope_clipped: bool,
    /// Actual rows visited, units and payload copied by this call.
    pub rows_visited: usize,
    pub copied_units: usize,
    pub copied_payload_bytes: usize,
    pub fragments: Vec<ReadFragment>,
    /// None means the selected scope is complete, not necessarily all history.
    pub continuation: Option<ReadContinuation>,
}

impl Terminal {
    pub fn content_version(&self) -> ContentVersion {
        let normal = self.screen.read_buffer();
        ContentVersion {
            session: self.read_session,
            buffer: if self.screen.is_alt() {
                BufferKind::Alternate
            } else {
                BufferKind::Primary
            },
            generation: normal.content_generation(),
            revision: normal.content_revision(),
        }
    }

    /// Reads an explicitly selected subset without mutable Screen or full-history
    /// projection. Overshooting the available scope clips and reports it; a start
    /// beyond available rows is an error. Zero selected rows return a complete page.
    pub fn read_content(
        &self,
        scope: ReadScope,
        budget: ReadBudget,
    ) -> Result<ReadPage, ReadError> {
        budget.validate()?;
        let normal = self.screen.read_buffer();
        let (base, available, start_row, row_count) = match scope {
            ReadScope::Visible {
                start_row,
                row_count,
            } => (
                normal.history_start() + (normal.scroll_count() - normal.view_offset()) as u64,
                normal.rows(),
                start_row,
                row_count,
            ),
            ReadScope::Retained {
                start_row,
                row_count,
            } => (
                normal.history_start(),
                normal.scroll_count() + normal.rows(),
                start_row,
                row_count,
            ),
        };
        if start_row > available {
            return Err(ReadError::InvalidScope);
        }
        let selected_rows = row_count.min(available - start_row);
        let next_generation = base + start_row as u64;
        let checkpoint = RowProof {
            generation: next_generation,
            revision: normal.row_version(next_generation).unwrap_or(0),
        };
        self.read_content_at(
            ReadContinuation {
                version: self.content_version(),
                scope,
                selected_rows,
                scope_clipped: selected_rows != row_count,
                next_generation,
                end_generation: next_generation + selected_rows as u64,
                col: 0,
                atom_offset: 0,
                checkpoint,
            },
            budget,
        )
    }

    /// Strict: any intervening semantic revision requires an explicit rebase.
    pub fn continue_content_read(
        &self,
        continuation: &ReadContinuation,
        budget: ReadBudget,
    ) -> Result<ReadPage, ReadError> {
        budget.validate()?;
        if continuation.version != self.content_version() {
            return Err(ReadError::StaleContinuation);
        }
        self.read_content_at(continuation.clone(), budget)
    }

    /// Explicit new consistency point for forward progress during unrelated output.
    /// Requires the page-edge checkpoint row to be unchanged and retained. Earlier
    /// page/range validity is NOT implied. The captured end is never extended.
    pub fn rebase_content_read(
        &self,
        continuation: &ReadContinuation,
    ) -> Result<ReadContinuation, ReadError> {
        let version = self.content_version();
        if !version.same_generation(continuation.version)
            || !self.valid_read_row(continuation.checkpoint)
            || self
                .screen
                .read_buffer()
                .retained_row(continuation.next_generation)
                .is_none()
        {
            return Err(ReadError::StaleContinuation);
        }
        let mut rebased = continuation.clone();
        rebased.version = version;
        Ok(rebased)
    }

    /// Bounded by MAX_READ_ROWS O(1) ring lookups; no text/history decoding.
    /// Inactive/replaced buffers fail. Untouched rows can survive other output;
    /// any mutation to a referenced row conservatively fails, even outside range.
    pub fn validate_content_range(&self, range: &ContentRange) -> bool {
        self.content_version().same_generation(range.version)
            && range.rows.iter().all(|proof| self.valid_read_row(*proof))
    }

    /// Requalifies surviving range evidence at the current consistency point.
    pub fn refresh_content_range(&self, range: &ContentRange) -> Option<ContentRange> {
        if !self.validate_content_range(range) {
            return None;
        }
        let mut range = range.clone();
        range.version = self.content_version();
        Some(range)
    }

    pub fn validate_action_target(&self, target: &ActionTarget) -> bool {
        self.validate_content_range(&target.range)
    }

    fn valid_read_row(&self, proof: RowProof) -> bool {
        self.screen.read_buffer().row_version(proof.generation) == Some(proof.revision)
    }

    fn read_content_at(
        &self,
        mut cursor: ReadContinuation,
        budget: ReadBudget,
    ) -> Result<ReadPage, ReadError> {
        let normal = self.screen.read_buffer();
        let mut page = ReadPage {
            version: cursor.version,
            scope: cursor.scope,
            selected_rows: cursor.selected_rows,
            scope_clipped: cursor.scope_clipped,
            rows_visited: 0,
            copied_units: 0,
            copied_payload_bytes: 0,
            fragments: Vec::new(),
            continuation: None,
        };
        while cursor.next_generation < cursor.end_generation && page.rows_visited < budget.rows {
            let row = normal
                .retained_row(cursor.next_generation)
                .ok_or(ReadError::StaleContinuation)?;
            page.rows_visited += 1;
            let extent = row.metadata.meaningful_extent.min(row.cells.len());
            if cursor.col == 0 {
                cursor.atom_offset = row.metadata.logical_atom_start;
            }
            let proof = RowProof {
                generation: row.generation,
                revision: normal
                    .row_version(row.generation)
                    .ok_or(ReadError::InvalidContent)?,
            };
            let mut fragment = ReadFragment {
                version: page.version,
                line: row.metadata.logical_line_id,
                atom_start: cursor.atom_offset,
                start: if cursor.col == 0 && !row.metadata.soft_wrapped {
                    FragmentStart::LineStart
                } else {
                    FragmentStart::Continuation {
                        prefix_evicted: row.metadata.head_truncated,
                    }
                },
                end: FragmentEnd::Continued,
                units: Vec::new(),
                proof,
            };
            while cursor.col < extent {
                let width = crate::logical_content::row_glyph_width(row, cursor.col)
                    .map_err(|_| ReadError::InvalidContent)?;
                let cell = &row.cells[cursor.col];
                let link = cell.hyperlink.and_then(|id| self.screen.read_hyperlink(id));
                let bytes = cell.ch.len_utf8()
                    + cell.suffix.len()
                    + link.map_or(0, |(uri, id)| uri.len() + id.map_or(0, str::len));
                if page.copied_units == budget.units
                    || bytes > budget.payload_bytes - page.copied_payload_bytes
                {
                    if page.copied_units == 0 && bytes > budget.payload_bytes {
                        return Err(ReadError::UnitTooLarge {
                            required_payload_bytes: bytes,
                        });
                    }
                    // Do not emit an empty continuation fragment as a fake blank line.
                    if !fragment.units.is_empty() {
                        page.fragments.push(fragment);
                    }
                    cursor.checkpoint = proof;
                    page.continuation = Some(cursor);
                    return Ok(page);
                }
                fragment.units.push(ReadUnit {
                    text: cell.raw_text(),
                    cell_width: width,
                    meaningful_blank: cell.ch == ' '
                        && cell.suffix.is_empty()
                        && row.cell_state[cursor.col].is_meaningful(),
                    style: TextStyle {
                        foreground: cell.fg,
                        background: cell.bg,
                        underline_color: cell.underline_color,
                        attrs: cell.attrs,
                        protected: cell.protected,
                    },
                    hyperlink: link.map(|(uri, id)| OwnedHyperlink {
                        uri: uri.to_owned(),
                        id: id.map(str::to_owned),
                    }),
                });
                page.copied_units += 1;
                page.copied_payload_bytes += bytes;
                cursor.col += usize::from(width);
                cursor.atom_offset += 1;
            }
            let next = normal.retained_row(row.generation + 1);
            fragment.end = match next {
                Some(next) if next.metadata.soft_wrapped => {
                    if next.metadata.logical_line_id != row.metadata.logical_line_id
                        || next.metadata.logical_atom_start != cursor.atom_offset
                        || next.metadata.logical_start
                            != row.metadata.logical_start + row.metadata.meaningful_extent
                    {
                        return Err(ReadError::InvalidContent);
                    }
                    FragmentEnd::SoftWrap
                }
                Some(_) => FragmentEnd::HardBreak,
                None => FragmentEnd::RetainedEnd,
            };
            page.fragments.push(fragment);
            cursor.checkpoint = proof;
            cursor.next_generation += 1;
            cursor.col = 0;
            cursor.atom_offset = 0;
            if page.copied_units == budget.units {
                break;
            }
        }
        if cursor.next_generation < cursor.end_generation {
            page.continuation = Some(cursor);
        }
        Ok(page)
    }
}

#[cfg(test)]
mod tests;
