//! Comparable CPU preparation/allocation benchmark; no draw or GPU wait in timing.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
#[cfg(feature = "renderer")]
use std::{hint::black_box, time::Instant};

thread_local! {
    // Calling-thread allocation requests, not live/peak memory or driver-thread allocations.
    static ALLOCS: Cell<Option<(usize, usize)>> = const { Cell::new(None) };
}
struct CountingAllocator;
fn record(bytes: usize) {
    let _ = ALLOCS.try_with(|counts| {
        if let Some((calls, requested)) = counts.get() {
            counts.set(Some((calls + 1, requested + bytes)));
        }
    });
}
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record(size);
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[cfg(feature = "renderer")]
fn main() -> anyhow::Result<()> {
    use harbor_terminal::{Terminal, TerminalGpuAccess, TerminalRenderPipeline};
    use harbor_text::TextMetrics;
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))?;
    println!("adapter={:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))?;
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    const WARMUP: usize = 20;
    const SAMPLES: usize = 100;
    const ROUNDS: usize = 3;
    println!(
        "profile=release font=Consolas/16 scale=1 sessions=1 warmup={WARMUP} samples={SAMPLES} rounds={ROUNDS} allocations=calling_thread_prepare_requests"
    );
    for (rows, cols) in [(24, 80), (50, 160)] {
        for mode in [
            "ascii_full",
            "ascii_scroll",
            "ascii_edit",
            "mixed_full",
            "mixed_edit",
        ] {
            let fonts = harbor_text::load_system_fonts(&harbor_config::FontSettings {
                family: Some("Consolas".into()),
                size: 16.0,
            })?;
            let metrics = TextMetrics::from_font_metrics(fonts.font_metrics());
            let mut terminal = Terminal::new_headless(rows, cols);
            let mixed = mode.starts_with("mixed");
            let line = if mixed {
                "┼".repeat(cols)
            } else {
                "xy".repeat(cols / 2)
            };
            for row in 1..=rows {
                terminal.put_str(&format!(
                    "\x1b[{row};1H{}{}",
                    if mixed { "\x1b[41;4:3;9;53m" } else { "" },
                    line
                ));
            }
            terminal.put_str("\x1b[0m");
            let initial = terminal.read_update(Instant::now());
            let surface = (
                (cols as f32 * metrics.cell_width + 32.0).ceil() as u32,
                (rows as f32 * metrics.line_height + 32.0).ceil() as u32,
            );
            let mut pipeline = TerminalRenderPipeline::new(gpu, surface, fonts, metrics, &initial)?;
            anyhow::ensure!(pipeline.prepare(gpu, &initial, false));
            anyhow::ensure!(terminal.acknowledge_update(&initial));
            queue.submit([]);
            device.poll(wgpu::PollType::wait_indefinitely())?;
            let scroll = format!("\r\n{}", "xy".repeat(cols / 2));
            for round in 0..ROUNDS {
                let mut times = Vec::with_capacity(SAMPLES);
                let mut calls = Vec::with_capacity(SAMPLES);
                let mut bytes = Vec::with_capacity(SAMPLES);
                for i in 0..WARMUP + SAMPLES {
                    match mode {
                        "ascii_full" | "mixed_full" => terminal.invalidate_update(),
                        "ascii_scroll" => terminal.process_output(scroll.as_bytes()),
                        "ascii_edit" => terminal.process_output(if i % 2 == 0 {
                            b"\x1b[1;1Hx"
                        } else {
                            b"\x1b[1;1Hy"
                        }),
                        "mixed_edit" => terminal.process_output(if i % 2 == 0 {
                            "\x1b[1;1H\x1b[41;4:3;9;53m┼\x1b[0m".as_bytes()
                        } else {
                            b"\x1b[1;1H\x1b[0mx"
                        }),
                        _ => unreachable!(),
                    }
                    let update = terminal.read_update(Instant::now());
                    ALLOCS.with(|a| a.set(Some((0, 0))));
                    let start = Instant::now();
                    let prepared = pipeline.prepare(gpu, black_box(&update), false);
                    let elapsed = start.elapsed().as_nanos();
                    let allocs = ALLOCS.with(|a| a.take().unwrap());
                    anyhow::ensure!(prepared && terminal.acknowledge_update(&update));
                    if i >= WARMUP {
                        times.push(elapsed);
                        calls.push(allocs.0);
                        bytes.push(allocs.1);
                    }
                    queue.submit([]);
                    device.poll(wgpu::PollType::wait_indefinitely())?;
                }
                times.sort_unstable();
                calls.sort_unstable();
                bytes.sort_unstable();
                println!(
                    "grid={cols}x{rows} mode={mode} round={round} prepare_ns={} p95_ns={} alloc_calls={} alloc_requested_bytes={}",
                    times[SAMPLES / 2],
                    times[SAMPLES * 95 / 100],
                    calls[SAMPLES / 2],
                    bytes[SAMPLES / 2]
                );
            }
        }
    }
    Ok(())
}

#[cfg(not(feature = "renderer"))]
fn main() {
    eprintln!("vertex_staging requires --features renderer");
}
