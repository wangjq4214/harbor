//! Release CPU timings, not UI/frame latency.
//! cargo run --release -p harbor-terminal --example hot_path_timing -- engine
//! cargo run --release -p harbor-terminal --example hot_path_timing -- sequence 24
use harbor_terminal::{
    RenderViewport, Terminal, TerminalAppearance, TerminalGpuAccess, Text, TextMetrics,
};
use std::hint::black_box;
use std::time::Instant;

fn engine() {
    for (rows, cols) in [(24, 80), (80, 240)] {
        for sessions in [1, 8] {
            for mode in ["unchanged", "local"] {
                let mut engines: Vec<_> = (0..sessions)
                    .map(|_| {
                        Terminal::new_headless_with_history(
                            rows,
                            cols,
                            TerminalAppearance::default(),
                            harbor_config::HistoryLimits {
                                max_rows: 0,
                                max_bytes: 0,
                            },
                        )
                    })
                    .collect();
                let now = Instant::now();
                for engine in &mut engines {
                    let first = engine.read_update(now);
                    assert!(engine.acknowledge_update(&first));
                }
                // Keep an old projection alive throughout the measurement.
                let old: Vec<_> = engines.iter().map(|e| e.read_update(now)).collect();
                let iterations = if sessions == 1 { 10_000 } else { 2_000 };
                let mut samples = Vec::new();
                for sample in 0..14 {
                    let start = Instant::now();
                    for i in 0..iterations {
                        for engine in &mut engines {
                            if mode == "local" {
                                engine.process_output(black_box(if i % 2 == 0 {
                                    b"\x1b[1;1HA"
                                } else {
                                    b"\x1b[1;1HB"
                                }));
                            }
                            let update = black_box(engine.read_update(now));
                            if mode == "local" {
                                assert!(engine.acknowledge_update(&update));
                            }
                            black_box(update);
                        }
                    }
                    let ns = start.elapsed().as_nanos() as f64 / (iterations * sessions) as f64;
                    if sample >= 3 {
                        samples.push(ns);
                    }
                }
                assert!(old.iter().all(|u| u.snapshot.cells[0].ch == ' '));
                samples.sort_by(f64::total_cmp);
                println!(
                    "TIMING engine rows={rows} cols={cols} sessions={sessions} mode={mode} batch_iterations={iterations} samples=11 median_ns={:.3} batch_p95_ns={:.3} samples_ns={samples:?}",
                    samples[5], samples[10]
                );
            }
        }
    }
}

fn sequence(rows: usize) -> anyhow::Result<()> {
    anyhow::ensure!(rows >= 2, "at least two rows required");
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::VULKAN;
    let instance = wgpu::Instance::new(descriptor);
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))?;
    println!("adapter={:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))?;
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    for fixture in ["ordinary", "persistent_emoji"] {
        let fonts = harbor_text::load_system_fonts(&harbor_config::FontSettings {
            family: Some("Consolas".into()),
            size: 16.0,
        })?;
        let metrics = TextMetrics::from_font_metrics(fonts.font_metrics());
        let viewport = RenderViewport::with_surface(
            metrics.cell_width,
            metrics.line_height,
            (1024, 8192),
            (1024, 8192),
        );
        let mut engine = Terminal::new_headless_with_history(
            rows,
            80,
            TerminalAppearance::default(),
            harbor_config::HistoryLimits {
                max_rows: 0,
                max_bytes: 0,
            },
        );
        engine.put_str("xy");
        if fixture == "persistent_emoji" {
            engine.put_str("\r\n😀");
        }
        let initial = engine.read_update(Instant::now());
        let mut text = Text::new(
            gpu,
            fonts,
            metrics,
            &initial.snapshot,
            &viewport,
            Default::default(),
        )?;
        assert!(engine.acknowledge_update(&initial));
        drop(initial);
        for mode in ["clean", "bounded", "sequence_churn"] {
            // Independent timer per prepare; GPU completion is deliberately outside it.
            let mut times = Vec::with_capacity(700);
            for i in 0..800 {
                match mode {
                    "bounded" => engine.put_str(if i % 2 == 0 {
                        "\x1b[1;1Hx"
                    } else {
                        "\x1b[1;1Hy"
                    }),
                    "sequence_churn" => engine.put_str(if i % 2 == 0 {
                        "\x1b[1;1H😀"
                    } else {
                        "\x1b[1;1H👩‍💻"
                    }),
                    _ => {}
                }
                let update = engine.read_update(Instant::now());
                let start = Instant::now();
                text.prepare_with_dirty(
                    gpu,
                    &update.snapshot,
                    &update.snapshot.dirty_ranges,
                    &viewport,
                    None,
                );
                let ns = start.elapsed().as_nanos();
                if i >= 100 {
                    times.push(ns);
                }
                queue.submit([]);
                device.poll(wgpu::PollType::wait_indefinitely())?;
                assert!(engine.acknowledge_update(&update));
            }
            times.sort_unstable();
            println!(
                "TIMING sequence rows={rows} cols=80 fixture={fixture} mode={mode} samples=700 median_ns={} p95_ns={} samples_ns={times:?}",
                times[350], times[665]
            );
        }
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("engine") => engine(),
        Some("sequence") => sequence(
            std::env::args()
                .nth(2)
                .unwrap_or_else(|| "24".into())
                .parse()?,
        )?,
        _ => anyhow::bail!("usage: hot_path_timing engine | sequence ROWS"),
    }
    Ok(())
}
