//! VT edit engine: pen state, cell operations, and character writing.
//!
//! Thin re-export module — the implementation has been split into three
//! focused sub-modules:
//!
//! - [`pen_state`]   — `Pen`, `PenState`, `TabStops`, `CharacterSets`, `map_dec_graphics`
//! - [`cell_ops`]    — `Rect`, `CellOps` (erase, insert, delete, scroll, DEC rects)
//! - [`cell_writer`] — `CellWriter` (write_char and decomposed helpers)

mod cell_ops;
mod cell_writer;
mod pen_state;

pub(crate) use cell_ops::*;
pub(crate) use cell_writer::*;
pub(crate) use pen_state::*;
