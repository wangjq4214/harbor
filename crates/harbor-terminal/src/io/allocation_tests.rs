use super::*;
use crate::primary_reflow::allocation_probe::{Stats, measure};
use std::cell::{Cell, RefCell};

struct Chunks {
    remaining: usize,
    size: usize,
}
impl Read for Chunks {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        if self.remaining == 0 {
            return Ok(0);
        }
        self.remaining -= 1;
        bytes[..self.size].fill((self.remaining % 251) as u8);
        Ok(self.size)
    }
}

fn run(chunks: usize, size: usize) -> Stats {
    let (tx, rx) = mpsc::sync_channel(PTY_QUEUE_CAPACITY);
    let (_commands, command_rx) = mpsc::channel();
    let pending = Arc::new(AtomicBool::new(false));
    let consumed = Cell::new(0);
    let (_, stats) = measure(|| {
        // Include endpoint/cache construction: do not hide moved allocations.
        let (buffers, recycle) = ReaderBuffers::pair();
        let recycle = RefCell::new(recycle);
        pump_reader(
            Chunks {
                remaining: chunks,
                size,
            },
            tx,
            command_rx,
            pending.clone(),
            || {
                let mut recycle = recycle.borrow_mut();
                while let Ok(event) = rx.try_recv() {
                    let ReaderEvent::Bytes(bytes) = event else {
                        panic!("unexpected ACK")
                    };
                    assert_eq!(bytes.len(), size);
                    assert!(
                        bytes
                            .iter()
                            .all(|b| *b == ((chunks - consumed.get() - 1) % 251) as u8)
                    );
                    consumed.set(consumed.get() + 1);
                    recycle.recycle(bytes);
                }
                recycle.flush();
                pending.store(false, Ordering::Release);
                true
            },
            buffers,
        );
    });
    assert_eq!(consumed.get(), chunks);
    stats
}

#[test]
fn hot_path_reader_allocation_measurements() {
    for size in [64, 4096] {
        let warm = run(1, size);
        let total = run(1025, size);
        eprintln!(
            "HOT reader chunk={size} warm={warm:?} total1025={total:?} incremental1024 calls={} bytes={}",
            total.calls - warm.calls,
            total.allocated_bytes - warm.allocated_bytes
        );
        assert_eq!(total.calls - warm.calls, 0);
        assert_eq!(total.allocated_bytes - warm.allocated_bytes, 0);
    }
}
