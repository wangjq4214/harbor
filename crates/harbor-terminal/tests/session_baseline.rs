//! Manual, GPU-free session/update throughput probe. Run with:
//! cargo test -p harbor-terminal --release --test session_baseline -- --ignored --nocapture
//! This measures synchronous parsing, screen mutation, snapshot construction and frame demand;
//! it does NOT measure a PTY, renderer, GPU upload, frame presentation or Windows UI latency.
#[path = "session_baseline/perf.rs"]
mod perf;

use harbor_terminal::Terminal;
use std::hint::black_box;
use std::time::{Duration, Instant};

const STEPS_PER_SESSION: usize = 400;
const SAMPLES: usize = 7;
const OUTPUT: &str = "\x1b[36mstatus\x1b[0m task 0014: abcdefghijklmnopqrstuvwxyz 0123456789\r\n";

fn sample(sessions: usize) -> Duration {
    let mut terminals: Vec<_> = (0..sessions)
        .map(|_| Terminal::new_headless(24, 80))
        .collect();
    let start = Instant::now();
    for step in 0..STEPS_PER_SESSION {
        for terminal in &mut terminals {
            terminal.put_str(black_box(OUTPUT));
            black_box(terminal.snapshot());
            black_box(terminal.frame_demand(Instant::now()));
        }
        black_box(step);
    }
    black_box(terminals);
    start.elapsed()
}
