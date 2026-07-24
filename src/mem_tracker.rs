//! Memory and stage event tracking infrastructure for Harbor.
//!
//! When the `mem-trace` feature is enabled, a custom global allocator wrapper
//! tracks Rust `GlobalAlloc` requests, deallocations, and active live bytes.
//! Also queries OS-level Working Set (RSS) and Private Bytes on Windows / Linux,
//! along with stage event counters for PTY reads, worker snapshots, and GPU updates.

use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(feature = "mem-trace")]
use std::alloc::{GlobalAlloc, Layout, System};

#[cfg(feature = "mem-trace")]
pub struct TracedAllocator;

static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);
static FREED_BYTES: AtomicU64 = AtomicU64::new(0);
static LIVE_BYTES: AtomicU64 = AtomicU64::new(0);
static PEAK_LIVE_BYTES: AtomicU64 = AtomicU64::new(0);
static ALLOC_COUNT: AtomicU64 = AtomicU64::new(0);
static FREE_COUNT: AtomicU64 = AtomicU64::new(0);

// Stage event counters
static PTY_READ_CHUNKS: AtomicU64 = AtomicU64::new(0);
static PTY_READ_BYTES: AtomicU64 = AtomicU64::new(0);
static WORKER_SNAPSHOTS: AtomicU64 = AtomicU64::new(0);
static UI_UPDATES_PREPARED: AtomicU64 = AtomicU64::new(0);
static FRAMES_RENDERED: AtomicU64 = AtomicU64::new(0);

#[cfg(feature = "mem-trace")]
unsafe impl GlobalAlloc for TracedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            let size = layout.size() as u64;
            ALLOCATED_BYTES.fetch_add(size, Ordering::Relaxed);
            ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
            let live = LIVE_BYTES.fetch_add(size, Ordering::Relaxed) + size;

            // Update peak live bytes atomically without allocation or locking
            let mut peak = PEAK_LIVE_BYTES.load(Ordering::Relaxed);
            while live > peak {
                match PEAK_LIVE_BYTES.compare_exchange_weak(
                    peak,
                    live,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => break,
                    Err(actual) => peak = actual,
                }
            }
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        let size = layout.size() as u64;
        FREED_BYTES.fetch_add(size, Ordering::Relaxed);
        LIVE_BYTES.fetch_sub(size, Ordering::Relaxed);
        FREE_COUNT.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(feature = "mem-trace")]
#[global_allocator]
pub static GLOBAL_ALLOCATOR: TracedAllocator = TracedAllocator;

/// Increments the PTY read chunk counter and byte count when Worker receives PTY bytes.
pub fn record_pty_read(bytes: usize) {
    PTY_READ_CHUNKS.fetch_add(1, Ordering::Relaxed);
    PTY_READ_BYTES.fetch_add(bytes as u64, Ordering::Relaxed);
}

/// Increments the worker snapshot build counter.
pub fn record_worker_snapshot() {
    WORKER_SNAPSHOTS.fetch_add(1, Ordering::Relaxed);
}

/// Increments the UI update preparation counter.
pub fn record_ui_update_prepared() {
    UI_UPDATES_PREPARED.fetch_add(1, Ordering::Relaxed);
}

/// Increments the CPU-side present call completed counter.
/// Note: GPU VRAM physical rendering and display residency require OS ETW/WPA/GPU profiler tools.
pub fn record_frame_rendered() {
    FRAMES_RENDERED.fetch_add(1, Ordering::Relaxed);
}

/// OS-level process memory info (Working Set / RSS and Virtual / Commit Memory).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OsMemoryInfo {
    /// Working Set Size (RSS) in bytes.
    pub working_set_bytes: u64,
    /// Virtual Memory Size (Linux) or Private Commit Usage (Windows) in bytes.
    pub virtual_or_commit_bytes: u64,
}

/// Returns the OS process memory information for the current process.
pub fn query_os_memory_info() -> OsMemoryInfo {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX,
        };
        use windows::Win32::System::Threading::GetCurrentProcess;

        let mut counters = PROCESS_MEMORY_COUNTERS_EX::default();
        let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        let success = unsafe {
            GetProcessMemoryInfo(GetCurrentProcess(), &mut counters as *mut _ as *mut _, size)
        };
        if success.is_ok() {
            return OsMemoryInfo {
                working_set_bytes: counters.WorkingSetSize as u64,
                virtual_or_commit_bytes: counters.PrivateUsage as u64,
            };
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            let mut working_set_bytes = 0u64;
            let mut virtual_or_commit_bytes = 0u64;
            for line in status.lines() {
                if let Some(val) = line.strip_prefix("VmRSS:") {
                    working_set_bytes = parse_kb(val);
                } else if let Some(val) = line.strip_prefix("VmSize:") {
                    virtual_or_commit_bytes = parse_kb(val);
                }
            }
            if working_set_bytes > 0 || virtual_or_commit_bytes > 0 {
                return OsMemoryInfo {
                    working_set_bytes,
                    virtual_or_commit_bytes,
                };
            }
        }
    }

    OsMemoryInfo::default()
}

