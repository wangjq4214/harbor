use super::*;

#[test]
fn osc_titles_enforce_utf8_control_and_scalar_limits() {
    let mut terminal = Terminal::new_headless(1, 8);
    let title_256 = "界".repeat(256);
    let title_257 = "界".repeat(257);

    terminal.put_bytes(format!("\x1b]1;{title_256}\x07").as_bytes());
    terminal.put_bytes(format!("\x1b]1;{title_257}\x07").as_bytes());
    for control in [
        '\0', '\u{000b}', '\u{001b}', '\u{007f}', '\u{0080}', '\u{009f}',
    ] {
        terminal.put_bytes(format!("\x1b]2;bad{control}value\x07").as_bytes());
    }
    terminal.put_bytes(b"\x1b]2;bad\xffvalue\x07");
    terminal.put_bytes(b"\x1b]9;ignored\x07");

    assert_eq!(
        terminal.drain_output_events(),
        vec![TerminalOutputEvent::TitleChanged(title_256)]
    );
}

#[test]
fn osc7_emits_structured_metadata_in_fifo_order_and_drains_once() {
    let mut terminal = Terminal::new_headless(1, 8);

    terminal.put_bytes(b"\x1b]7;file:///home/Alice%20Smith\x07");
    terminal.put_bytes(b"\x1b]7;file://build");
    terminal.put_bytes(b"-host/C:/work\x1b");
    terminal.put_bytes(b"\\");

    assert_eq!(
        terminal.drain_output_events(),
        vec![
            TerminalOutputEvent::WorkingDirectoryChanged(WorkingDirectoryMetadata {
                host: None,
                path: "/home/Alice Smith".to_owned(),
            }),
            TerminalOutputEvent::WorkingDirectoryChanged(WorkingDirectoryMetadata {
                host: Some("build-host".to_owned()),
                path: "/C:/work".to_owned(),
            }),
        ]
    );
    assert!(terminal.drain_output_events().is_empty());
}

#[test]
fn osc7_reset_and_invalid_sequence_semantics_match_contract() {
    let mut terminal = Terminal::new_headless(1, 8);

    terminal.put_bytes(b"\x1b]7;file:///kept\x07");
    terminal.put_bytes(b"\x1b]7;file://user@host/rejected\x07");
    terminal.put_bytes(b"\x1b]7;file:///cancelled\x18");
    terminal.put_bytes(b"\x1b[!p");
    terminal.put_bytes(b"\x1b]7;\x07");
    terminal.put_bytes(b"\x1b]7\x1b\\");
    terminal.put_bytes(b"\x1bc");

    assert_eq!(
        terminal.drain_output_events(),
        vec![
            TerminalOutputEvent::WorkingDirectoryChanged(WorkingDirectoryMetadata {
                host: None,
                path: "/kept".to_owned(),
            }),
            TerminalOutputEvent::WorkingDirectoryReset,
            TerminalOutputEvent::WorkingDirectoryReset,
            TerminalOutputEvent::TitleReset,
            TerminalOutputEvent::WorkingDirectoryReset,
            TerminalOutputEvent::ShellIntegrationReset,
        ]
    );
}

#[test]
fn osc7_incomplete_and_overflow_sequences_emit_nothing_then_recover() {
    let mut terminal = Terminal::new_headless(1, 8);

    terminal.put_bytes(b"\x1b]7;file:///incomplete");
    assert!(terminal.drain_output_events().is_empty());
    terminal.put_bytes(b"\x18");

    let oversized = format!("\x1b]7;file:///{}\x07", "x".repeat(4096));
    terminal.put_bytes(oversized.as_bytes());
    assert!(terminal.drain_output_events().is_empty());

    terminal.put_bytes(b"\x1b]7;file:///recovered\x07");
    assert_eq!(
        terminal.drain_output_events(),
        vec![TerminalOutputEvent::WorkingDirectoryChanged(
            WorkingDirectoryMetadata {
                host: None,
                path: "/recovered".to_owned(),
            }
        )]
    );
}

#[test]
fn osc_empty_and_ris_reset_title_and_cwd_but_cancellation_and_decstr_do_not() {
    let mut terminal = Terminal::new_headless(1, 8);

    terminal.put_bytes(b"\x1b]2;kept\x18");
    terminal.put_bytes(b"\x1b]2;\x07");
    terminal.put_bytes(b"\x1b]2\x07");
    terminal.put_bytes(b"\x1b[!p");
    terminal.put_bytes(b"\x1bc");

    assert_eq!(
        terminal.drain_output_events(),
        vec![
            TerminalOutputEvent::TitleReset,
            TerminalOutputEvent::TitleReset,
            TerminalOutputEvent::TitleReset,
            TerminalOutputEvent::WorkingDirectoryReset,
            TerminalOutputEvent::ShellIntegrationReset,
        ]
    );
}

