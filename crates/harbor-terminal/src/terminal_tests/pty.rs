use super::*;

#[cfg(windows)]
#[test]
#[ignore = "pty: real Windows ConPTY"]
fn live_conpty_rapid_resize_preserves_history_and_prompt() {
    let size = harbor_pty::TerminalSize { rows: 24, cols: 80 };
    let command = harbor_pty::ShellCommand::new(
        Some(format!(
            r"{}\System32\cmd.exe",
            std::env::var("SystemRoot").unwrap()
        )),
        Vec::new(),
    );
    let endpoints = harbor_pty::PtyEndpoints::spawn_shell(size, &command).unwrap();
    assert_eq!(endpoints.shell_name().to_ascii_lowercase(), "cmd");
    let (reader, writer, control) = endpoints.into_parts();
    let (wake_tx, wake_rx) = std::sync::mpsc::channel();
    let mut terminal = Terminal::new_headless(size.rows, size.cols);
    terminal.io = crate::io::TerminalIo::new(reader, writer, Some(control), move || {
        wake_tx.send(()).is_ok()
    });
    terminal.io.write_pty(b"@echo off\rcls & (for /L %i in (1,1,20) do @echo harbor-profile-%i 0123456789 abcdefghijklmnopqrstuvwxyz) & set /p answer=READY:&echo ACK:&set /p answer=\r").unwrap();
    let retained_text = |terminal: &Terminal| {
        let s = terminal.screen();
        s.selected_text(crate::SelectionBounds {
            start_row: s.history_start(),
            start_col: 0,
            end_row: s.history_start() + (s.scroll_count() + s.rows()) as u64 - 1,
            end_col: s.cols() - 1,
        })
    };
    let deadline = Instant::now() + std::time::Duration::from_secs(10);
    while !terminal.row_text(20).starts_with("READY:") {
        assert!(
            Instant::now() < deadline,
            "fixture should finish startup: {:?}",
            retained_text(&terminal)
        );
        let _ = wake_rx.recv_timeout(std::time::Duration::from_millis(20));
        terminal.drain_pty();
    }
    let before = retained_text(&terminal);
    assert_eq!(
        (terminal.screen().cursor_y(), terminal.screen().cursor_x()),
        (20, 6)
    );
    // Narrowing unused capacity must not move text or the input row.
    for cols in [79, 70, 64, 80] {
        resize_to_completion(&mut terminal, TerminalSize { rows: 24, cols }).unwrap();
        assert_eq!(
            (terminal.screen().cursor_y(), terminal.screen().cursor_x()),
            (20, 6),
            "short lines must stay put at {cols} columns"
        );
        for row in 0..20 {
            assert!(
                terminal
                    .row_text(row)
                    .starts_with(&format!("harbor-profile-{} ", row + 1)),
                "line {row} moved before reaching its text at {cols} columns"
            );
        }
    }
    for _ in 0..5 {
        for (rows, cols) in [(24, 30), (24, 12), (29, 50), (20, 8), (24, 80)] {
            resize_to_completion(&mut terminal, TerminalSize { rows, cols }).unwrap();
            let deadline = Instant::now() + std::time::Duration::from_millis(30);
            while Instant::now() < deadline {
                let _ = wake_rx.recv_timeout(std::time::Duration::from_millis(5));
                terminal.drain_pty();
            }
            let row = terminal.screen().cursor_y();
            terminal.io.write_pty(b"x").unwrap();
            let deadline = Instant::now() + std::time::Duration::from_secs(3);
            while !terminal.row_text(row).starts_with("READY:x") {
                assert!(
                    Instant::now() < deadline,
                    "echo at {cols} columns, row {row}: {:?}",
                    retained_text(&terminal)
                );
                let _ = wake_rx.recv_timeout(std::time::Duration::from_millis(10));
                terminal.drain_pty();
            }
            terminal.io.write_pty(b"\x08").unwrap();
            let deadline = Instant::now() + std::time::Duration::from_secs(3);
            while terminal.row_text(row).starts_with("READY:x") {
                assert!(Instant::now() < deadline, "backspace after resize");
                let _ = wake_rx.recv_timeout(std::time::Duration::from_millis(10));
                terminal.drain_pty();
            }
        }
    }
    // Let asynchronous ConPTY output settle; service queries throughout the wait.
    let deadline = Instant::now() + std::time::Duration::from_millis(750);
    while Instant::now() < deadline {
        let _ = wake_rx.recv_timeout(std::time::Duration::from_millis(20));
        terminal.drain_pty();
    }
    assert_eq!(retained_text(&terminal).trim_end(), before.trim_end());
    let prompt_row = terminal.screen().cursor_y();
    assert!(terminal.row_text(prompt_row).starts_with("READY:"));
    assert_eq!(terminal.screen().cursor_x(), 6);
    terminal.io.write_pty(b"x").unwrap();
    let deadline = Instant::now() + std::time::Duration::from_secs(3);
    while !terminal.row_text(prompt_row).starts_with("READY:x") {
        assert!(
            Instant::now() < deadline,
            "input must follow prompt: {:?}",
            retained_text(&terminal)
        );
        let _ = wake_rx.recv_timeout(std::time::Duration::from_millis(20));
        terminal.drain_pty();
    }
    assert_eq!(
        (terminal.screen().cursor_y(), terminal.screen().cursor_x()),
        (prompt_row, 7)
    );
    terminal.io.write_pty(b"\r").unwrap();
    let deadline = Instant::now() + std::time::Duration::from_secs(3);
    while !retained_text(&terminal).contains("ACK:") {
        assert!(
            Instant::now() < deadline,
            "output after resize must survive"
        );
        let _ = wake_rx.recv_timeout(std::time::Duration::from_millis(20));
        terminal.drain_pty();
    }
}

