//! Reproducible parser-throughput probe for OSC refactors; run in release mode.
#[path = "osc_benchmark/perf.rs"]
mod perf;

use super::TerminalParser;
use crate::screen::Screen;
use std::hint::black_box;
use std::time::Instant;
