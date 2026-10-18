//! GPU-free live Windows ConPTY host for verify_capabilities.py.
//! This exercises real child I/O and terminal input encoding, not rendering or physical keys.
use anyhow::{Context, bail};
use harbor_pty::{PtyEndpoints, ShellCommand};
use harbor_terminal::{
    Terminal, TerminalEvent, TerminalKey, TerminalKeyboardEvent, TerminalModifiers,
};
use std::collections::HashSet;
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let program = args
        .next()
        .context("usage: capability_probe_host PROGRAM [ARGS...] (Windows ConPTY)")?;
    let command = ShellCommand::new(Some(program), args.collect());
    let endpoints = PtyEndpoints::spawn_shell(
        harbor_pty::TerminalSize {
            rows: 40,
            cols: 160,
        },
        &command,
    )?;
    let (tx, wake) = mpsc::channel();
    let mut terminal = Terminal::new_headless(40, 160)
        .start_session_from_endpoints(endpoints, move || tx.send(()).is_ok())?;
    // Allow the guest's worst case: 20 queries + 12 keys at 5s, 45s Vim,
    // bounded metadata commands and restoration, so failure JSON can be written.
    let deadline = Instant::now() + Duration::from_secs(300);
    let mut sent = HashSet::new();
    loop {
        terminal.drain_pty();
        let lines: Vec<_> = (0..terminal.screen().rows())
            .map(|row| terminal.row_text(row))
            .collect();
        if lines
            .iter()
            .any(|line| line.trim() == "HARBOR_PROBE_DONE:PASS")
        {
            println!(
                "PASS: real ConPTY session completed; inspect the probe JSON for per-case evidence and NOT RUN cases."
            );
            return Ok(());
        }
        if lines
            .iter()
            .any(|line| line.trim() == "HARBOR_PROBE_DONE:FAIL")
        {
            bail!("probe reported FAIL; inspect its JSON report");
        }
        for (stage, mode) in [
            ("KEY", "normal"),
            ("KEY", "application"),
            ("VIM", "application"),
        ] {
            for (name, key) in [
                ("up", TerminalKey::ArrowUp),
                ("down", TerminalKey::ArrowDown),
                ("right", TerminalKey::ArrowRight),
                ("left", TerminalKey::ArrowLeft),
                ("home", TerminalKey::Home),
                ("end", TerminalKey::End),
            ] {
                let marker = format!("HARBOR_{stage}_READY:{mode}:{name}");
                if !sent.contains(&marker) && lines.iter().any(|line| line.trim() == marker) {
                    terminal.handle_event(TerminalEvent::Keyboard(
                        TerminalKeyboardEvent::KeyDown {
                            key,
                            modifiers: TerminalModifiers::default(),
                        },
                    ))?;
                    sent.insert(marker);
                }
            }
        }
        if terminal.is_session_closed() || Instant::now() >= deadline {
            bail!(
                "probe closed or exceeded 300s before its completion marker; last screen: {lines:?}"
            );
        }
        let _ = wake.recv_timeout(Duration::from_millis(10));
    }
}
