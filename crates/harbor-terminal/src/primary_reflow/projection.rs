//! Preparation-only cell locations. No source payload is owned until materialization.

use crate::screen::Cell;

/// Shared by read-only primary projections and the owned rectangular/reference rows.
pub(crate) trait ReflowCell {
    fn materialize(&self) -> Cell;
    fn source_bytes(&self) -> usize;
}

impl ReflowCell for Cell {
    fn materialize(&self) -> Cell {
        self.clone()
    }

    fn source_bytes(&self) -> usize {
        if self.wide_continuation {
            0
        } else {
            self.ch.len_utf8() + self.suffix.len()
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum ProjectedCell<'a> {
    #[default]
    Padding,
    Source(&'a Cell),
    /// Derived from the lead's style, not a copy of a possibly stale source half.
    Continuation(&'a Cell),
}

impl ReflowCell for ProjectedCell<'_> {
    fn materialize(&self) -> Cell {
        match self {
            Self::Padding => Cell::default(),
            Self::Source(cell) => {
                #[cfg(test)]
                SOURCE_COPIES.with(|copies| copies.set(copies.get() + 1));
                (*cell).clone()
            }
            Self::Continuation(cell) => Cell::wide_continuation_of(cell),
        }
    }

    fn source_bytes(&self) -> usize {
        match self {
            Self::Source(cell) => cell.source_bytes(),
            Self::Padding | Self::Continuation(_) => 0,
        }
    }
}

#[cfg(test)]
std::thread_local! {
    pub(super) static SOURCE_COPIES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
