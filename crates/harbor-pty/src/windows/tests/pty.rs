use super::*;

#[test]
#[ignore = "pty: run the matching explicit test recipe"]
fn unavailable_configured_shell_retries_default_shell() {
    let command = ShellCommand::new(
        Some("harbor-definitely-missing-shell.exe".to_owned()),
        vec!["--invalid-for-default-shell".to_owned()],
    );

    let (_pty, _reader, shell_name) = Pty::spawn_shell(PtySize { rows: 24, cols: 80 }, &command)
        .unwrap_or_else(|error| panic!("configured-shell fallback failed: {error:#}"));
    let (_default_pty, _default_reader, expected) =
        Pty::spawn_shell(PtySize { rows: 24, cols: 80 }, &ShellCommand::default()).unwrap();
    assert_eq!(shell_name, expected);
}

#[test]
#[ignore = "pty: run the matching explicit test recipe"]
fn shell_prompt_output_is_readable_through_pseudoconsole() {
    // Keep this transport check independent of the user's PowerShell prompt/profile.
    let command = ShellCommand::new(None, vec!["-NoLogo".to_owned(), "-NoProfile".to_owned()]);
    let (pty, mut reader, _shell_name) =
        Pty::spawn_shell(PtySize { rows: 24, cols: 80 }, &command).unwrap();
    let mut buffer = [0_u8; 4096];
    let mut output = Vec::new();
    let mut replied_to_attributes = false;

    let deadline = Instant::now() + Duration::from_secs(2);
    while !contains_shell_prompt(&output) && Instant::now() < deadline {
        if output_bytes_available(&reader) == 0 {
            thread::sleep(Duration::from_millis(10));
            continue;
        }

        let bytes = reader.read(&mut buffer).unwrap();
        if bytes == 0 {
            break;
        }
        output.extend_from_slice(&buffer[..bytes]);
        if !replied_to_attributes && output.windows(3).any(|bytes| bytes == b"\x1b[c") {
            // This transport-only test acts as a minimal VT220 terminal. The
            // bundled host waits for DA1; production replies through TerminalParser.
            let reply = b"\x1b[?62c";
            assert_eq!(
                pty._input_write.as_ref().unwrap().write(reply).unwrap(),
                reply.len()
            );
            replied_to_attributes = true;
        }
    }

    let text = String::from_utf8_lossy(&output);
    assert!(contains_shell_prompt(&output), "{text:?}");
}