#[cfg(target_os = "linux")]
fn parse_kb(s: &str) -> u64 {
    s.trim()
        .trim_end_matches("kB")
        .trim()
        .parse::<u64>()
        .unwrap_or(0)
        * 1024
}

/// Memory statistics snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemorySnapshot {
    pub rust_global_alloc_live_bytes: u64,
    pub rust_global_alloc_peak_bytes: u64,
    pub rust_allocated_bytes: u64,
    pub rust_freed_bytes: u64,
    pub rust_alloc_count: u64,
    pub rust_free_count: u64,
    pub os_working_set_bytes: u64,
    pub os_virtual_or_commit_bytes: u64,
    pub pty_read_chunks: u64,
    pub pty_read_bytes: u64,
    pub worker_snapshots: u64,
    pub ui_updates_prepared: u64,
    pub frames_rendered: u64,
}

impl MemorySnapshot {
    pub fn capture() -> Self {
        let os_mem = query_os_memory_info();
        Self {
            rust_global_alloc_live_bytes: LIVE_BYTES.load(Ordering::Relaxed),
            rust_global_alloc_peak_bytes: PEAK_LIVE_BYTES.load(Ordering::Relaxed),
            rust_allocated_bytes: ALLOCATED_BYTES.load(Ordering::Relaxed),
            rust_freed_bytes: FREED_BYTES.load(Ordering::Relaxed),
            rust_alloc_count: ALLOC_COUNT.load(Ordering::Relaxed),
            rust_free_count: FREE_COUNT.load(Ordering::Relaxed),
            os_working_set_bytes: os_mem.working_set_bytes,
            os_virtual_or_commit_bytes: os_mem.virtual_or_commit_bytes,
            pty_read_chunks: PTY_READ_CHUNKS.load(Ordering::Relaxed),
            pty_read_bytes: PTY_READ_BYTES.load(Ordering::Relaxed),
            worker_snapshots: WORKER_SNAPSHOTS.load(Ordering::Relaxed),
            ui_updates_prepared: UI_UPDATES_PREPARED.load(Ordering::Relaxed),
            frames_rendered: FRAMES_RENDERED.load(Ordering::Relaxed),
        }
    }

    pub fn report(&self) -> String {
        let live_mb = self.rust_global_alloc_live_bytes as f64 / (1024.0 * 1024.0);
        let peak_mb = self.rust_global_alloc_peak_bytes as f64 / (1024.0 * 1024.0);
        let alloc_mb = self.rust_allocated_bytes as f64 / (1024.0 * 1024.0);
        let free_mb = self.rust_freed_bytes as f64 / (1024.0 * 1024.0);
        let rss_mb = self.os_working_set_bytes as f64 / (1024.0 * 1024.0);
        let os_secondary_mb = self.os_virtual_or_commit_bytes as f64 / (1024.0 * 1024.0);
        let pty_mb = self.pty_read_bytes as f64 / (1024.0 * 1024.0);

        let alloc_status = if cfg!(feature = "mem-trace") {
            format!(
                "rust_global_alloc_live={:.2}MB (peak={:.2}MB) rust_alloc_total={:.2}MB rust_freed_total={:.2}MB allocs={} frees={}",
                live_mb, peak_mb, alloc_mb, free_mb, self.rust_alloc_count, self.rust_free_count
            )
        } else {
            "rust_global_alloc=[disabled; compile with --features mem-trace]".to_string()
        };

        let secondary_label = if cfg!(target_os = "windows") {
            "os_commit"
        } else {
            "os_virtual"
        };

        format!(
            "mem_trace: {} os_rss={:.2}MB {}={:.2}MB pty_chunks={} pty_mb={:.2}MB worker_snapshots={} ui_updates={} frames={}",
            alloc_status,
            rss_mb,
            secondary_label,
            os_secondary_mb,
            self.pty_read_chunks,
            pty_mb,
            self.worker_snapshots,
            self.ui_updates_prepared,
            self.frames_rendered,
        )
    }
}
