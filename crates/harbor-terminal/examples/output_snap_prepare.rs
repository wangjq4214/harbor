//! Warm CPU microbenchmark of snapped output -> read -> prepare -> acknowledge.
//! No PTY, draw, surface presentation, or GPU completion time in measured intervals.
//! cargo run --release -p harbor-terminal --example output_snap_prepare
#[cfg(feature = "renderer")]
fn main() -> anyhow::Result<()> {
    use harbor_terminal::{Terminal, TerminalGpuAccess, TerminalRenderPipeline, TextMetrics};
    use std::{hint::black_box, time::Instant};

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))?;
    println!("adapter={:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))?;
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    const WARMUP: usize = 50;
    const SAMPLES: usize = 200;
    const ROUNDS: usize = 5;
    println!(
        "profile=release font=Consolas/16 dpi=96 sessions=1 warmup={WARMUP} samples={SAMPLES} rounds={ROUNDS}"
    );
    for (rows, cols) in [(24, 80), (50, 160)] {
        for mode in ["clean", "one_cell", "control_only", "eight_chunks", "full"] {
            let fonts = harbor_text::load_system_fonts(&harbor_config::FontSettings {
                family: Some("Consolas".into()),
                size: 16.0,
            })?;
            let metrics = TextMetrics::from_font_metrics(fonts.font_metrics());
            let mut terminal = Terminal::new_headless(rows, cols);
            // Dense stable ASCII content, with both edit glyphs cached before timing.
            for row in 1..=rows {
                terminal.put_str(&format!("\x1b[{row};1H{}", "xy".repeat(cols / 2)));
            }
            terminal.put_bytes(b"\x1b[1;1H");
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
            for round in 0..ROUNDS {
                let mut timings: [Vec<u128>; 5] =
                    std::array::from_fn(|_| Vec::with_capacity(SAMPLES));
                let mut dirty_cells = 0;
                for i in 0..WARMUP + SAMPLES {
                    let start = Instant::now();
                    match mode {
                        "one_cell" | "eight_chunks" => {
                            let bytes: &[u8] = if i % 2 == 0 { b"\rx" } else { b"\ry" };
                            for _ in 0..if mode == "eight_chunks" { 8 } else { 1 } {
                                terminal.process_output(black_box(bytes));
                            }
                        }
                        "control_only" => terminal.process_output(black_box(b"\x1b[0m")),
                        "full" => terminal.invalidate_update(),
                        _ => {}
                    }
                    let ingested = Instant::now();
                    let update = terminal.read_update(ingested);
                    let read = Instant::now();
                    let prepared = pipeline.prepare(gpu, &update, false);
                    let prepare = Instant::now();
                    let acknowledged = terminal.acknowledge_update(&update);
                    let end = Instant::now();
                    anyhow::ensure!(prepared && acknowledged);
                    if i >= WARMUP {
                        for (target, duration) in timings.iter_mut().zip([
                            ingested - start,
                            read - ingested,
                            prepare - read,
                            end - prepare,
                            end - start,
                        ]) {
                            target.push(duration.as_nanos());
                        }
                    }
                    dirty_cells = update
                        .snapshot
                        .dirty_ranges
                        .iter()
                        .map(|r| r.end_col - r.start_col)
                        .sum::<usize>();
                    // Bound in-flight staging; completion is deliberately outside CPU timing.
                    queue.submit([]);
                    device.poll(wgpu::PollType::wait_indefinitely())?;
                    black_box(update);
                }
                let stats: Vec<_> = timings
                    .iter_mut()
                    .map(|t| {
                        t.sort_unstable();
                        (t[SAMPLES / 2], t[SAMPLES * 95 / 100])
                    })
                    .collect();
                println!(
                    "grid={cols}x{rows} mode={mode} round={round} dirty_cells={dirty_cells} ingest_ns={:?} read_ns={:?} prepare_ns={:?} ack_ns={:?} total_ns={:?}",
                    stats[0], stats[1], stats[2], stats[3], stats[4]
                );
            }
        }
    }
    Ok(())
}

#[cfg(not(feature = "renderer"))]
fn main() {
    eprintln!("output_snap_prepare requires --features renderer");
}
