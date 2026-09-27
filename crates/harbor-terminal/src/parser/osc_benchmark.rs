//! Reproducible parser-throughput probe for OSC refactors; run in release mode.
use super::TerminalParser;
use crate::screen::Screen;
use std::hint::black_box;
use std::time::Instant;

#[test]
#[ignore = "manual release-mode throughput measurement"]
fn osc_throughput_probe() {
    const ROUNDS: usize = 30;
    const SAMPLES: usize = 9;
    let sequence = b"x\x1b]0;harbor\x07\x1b]7;file:///tmp\x1b\\\x1b]8;id=a;https://example.test\x07y\x1b]8;;\x1b\\\x1b]10;#123456\x07\x1b]10;?\x1b\\\x1b]133;A\x07z\x1b]999;ignored\x07";
    let input = sequence.repeat(1024);
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(4, 80);
    let mut run = || {
        let start = Instant::now();
        for _ in 0..ROUNDS {
            parser.put_bytes(&mut screen, black_box(&input));
            black_box(parser.drain_output_events());
            black_box(screen.drain_replies());
        }
        start.elapsed().as_secs_f64()
    };
    black_box(run()); // warmup outside the recorded distribution
    let mut samples = (0..SAMPLES).map(|_| run()).collect::<Vec<_>>();
    samples.sort_by(f64::total_cmp);
    let bytes = (input.len() * ROUNDS) as f64;
    println!(
        "OSC throughput: median {:.2} MiB/s, samples {:?} s, bytes/sample {}, rounds {}, input bytes {}",
        bytes / samples[SAMPLES / 2] / (1024.0 * 1024.0),
        samples,
        bytes as usize,
        ROUNDS,
        input.len()
    );
}
