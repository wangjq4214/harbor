//! Diagnostic ingestion-only comparison. No PTY, fonts, GPU or presentation.
//! cargo run --release -p harbor-terminal --example output_ingest_probe -- INPUT row 3
use anyhow::{Context, ensure};
use harbor_terminal::{Screen, Terminal, TerminalParser};
use std::hint::black_box;
use std::io::Read;
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() == 4 || args.len() == 5,
        "usage: output_ingest_probe INPUT row|CHUNK_BYTES ROUNDS [parser|engine]"
    );
    let mut input = Vec::new();
    std::fs::File::open(&args[1])
        .context("open synthetic input")?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut input)
        .context("read bounded synthetic input")?;
    ensure!(
        input.len() <= 16 * 1024 * 1024,
        "input exceeds 16 MiB diagnostic bound"
    );
    let rounds: usize = args[3].parse()?;
    ensure!((1..=9).contains(&rounds), "rounds must be 1..9");
    let chunks: Vec<&[u8]> = if args[2] == "row" {
        input.split_inclusive(|b| *b == b'\n').collect()
    } else {
        let size: usize = args[2].parse()?;
        ensure!((1..=65536).contains(&size), "chunk must be 1..65536");
        input.chunks(size).collect()
    };
    println!(
        "boundary=ingestion_only grid=107x32 bytes={} chunks={} chunk_mode={} rounds={rounds}",
        input.len(),
        chunks.len(),
        args[2]
    );
    for boundary in ["parser_screen", "terminal_engine"] {
        let selected = args.get(4).map(String::as_str);
        ensure!(
            selected.is_none_or(|s| matches!(s, "parser" | "engine")),
            "invalid boundary"
        );
        if selected.is_some_and(|s| (s == "parser") != (boundary == "parser_screen")) {
            continue;
        }
        for round in 0..rounds {
            let mut terminal = Terminal::new_headless(32, 107);
            let mut screen = Screen::new(32, 107);
            let mut parser = TerminalParser::default();
            let start = Instant::now();
            for chunk in &chunks {
                if boundary == "parser_screen" {
                    parser.put_bytes(&mut screen, black_box(chunk));
                } else {
                    terminal.process_output(black_box(chunk));
                }
            }
            let elapsed = start.elapsed();
            let target = if boundary == "parser_screen" {
                &screen
            } else {
                terminal.screen()
            };
            let last = target.row_text(30);
            println!(
                "boundary={boundary} round={round} elapsed_ns={} tail_prefix={:?}",
                elapsed.as_nanos(),
                last.chars().take(6).collect::<String>()
            );
            black_box(target);
        }
    }
    Ok(())
}
