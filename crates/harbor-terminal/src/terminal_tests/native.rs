use super::*;

#[cfg(windows)]
#[test]
#[ignore = "requires installed nvim.exe and real ConPTY; run explicitly"]
fn live_nvim_async_resize_keeps_alternate_app_responsive() {
    fn wait_until(
        terminal: &mut Terminal,
        wake: &std::sync::mpsc::Receiver<()>,
        predicate: impl Fn(&Terminal) -> bool,
    ) {
        let deadline = Instant::now() + std::time::Duration::from_secs(10);
        loop {
            terminal.drain_pty();
            if predicate(terminal) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "nvim did not respond: {:?}",
                terminal.snapshot()
            );
            assert!(
                !terminal.is_session_closed(),
                "nvim exited before the interaction completed"
            );
            let _ = wake.recv_timeout(std::time::Duration::from_millis(10));
        }
    }
    let command = harbor_pty::ShellCommand::new(
        Some("nvim.exe".to_owned()),
        vec![
            "--clean".into(),
            "-n".into(),
            "-i".into(),
            "NONE".into(),
            "-c".into(),
            "call setline(1, 'HARBOR_NVIM_BUFFER')".into(),
        ],
    );
    let endpoints = harbor_pty::PtyEndpoints::spawn_shell(
        harbor_pty::TerminalSize { rows: 24, cols: 80 },
        &command,
    )
    .expect("dedicated nvim PTY");
    assert_eq!(
        endpoints.shell_name().to_ascii_lowercase(),
        "nvim",
        "do not count fallback shell as nvim"
    );
    let (tx, rx) = std::sync::mpsc::channel();
    let mut terminal = Terminal::new_headless(24, 80)
        .start_session_from_endpoints(endpoints, move || tx.send(()).is_ok())
        .unwrap();
    let visible = |terminal: &Terminal| {
        (0..terminal.screen().rows())
            .map(|row| terminal.row_text(row))
            .collect::<Vec<_>>()
            .join("\n")
    };
    wait_until(&mut terminal, &rx, |terminal| {
        terminal.is_alt_screen() && visible(terminal).contains("HARBOR_NVIM_BUFFER")
    });
    for (index, (rows, cols)) in [(20, 60), (25, 90), (15, 40), (24, 80)]
        .into_iter()
        .enumerate()
    {
        resize_to_completion(&mut terminal, TerminalSize { rows, cols }).unwrap();
        let marker = format!("NVIM_RESIZED_{index}");
        terminal
            .write_pty(format!("\x1b:echo '{marker}'\r").as_bytes())
            .unwrap();
        wait_until(&mut terminal, &rx, |terminal| {
            visible(terminal).contains(&marker)
        });
        assert!(terminal.is_alt_screen());
        assert!(visible(&terminal).contains("HARBOR_NVIM_BUFFER"));
    }
    terminal.write_pty(b"\x1b:q!\r").unwrap();
    // Leaving the alternate screen proves nvim processed quit. ConPTY's output
    // pipe can remain open while its owner is alive; EOF is not the app-exit handshake.
    wait_until(&mut terminal, &rx, |terminal| !terminal.is_alt_screen());
    assert!(!terminal.resize_pending());
    drop(terminal); // Platform shutdown owns cancellation of the remaining blocked read.
}
