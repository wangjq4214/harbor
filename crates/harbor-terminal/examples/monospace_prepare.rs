//! Reproducible prepare-only microbenchmark; not end-to-end frame latency.
//! cargo run --release -p harbor-terminal --example monospace_prepare
use harbor_terminal::{Terminal, TerminalGpuAccess, TerminalRenderPipeline, TextMetrics};
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    println!("adapter={:?}", adapter.get_info());
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))?;
    let fonts = harbor_text::load_system_fonts(&harbor_config::FontSettings {
        family: Some("Consolas".into()),
        size: 16.0,
    })?;
    let metrics = TextMetrics::from_font_metrics(fonts.font_metrics());
    println!(
        "font=Consolas requested; cell={}x{} ascent={}; grid=80x24; dpi=96; profile=release",
        metrics.cell_width, metrics.line_height, metrics.ascent
    );
    let mut terminal = Terminal::new_headless(24, 80);
    for _ in 0..24 {
        terminal.put_str(&"abcdefghijklmnopqrst".repeat(4));
    }
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let initial = terminal.read_update(Instant::now());
    let mut pipeline = TerminalRenderPipeline::new(gpu, (800, 600), fonts, metrics, &initial)?;
    pipeline.prepare(gpu, &initial, false);
    terminal.acknowledge_update(&initial);
    for mode in ["clean", "one_cell", "full"] {
        let mut timings = Vec::new();
        for i in 0..2200 {
            if mode == "one_cell" {
                terminal.process_output(
                    format!("\x1b[1;1H{}", if i % 2 == 0 { "x" } else { "y" }).as_bytes(),
                );
            } else if mode == "full" {
                terminal.invalidate_update();
            }
            let update = terminal.read_update(Instant::now());
            let start = Instant::now();
            pipeline.prepare(gpu, &update, false);
            if i >= 200 {
                timings.push(start.elapsed().as_nanos());
            }
            queue.submit([]);
            device.poll(wgpu::PollType::wait_indefinitely())?;
            terminal.acknowledge_update(&update);
        }
        timings.sort_unstable();
        println!(
            "{mode}: samples=2000 median_ns={} p95_ns={}",
            timings[1000], timings[1900]
        );
    }
    println!(
        "presentation_stats={:?}",
        pipeline.text.presentation_stats()
    );
    println!(
        "ordinary R8 texture=2048x2048=4194304 bytes; grid text vertices=368640 bytes; microbenchmark excludes draw/PTY/readback"
    );
    Ok(())
}
