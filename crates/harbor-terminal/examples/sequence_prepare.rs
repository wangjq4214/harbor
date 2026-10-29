//! Text preparation only, with explicit bounded engine damage (no scroll-to-bottom snap).
//! cargo run --release -p harbor-terminal --example sequence_prepare -- 24
use harbor_terminal::{RenderViewport, Terminal, TerminalGpuAccess, Text, TextMetrics};
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let rows: usize = std::env::args().nth(1).unwrap_or("24".into()).parse()?;
    anyhow::ensure!(rows >= 2, "at least two rows required");
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
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
        let mut engine = Terminal::new_headless(rows, 80);
        engine.put_str("xy");
        if fixture == "persistent_emoji" {
            engine.put_str("\r\n😀");
        }
        let update = engine.read_update(Instant::now());
        let mut text = Text::new(
            gpu,
            fonts,
            metrics,
            &update.snapshot,
            &viewport,
            Default::default(),
        )?;
        assert!(engine.acknowledge_update(&update));
        println!(
            "fixture={fixture}; grid={rows}x80; Consolas 16px, DPI=96; text-only prepare; release"
        );
        for mode in ["clean", "bounded", "sequence_churn", "full"] {
            let mut times = Vec::new();
            for i in 0..600 {
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
                if i >= 100 {
                    times.push(start.elapsed().as_nanos());
                }
                queue.submit([]);
                device.poll(wgpu::PollType::wait_indefinitely())?;
                assert!(engine.acknowledge_update(&update));
            }
            times.sort_unstable();
            println!(
                "{mode}: n=500 median_ns={} p95_ns={}; stats={:?}",
                times[250],
                times[475],
                text.presentation_stats()
            );
        }
    }
    Ok(())
}