#[test]
fn osc133_emits_structured_metadata_in_fifo_order_and_drains_once() {
    let mut terminal = Terminal::new_headless(1, 8);

    terminal.put_bytes(b"\x1b]133;A\x07");
    terminal.put_bytes(b"\x1b]133;B\x1b\\");
    terminal.put_bytes(b"\x1b]133;C\x07");
    terminal.put_bytes(b"\x1b]133;D\x07");
    terminal.put_bytes(b"\x1b]133;D;0\x07");
    terminal.put_bytes(b"\x1b]133;D;130\x07");
    terminal.put_bytes(b"\x1b]133;D;-1\x07");
    terminal.put_bytes(b"\x1b]133;D;42\x07");

    // Split sequence across input fragments
    terminal.put_bytes(b"\x1b]133;");
    terminal.put_bytes(b"D;7");
    terminal.put_bytes(b"7\x07");

    assert_eq!(
        terminal.drain_output_events(),
        vec![
            TerminalOutputEvent::ShellIntegration(ShellIntegrationMarker::PromptStart),
            TerminalOutputEvent::ShellIntegration(ShellIntegrationMarker::PromptEnd),
            TerminalOutputEvent::ShellIntegration(ShellIntegrationMarker::CommandExecuted),
            TerminalOutputEvent::ShellIntegration(ShellIntegrationMarker::CommandFinished(None)),
            TerminalOutputEvent::ShellIntegration(ShellIntegrationMarker::CommandFinished(Some(0))),
            TerminalOutputEvent::ShellIntegration(ShellIntegrationMarker::CommandFinished(Some(
                130
            ))),
            TerminalOutputEvent::ShellIntegration(ShellIntegrationMarker::CommandFinished(Some(
                -1
            ))),
            TerminalOutputEvent::ShellIntegration(ShellIntegrationMarker::CommandFinished(Some(
                42
            ))),
            TerminalOutputEvent::ShellIntegration(ShellIntegrationMarker::CommandFinished(Some(
                77
            ))),
        ]
    );
    assert!(terminal.drain_output_events().is_empty());
}

#[test]
fn osc133_unknown_subcommands_and_malformed_payloads_are_safely_ignored() {
    let mut terminal = Terminal::new_headless(1, 8);

    // Unknown subcommands
    terminal.put_bytes(b"\x1b]133;E\x07");
    terminal.put_bytes(b"\x1b]133;P;foo=bar\x07");
    terminal.put_bytes(b"\x1b]133;?\x07");
    terminal.put_bytes(b"\x1b]133;Afoo\x07");
    terminal.put_bytes(b"\x1b]133;D_\x07");

    // Unsupported extra arguments or malformed exit codes return None (consume-ignore)
    terminal.put_bytes(b"\x1b]133;A;cl=m\x07");
    terminal.put_bytes(b"\x1b]133;D;not_numeric\x07");
    terminal.put_bytes(b"\x1b]133;D;\x07");
    terminal.put_bytes(b"\x1b]133;D;0;aid=foo\x07");

    // Plain text after invalid sequence is not corrupted
    terminal.put_bytes(b"ok");

    assert!(terminal.drain_output_events().is_empty());
    assert_eq!(terminal.screen().cell(0, 0).ch, 'o');
    assert_eq!(terminal.screen().cell(0, 1).ch, 'k');
}

#[test]
fn osc133_reset_and_cancellation_semantics() {
    let mut terminal = Terminal::new_headless(1, 8);

    terminal.put_bytes(b"\x1b]133;A\x07");
    terminal.put_bytes(b"\x1b]133;cancelled\x18");
    terminal.put_bytes(b"\x1b[!p");
    terminal.put_bytes(b"\x1b]133;\x07");
    terminal.put_bytes(b"\x1b]133\x1b\\");
    terminal.put_bytes(b"\x1bc");

    assert_eq!(
        terminal.drain_output_events(),
        vec![
            TerminalOutputEvent::ShellIntegration(ShellIntegrationMarker::PromptStart),
            TerminalOutputEvent::ShellIntegrationReset,
            TerminalOutputEvent::ShellIntegrationReset,
            TerminalOutputEvent::TitleReset,
            TerminalOutputEvent::WorkingDirectoryReset,
            TerminalOutputEvent::ShellIntegrationReset,
        ]
    );

    // Recovers cleanly after reset
    terminal.put_bytes(b"\x1b]133;C\x07");
    assert_eq!(
        terminal.drain_output_events(),
        vec![TerminalOutputEvent::ShellIntegration(
            ShellIntegrationMarker::CommandExecuted
        )]
    );
}

