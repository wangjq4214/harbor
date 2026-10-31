//! Test-binary-only, thread-local accounting. Counts requested heap bytes,
//! not allocator overhead, process working set, or allocations on other threads.
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Stats {
    pub calls: usize,
    pub allocated_bytes: usize,
    pub peak_live_bytes: usize,
    live_bytes: usize,
}

thread_local! {
    static ACTIVE: Cell<Option<Stats>> = const { Cell::new(None) };
}

struct Probe;

#[global_allocator]
static ALLOCATOR: Probe = Probe;

fn record(allocated: usize, freed: usize, allocation: bool) {
    let _ = ACTIVE.try_with(|active| {
        if let Some(mut stats) = active.get() {
            stats.calls = stats.calls.saturating_add(usize::from(allocation));
            stats.allocated_bytes = stats.allocated_bytes.saturating_add(allocated);
            stats.live_bytes = stats
                .live_bytes
                .saturating_sub(freed)
                .saturating_add(allocated);
            stats.peak_live_bytes = stats.peak_live_bytes.max(stats.live_bytes);
            active.set(Some(stats));
        }
    });
}

// SAFETY: Every allocation/deallocation is forwarded unchanged to System.
// Accounting has no allocations, panics or pointer dereferences of its own.
unsafe impl GlobalAlloc for Probe {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            record(layout.size(), 0, true);
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            record(layout.size(), 0, true);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        record(0, layout.size(), false);
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let new = unsafe { System.realloc(ptr, layout, size) };
        if !new.is_null() {
            record(size, layout.size(), true);
        }
        new
    }
}

pub(crate) fn measure<T>(prepare: impl FnOnce() -> T) -> (T, Stats) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            ACTIVE.with(|active| active.set(None));
        }
    }
    ACTIVE.with(|active| {
        assert!(active.get().is_none(), "allocation probes cannot nest");
        active.set(Some(Stats::default()));
    });
    let reset = Reset;
    let result = prepare();
    let stats = ACTIVE.with(|active| active.get().unwrap());
    drop(reset);
    (result, stats)
}