#[cfg(windows)]
#[test]
#[ignore = "pty: real Windows ConPTY"]
fn live_conpty_alt_resize_restores_primary_input_position() {
    let size = harbor_pty::TerminalSize { rows: 3, cols: 12 };
    struct Script(std::path::PathBuf);
    impl Drop for Script {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let script =
        Script(std::env::temp_dir().join(format!("harbor-alt-resize-{}.cmd", std::process::id())));
    std::fs::write(&script.0, b"@echo off\r\ncls\r\necho abcdefghijkl\r\necho mnopqrstuvwx\r\n<nul set /p =P:\r\n<nul set /p =\x1b[?1049h\r\nset /p answer=ALT:\r\n<nul set /p =\x1b[?1049l\r\nset /p answer=\r\n").unwrap();
    let command = harbor_pty::ShellCommand::new(
        Some(format!(
            r"{}\System32\cmd.exe",
            std::env::var("SystemRoot").unwrap()
        )),
        vec![
            "/d".into(),
            "/q".into(),
            "/c".into(),
            script.0.to_string_lossy().into_owned(),
        ],
    );
    let endpoints = harbor_pty::PtyEndpoints::spawn_shell(size, &command).unwrap();
    let (reader, writer, control) = endpoints.into_parts();
    let (wake_tx, wake_rx) = std::sync::mpsc::channel();
    let mut terminal = Terminal::new_headless(size.rows, size.cols);
    terminal.io = crate::io::TerminalIo::new(reader, writer, Some(control), move || {
        wake_tx.send(()).is_ok()
    });
    let deadline = Instant::now() + std::time::Duration::from_secs(10);
    while !terminal.is_alt_screen() || !terminal.row_text(0).starts_with("ALT:") {
        assert!(
            Instant::now() < deadline,
            "alternate fixture startup: {:?}",
            terminal.snapshot()
        );
        let _ = wake_rx.recv_timeout(std::time::Duration::from_millis(20));
        terminal.drain_pty();
    }
    for cols in [5, 2, 9, 12] {
        resize_to_completion(&mut terminal, TerminalSize { rows: 3, cols }).unwrap();
    }
    terminal.io.write_pty(b"\r").unwrap();
    let deadline = Instant::now() + std::time::Duration::from_secs(3);
    while terminal.is_alt_screen() {
        assert!(Instant::now() < deadline, "alternate fixture exit");
        let _ = wake_rx.recv_timeout(std::time::Duration::from_millis(20));
        terminal.drain_pty();
    }
    assert_eq!(terminal.row_text(0), "abcdefghijkl");
    assert_eq!(terminal.row_text(1), "mnopqrstuvwx");
    assert_eq!(
        (terminal.screen().cursor_y(), terminal.screen().cursor_x()),
        (2, 2)
    );
    terminal.io.write_pty(b"x").unwrap();
    let deadline = Instant::now() + std::time::Duration::from_secs(3);
    while !terminal.row_text(2).starts_with("P:x") {
        assert!(
            Instant::now() < deadline,
            "primary echo must follow its prompt"
        );
        let _ = wake_rx.recv_timeout(std::time::Duration::from_millis(20));
        terminal.drain_pty();
    }
    assert_eq!(
        (terminal.screen().cursor_y(), terminal.screen().cursor_x()),
        (2, 3)
    );
}

#[cfg(windows)]
#[test]
#[ignore = "pty: real Windows ConPTY"]
fn live_conpty_resize_barrier_resumes_output() {
    let size = harbor_pty::TerminalSize { rows: 4, cols: 40 };
    let endpoints =
        harbor_pty::PtyEndpoints::spawn_shell(size, &harbor_pty::ShellCommand::default())
            .expect("ConPTY shell should start");
    let (reader, writer, control) = endpoints.into_parts();
    let (wake_tx, wake_rx) = std::sync::mpsc::channel();
    let mut terminal = Terminal::new_headless(size.rows, size.cols);
    terminal.io = crate::io::TerminalIo::new(reader, writer, Some(control), move || {
        wake_tx.send(()).is_ok()
    });

    let started = Instant::now();
    resize_to_completion(&mut terminal, TerminalSize { rows: 5, cols: 32 }).unwrap();
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "ConPTY should acknowledge the resize barrier within its bound"
    );

    terminal
        .io
        .write_pty(b"echo HARBOR_BARRIER_RESUMED\r")
        .expect("resumed PTY should accept input");
    let deadline = Instant::now() + std::time::Duration::from_secs(3);
    loop {
        let now = Instant::now();
        assert!(now < deadline, "resumed reader should publish shell output");
        let _ = wake_rx.recv_timeout(
            deadline
                .saturating_duration_since(now)
                .min(std::time::Duration::from_millis(100)),
        );
        terminal.drain_pty();
        let screen = terminal.screen();
        let retained_rows = screen.scroll_count() + screen.rows();
        let text = screen.selected_text(crate::SelectionBounds {
            start_row: screen.history_start(),
            start_col: 0,
            end_row: screen.history_start() + retained_rows as u64 - 1,
            end_col: screen.cols() - 1,
        });
        if text.contains("HARBOR_BARRIER_RESUMED") {
            break;
        }
    }
}
