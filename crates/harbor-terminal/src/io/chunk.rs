//! Explicit bounded batch recycling. Payloads own their buffers, not pool handles.
use super::{Arc, Mutex, PTY_QUEUE_CAPACITY};

pub(super) const CHUNK_BYTES: usize = 4096;
const RETAINED_BUFFERS: usize = PTY_QUEUE_CAPACITY + 2;
const RETURN_BATCH: usize = 16;
type Buffer = Box<[u8]>;
type Pool = Arc<Mutex<Vec<Buffer>>>;

pub(super) struct ReaderBuffers {
    shared: Pool,
    local: Vec<Buffer>,
}

pub(super) struct ReaderRecycle {
    shared: Pool,
    pending: Vec<Buffer>,
}

impl ReaderBuffers {
    pub fn pair() -> (Self, ReaderRecycle) {
        let shared = Arc::new(Mutex::new(Vec::with_capacity(RETAINED_BUFFERS)));
        (
            Self {
                shared: shared.clone(),
                local: Vec::with_capacity(RETAINED_BUFFERS),
            },
            ReaderRecycle {
                shared,
                pending: Vec::with_capacity(RETURN_BATCH),
            },
        )
    }

    fn buffer(&mut self) -> Buffer {
        if self.local.is_empty() {
            let mut shared = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            std::mem::swap(&mut self.local, &mut *shared);
        }
        self.local
            .pop()
            .unwrap_or_else(|| vec![0; CHUNK_BYTES].into_boxed_slice())
    }

    // An interrupted, unpublished read stays on the producer without a pool lock.
    pub fn put_unread(&mut self, chunk: ReaderChunk) {
        debug_assert!(self.local.len() < RETAINED_BUFFERS);
        self.local.push(chunk.bytes);
    }

    pub fn take(&mut self) -> ReaderChunk {
        ReaderChunk {
            bytes: self.buffer(),
            valid_len: 0,
        }
    }
}

impl ReaderRecycle {
    pub fn recycle(&mut self, chunk: ReaderChunk) {
        if chunk.bytes.len() == CHUNK_BYTES {
            self.pending.push(chunk.bytes);
            if self.pending.len() == RETURN_BATCH {
                self.flush();
            }
        }
    }

    pub fn flush(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        {
            let mut shared = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            let keep = self.pending.len().min(RETAINED_BUFFERS - shared.len());
            shared.extend(self.pending.drain(..keep));
        }
        self.pending.clear();
    }
}

impl Default for ReaderRecycle {
    fn default() -> Self {
        ReaderBuffers::pair().1
    }
}

pub(super) struct ReaderChunk {
    bytes: Buffer,
    valid_len: usize,
}

impl std::fmt::Debug for ReaderChunk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ReaderChunk").field(&&self[..]).finish()
    }
}

impl ReaderChunk {
    pub fn read_buffer(&mut self) -> &mut [u8] {
        &mut self.bytes
    }
    pub fn finish(&mut self, len: usize) {
        assert!(len <= self.bytes.len());
        self.valid_len = len;
    }
}

impl std::ops::Deref for ReaderChunk {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.bytes[..self.valid_len]
    }
}

