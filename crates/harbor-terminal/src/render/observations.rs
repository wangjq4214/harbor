//! Test-only, thread-local observations of the most recent preparation.
//! No observation state is stored in production renderer types. Read a snapshot
//! before preparing another renderer on the same thread.
use std::cell::RefCell;

#[derive(Clone, Default, Debug)]
pub(super) struct SequenceWork {
    pub classified_cells: usize,
    pub built_rows: Vec<usize>,
    // Row, complete-tile bytes, missing-cue bytes. Clearing draws writes no bytes.
    pub writes: Vec<(usize, usize, usize)>,
    pub texture_bytes: usize,
}

thread_local! {
    static SEQUENCE: RefCell<SequenceWork> = RefCell::default();
    static TEXT_UPLOADS: RefCell<Vec<(usize, usize)>> = const { RefCell::new(Vec::new()) };
}

pub(super) fn reset_sequence() {
    SEQUENCE.with_borrow_mut(|work| *work = SequenceWork::default());
}

pub(super) fn record_sequence(record: impl FnOnce(&mut SequenceWork)) {
    SEQUENCE.with_borrow_mut(record);
}

pub(super) fn sequence_work() -> SequenceWork {
    SEQUENCE.with_borrow(Clone::clone)
}

pub(super) fn reset_text() {
    TEXT_UPLOADS.with_borrow_mut(Vec::clear);
}

pub(super) fn record_text_upload(offset: usize, bytes: usize) {
    TEXT_UPLOADS.with_borrow_mut(|uploads| uploads.push((offset, bytes)));
}

pub(super) fn text_uploads() -> Vec<(usize, usize)> {
    TEXT_UPLOADS.with_borrow(Clone::clone)
}
