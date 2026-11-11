use harbor_terminal::Terminal;

fn emit(terminal: &mut Terminal, action: &str, fields: &[&str]) {
    let fields = fields
        .iter()
        .map(|field| {
            field
                .as_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join(";");
    let wire = format!("\x1b]777;harbor;1;{action};{fields}\x1b\\");
    // Real parser, split inside every UTF-8 hex pair/terminator.
    for byte in wire.as_bytes() {
        terminal.process_output(&[*byte]);
    }
    terminal.drain_output_events();
}
#[test]
fn project_metadata_retains_exited_agents_but_not_stale_cwd_or_other_terminal_records() {
    let mut one = Terminal::new_headless(3, 20);
    let mut two = Terminal::new_headless(3, 20);
    emit(
        &mut one,
        "agent",
        &[
            "ssh", "a", "/old", "pi", "", "a b", "q'\"\\", "名", "--resume",
        ],
    );
    let running = one.project_snapshot().unwrap();
    one.process_output(b"\x1b]777;harbor;1;busy\x07");
    one.drain_output_events();
    assert!(one.project_snapshot().is_none());
    emit(&mut one, "context", &["ssh", "a", "/new"]);
    let exited = one.project_snapshot().unwrap();
    assert_eq!(exited.agent, running.agent);
    assert_eq!(exited.directory, "/new");
    assert_eq!(running.directory, "/old", "prior snapshot is stable");
    emit(&mut two, "context", &["ssh", "a", "/other-tab"]);
    assert!(two.project_snapshot().unwrap().agent.is_none());
    emit(&mut one, "context", &["ssh", "b", "/host-b"]);
    assert!(one.project_snapshot().unwrap().agent.is_none());
    emit(
        &mut one,
        "agent",
        &["ssh", "a", "/new", "claude", "--resume"],
    );
    assert_eq!(
        one.project_snapshot().unwrap().agent.unwrap().program,
        "claude"
    );
    one.process_output(b"\x1b]777;harbor;1;context;broken\x07");
    one.drain_output_events();
    assert!(one.project_snapshot().is_none());
    one.process_output(b"\x1b]7;file://remote.invalid/project\x07");
    one.drain_output_events();
    assert!(one.project_snapshot().is_none());
    one.process_output(b"\x1bc");
    one.drain_output_events();
    emit(&mut one, "context", &["ssh", "a", "/reset"]);
    assert!(one.project_snapshot().unwrap().agent.is_none());
}
