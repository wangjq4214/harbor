//! Reproducible CPU-only mutation/read/ack timing; no GPU or PTY measurements.
use harbor_terminal::{Terminal, TerminalAppearance};
use std::hint::black_box;
use std::time::{Duration, Instant};

fn main() {
    let iterations = 200;
    println!("grid,workload,iterations,mutation_us,read_us,ack_us");
    for (rows, cols) in [(24, 80), (80, 240), (160, 400)] {
        for local in [false, true] {
            let mut terminal = Terminal::new_headless_with_history(
                rows,
                cols,
                TerminalAppearance::from_palette(harbor_config::Palette::default()),
                harbor_config::HistoryLimits {
                    max_rows: 0,
                    max_bytes: 0,
                },
            );
            let now = Instant::now();
            let initial = terminal.read_update(now);
            assert!(terminal.acknowledge_update(&initial));
            // Keep an old coherent update alive throughout the workload.
            let retained = terminal.read_update(now);
            let (mut mutation, mut read, mut ack) =
                (Duration::ZERO, Duration::ZERO, Duration::ZERO);
            for i in 0..iterations {
                let start = Instant::now();
                if local {
                    terminal.put_bytes(if i % 2 == 0 { b"\rA" } else { b"\rB" });
                }
                mutation += start.elapsed();
                let start = Instant::now();
                let update = black_box(terminal.read_update(now));
                read += start.elapsed();
                let start = Instant::now();
                assert!(terminal.acknowledge_update(black_box(&update)));
                ack += start.elapsed();
                black_box(update);
            }
            black_box(retained);
            let us = |d: Duration| d.as_secs_f64() * 1_000_000.0 / f64::from(iterations);
            println!(
                "{rows}x{cols},{},{iterations},{:.3},{:.3},{:.3}",
                if local { "local" } else { "unchanged" },
                us(mutation),
                us(read),
                us(ack)
            );
        }
    }
}
