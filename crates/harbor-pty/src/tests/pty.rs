use super::*;

#[cfg(windows)]
#[test]
#[ignore = "pty: run the matching explicit test recipe"]
fn windows_endpoints_write_resize_and_shutdown_without_blocking_ui() {
    use std::time::{Duration, Instant};

    let (mut reader, mut writer, mut control) = PtyEndpoints::spawn_shell(
        TerminalSize { rows: 24, cols: 80 },
        &ShellCommand::default(),
    )
    .expect("Windows ConPTY endpoints should start")
    .into_parts();
    let reader_thread = std::thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        while reader.read(&mut buffer).is_ok_and(|length| length != 0) {}
    });

    writer
        .write_all(b"\r")
        .expect("endpoint writer should forward all bytes");
    control
        .resize(TerminalSize { rows: 25, cols: 81 })
        .expect("endpoint control should forward resize");
    drop(writer);

    let started = Instant::now();
    control.shutdown(reader_thread);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "endpoint shutdown must transfer cleanup rather than block the UI"
    );
}