#[test]
fn osc133_incomplete_fragmented_terminators_and_oversized_recovery() {
    let mut terminal = Terminal::new_headless(1, 8);

    // Incomplete sequence across chunk boundaries with split ST (\x1b and \)
    terminal.put_bytes(b"\x1b]133;B");
    assert!(terminal.drain_output_events().is_empty());
    terminal.put_bytes(b"\x1b");
    assert!(terminal.drain_output_events().is_empty());
    terminal.put_bytes(b"\\");
    assert_eq!(
        terminal.drain_output_events(),
        vec![TerminalOutputEvent::ShellIntegration(
            ShellIntegrationMarker::PromptEnd
        )]
    );

    // Fragmented BEL
    terminal.put_bytes(b"\x1b]133;A");
    assert!(terminal.drain_output_events().is_empty());
    terminal.put_bytes(b"\x07");
    assert_eq!(
        terminal.drain_output_events(),
        vec![TerminalOutputEvent::ShellIntegration(
            ShellIntegrationMarker::PromptStart
        )]
    );

    // Oversized sequence (> 4096 bytes parser limit)
    let oversized = format!("\x1b]133;D;{}\x07", "9".repeat(4096));
    terminal.put_bytes(oversized.as_bytes());
    assert!(terminal.drain_output_events().is_empty());

    // Clean recovery after oversized payload
    terminal.put_bytes(b"\x1b]133;C\x07");
    assert_eq!(
        terminal.drain_output_events(),
        vec![TerminalOutputEvent::ShellIntegration(
            ShellIntegrationMarker::CommandExecuted
        )]
    );
}

#[test]
fn cargo_update_output_spans_multiple_rows() {
    // Replay the PTY output chunks logged during `cargo update`.
    let mut terminal = Terminal::new_headless(5, 80);

    // Chunk 0: "    Updating crates.io index\r\n"
    terminal.put_bytes(b"\x1b[92m\x1b[1m    Updating\x1b[m crates.io index\r\n");
    assert_eq!(
        terminal.row_text(0).trim_end(),
        "    Updating crates.io index"
    );
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (0, 1)
    );

    // Chunks 1-6: progress-bar updates that rewrite the same row via \r.
    terminal.put_bytes(b"\x1b[96m\x1b[1m       Fetch\x1b[m ");
    terminal.put_bytes(b"\x1b]9;4;3;0\x1b\\");
    terminal.put_bytes(b"[=====>                           ] 0 complete; 1 pending\x1b[144X\r");
    terminal.put_bytes(b"\x1b[96m\x1b[1m       Fetch\x1b[m ");
    terminal.put_bytes(b"\x1b]9;4;3;0\x1b\\");
    terminal.put_bytes(b"[=====>                           ] 1 complete; 0 pending\x1b[144X\r");
    // Confirm row 0 is untouched by progress bars.
    assert_eq!(
        terminal.row_text(0).trim_end(),
        "    Updating crates.io index"
    );

    // Chunk 7: "     Locking 0 packages ...\r\n"
    terminal.put_bytes(
            b"\x1b[92m\x1b[1m     Locking\x1b[m 0 packages to latest Rust 1.95.0 compatible versions\x1b[151X\r\n",
        );
    let row1 = terminal.row_text(1);
    assert!(
        row1.contains("Locking 0 packages to latest Rust 1.95.0 compatible versions"),
        "expected locking line on row 1, got: {row1:?}"
    );
    // CSI 151 X should have erased the stale progress-bar tail ("0 pending").
    assert!(
        !row1.contains("pending"),
        "ECH should have erased stale 'pending' text from progress bar, got: {row1:?}"
    );
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (0, 2)
    );

    // Chunk 8: "\r\nd:\workspaces\harbor>"
    // Chunk 8: "\r\nd:\workspaces\harbor>" — the leading \r\n advances
    // to the next row, so the prompt lands on row 3, not row 2.
    terminal.put_bytes(b"\r\nd:\\workspaces\\harbor>");
    let row3 = terminal.row_text(3);
    assert!(
        row3.contains("d:\\workspaces\\harbor>"),
        "expected prompt on row 3, got: {row3:?}"
    );
}