#[cfg(test)]
impl From<Vec<u8>> for ReaderChunk {
    fn from(bytes: Vec<u8>) -> Self {
        let valid_len = bytes.len();
        Self {
            bytes: bytes.into_boxed_slice(),
            valid_len,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primary_reflow::allocation_probe::measure;
    use std::collections::VecDeque;

    fn payload(buffers: &mut ReaderBuffers, value: u8, len: usize) -> ReaderChunk {
        let mut chunk = buffers.take();
        chunk.read_buffer()[..len].fill(value);
        chunk.finish(len);
        chunk
    }

    #[test]
    fn owned_payload_is_not_reused_until_explicit_return_and_flush() {
        let (mut buffers, mut recycle) = ReaderBuffers::pair();
        let old = payload(&mut buffers, b'o', 64);
        let old_ptr = old.as_ptr();
        let current = payload(&mut buffers, b'n', 4096);
        let current_ptr = current.as_ptr();
        assert_ne!(old_ptr, current_ptr);
        recycle.recycle(current);
        assert_eq!(recycle.pending.len(), 1);
        assert!(recycle.shared.lock().unwrap().is_empty());
        recycle.flush();
        let short = payload(&mut buffers, b'x', 1);
        assert_eq!(short.as_ptr(), current_ptr);
        assert_eq!(&*short, b"x", "stale bytes beyond valid_len are invisible");
        assert!(old.iter().all(|&b| b == b'o'));
        recycle.recycle(short);
        recycle.recycle(old);
        recycle.flush();
        assert!(recycle.pending.is_empty());
    }

    #[test]
    fn queued_batch_churn_is_warmed_allocation_free_with_variable_lengths() {
        let (mut buffers, mut recycle) = ReaderBuffers::pair();
        let mut queued: VecDeque<_> = (0..PTY_QUEUE_CAPACITY)
            .map(|n| {
                payload(
                    &mut buffers,
                    n as u8,
                    if n.is_multiple_of(2) { 64 } else { 4096 },
                )
            })
            .collect();
        let mut step = |n: usize| {
            let next = n + PTY_QUEUE_CAPACITY;
            let chunk = payload(
                &mut buffers,
                (next % 251) as u8,
                if next.is_multiple_of(2) { 64 } else { 4096 },
            );
            let old = queued.pop_front().unwrap();
            assert_eq!(old.len(), if n.is_multiple_of(2) { 64 } else { 4096 });
            assert!(old.iter().all(|&b| b == (n % 251) as u8));
            queued.push_back(chunk);
            recycle.recycle(old);
        };
        // A batch may retain up to 15 returned buffers before publication.
        for n in 0..64 {
            step(n);
        }
        let (_, stats) = measure(|| {
            for n in 64..1088 {
                step(n);
            }
        });
        assert_eq!(stats.calls, 0);
        for chunk in queued {
            recycle.recycle(chunk);
        }
        recycle.flush();
        assert!(recycle.pending.is_empty());
        assert!(recycle.shared.lock().unwrap().len() <= RETAINED_BUFFERS);
        assert!(buffers.local.len() <= RETAINED_BUFFERS);
    }

    #[test]
    fn retained_storage_stays_bounded_and_oversized_test_chunks_are_not_cached() {
        let (mut buffers, mut recycle) = ReaderBuffers::pair();
        let pressure: Vec<_> = (0..RETAINED_BUFFERS * 4)
            .map(|n| payload(&mut buffers, n as u8, CHUNK_BYTES))
            .collect();
        for chunk in pressure {
            recycle.recycle(chunk);
        }
        recycle.flush();
        assert_eq!(recycle.shared.lock().unwrap().len(), RETAINED_BUFFERS);
        recycle.recycle(vec![b'x'; CHUNK_BYTES * 2].into());
        recycle.flush();
        assert_eq!(recycle.shared.lock().unwrap().len(), RETAINED_BUFFERS);
        assert!(recycle.pending.capacity() <= RETURN_BATCH);
        assert!(buffers.local.capacity() <= RETAINED_BUFFERS);
        assert_eq!(
            std::mem::size_of::<ReaderChunk>(),
            std::mem::size_of::<Vec<u8>>()
        );
    }

    #[test]
    fn unpublished_read_buffer_is_reused_locally_without_allocations() {
        let (mut buffers, _recycle) = ReaderBuffers::pair();
        let mut chunk = buffers.take();
        let pointer = chunk.as_ptr();
        let (_, stats) = measure(|| {
            for _ in 0..1024 {
                buffers.put_unread(chunk);
                chunk = buffers.take();
                assert_eq!(chunk.as_ptr(), pointer);
                assert!(chunk.is_empty());
            }
        });
        assert_eq!(stats.calls, 0);
    }
}
