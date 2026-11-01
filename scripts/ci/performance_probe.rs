//! Fixed A/B workload copied unchanged into both revisions by performance_report.py.
//! CPU preparation only; no native window, ConPTY session or presentation latency.
use harbor_terminal::{
    RenderViewport, Terminal, TerminalAppearance, TerminalGpuAccess, Text, TextMetrics,
};
use std::hint::black_box;
use std::time::Instant;

fn terminal(rows: usize, cols: usize, history: usize) -> Terminal {
    Terminal::new_headless_with_history(
        rows,
        cols,
        TerminalAppearance::default(),
        harbor_config::HistoryLimits {
            max_rows: history,
            max_bytes: 4 * 1024 * 1024,
        },
    )
}

fn emit(case: &str, samples: &[f64]) {
    println!("HARBOR_PERF {{\"case\":\"{case}\",\"samples_ns\":{samples:?}}}");
}

fn ingest() {
    for (case, line) in [
        (
            "ingest_ascii",
            "status task abcdefghijklmnopqrstuvwxyz 0123456789\r\n",
        ),
        (
            "ingest_ansi",
            "\x1b[36mstatus\x1b[0m task abcdefghijklmnopqrstuvwxyz\r\n",
        ),
        (
            "ingest_unicode",
            "界面 e\u{301} ❤\u{fe0f} 👩\u{200d}💻 😀 status\r\n",
        ),
    ] {
        let input = line.repeat(2000);
        let mut samples = Vec::new();
        for iteration in 0..14 {
            let mut engine = terminal(24, 80, 4000);
            let start = Instant::now();
            for chunk in input.as_bytes().chunks(4096) {
                engine.put_bytes(black_box(chunk));
            }
            let elapsed = start.elapsed().as_nanos() as f64;
            black_box(engine.screen());
            if iteration >= 3 {
                samples.push(elapsed);
            }
        }
        emit(case, &samples);
    }
}

fn updates() {
    for sessions in [1, 8] {
        for mode in ["clean", "local"] {
            let mut engines: Vec<_> = (0..sessions).map(|_| terminal(24, 80, 0)).collect();
            let now = Instant::now();
            for engine in &mut engines {
                let update = engine.read_update(now);
                assert!(engine.acknowledge_update(&update));
            }
            // Exercise copy-on-write with a retained, old projection.
            let old: Vec<_> = engines.iter().map(|e| e.read_update(now)).collect();
            let mut samples = Vec::new();
            let iterations = 2000;
            for sample in 0..14 {
                let start = Instant::now();
                for i in 0..iterations {
                    for engine in &mut engines {
                        if mode == "local" {
                            engine.put_bytes(black_box(if i % 2 == 0 {
                                b"\x1b[1;1HA"
                            } else {
                                b"\x1b[1;1HB"
                            }));
                        }
                        let update = black_box(engine.read_update(now));
                        assert!(engine.acknowledge_update(&update));
                    }
                }
                let elapsed = start.elapsed().as_nanos() as f64 / (iterations * sessions) as f64;
                if sample >= 3 {
                    samples.push(elapsed);
                }
            }
            assert!(old.iter().all(|update| update.snapshot.cells[0].ch == ' '));
            emit(&format!("update_{mode}_{sessions}"), &samples);
        }
    }
}

fn resize() {
    for (kind, line) in [
        ("ascii", format!("{}\r\n", "abcdefghij".repeat(14))),
        ("unicode", format!("{}\r\n", "界e\u{301}😀".repeat(24))),
    ] {
        for cols in [80, 200] {
            let mut samples = Vec::new();
            for iteration in 0..14 {
                let mut engine = terminal(24, 160, 4000);
                engine.put_str(&line.repeat(4024));
                let start = Instant::now();
                engine.resize(24, cols);
                let elapsed = start.elapsed().as_nanos() as f64;
                assert_eq!(engine.screen().cols(), cols);
                black_box(engine.screen());
                if iteration >= 3 {
                    samples.push(elapsed);
                }
            }
            emit(&format!("resize_{kind}_{cols}"), &samples);
        }
    }
    let mut samples = Vec::new();
    for iteration in 0..14 {
        let mut engine = terminal(24, 160, 4000);
        engine.put_str("\x1b[?1049h");
        engine.put_str(&"alternate 界 😀\r\n".repeat(24));
        let start = Instant::now();
        engine.resize(40, 80);
        let elapsed = start.elapsed().as_nanos() as f64;
        assert_eq!(engine.screen().cols(), 80);
        if iteration >= 3 {
            samples.push(elapsed);
        }
    }
    emit("resize_alternate", &samples);
}

fn prepare() -> anyhow::Result<()> {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::DX12;
    let instance = wgpu::Instance::new(descriptor);
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        force_fallback_adapter: true,
        ..Default::default()
    }))?;
    println!("WARP adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))?;
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    for fixture in ["ordinary", "emoji"] {
        for mode in ["clean", "local", "churn", "full"] {
            let fonts = harbor_text::load_system_fonts(&harbor_config::FontSettings {
                family: Some("Consolas".into()),
                size: 16.0,
            })?;
            let metrics = TextMetrics::from_font_metrics(fonts.font_metrics());
            let viewport = RenderViewport::with_surface(
                metrics.cell_width,
                metrics.line_height,
                (1024, 768),
                (1024, 768),
            );
            let mut engine = terminal(24, 80, 0);
            engine.put_str("xy");
            if fixture == "emoji" {
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
            let mut samples = Vec::new();
            for i in 0..121 {
                match mode {
                    "local" => engine.put_str(if i % 2 == 0 {
                        "\x1b[1;1Hx"
                    } else {
                        "\x1b[1;1Hy"
                    }),
                    "churn" => engine.put_str(if i % 2 == 0 {
                        "\x1b[1;1H😀"
                    } else {
                        "\x1b[1;1H👩‍💻"
                    }),
                    "full" => text.invalidate_projection(),
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
                let elapsed = start.elapsed().as_nanos() as f64;
                if i >= 20 {
                    samples.push(elapsed);
                }
                // GPU completion and engine acknowledgement are outside the timer.
                queue.submit([]);
                device.poll(wgpu::PollType::wait_indefinitely())?;
                assert!(engine.acknowledge_update(&update));
            }
            emit(&format!("prepare_{fixture}_{mode}"), &samples);
        }
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("core") => {
            ingest();
            updates();
            resize();
        }
        Some("prepare") => prepare()?,
        _ => anyhow::bail!("usage: pr_performance core | prepare"),
    }
    Ok(())
}
