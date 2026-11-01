//! Live Windows/ConPTY coverage, deliberately separate from parser-only fixtures.
//! Approved live-primary policy: style-only row tails may be clipped on resize,
//! but must not create extra physical rows or displace the shell's input cursor.
#![cfg(windows)]

#[path = "conpty_styled_tail_resize/pty.rs"]
mod pty;

use harbor_pty::{PtyEndpoints, ShellCommand};
use harbor_terminal::{Cell, CellAttrs, Color, SelectionBounds, Terminal, TerminalSize};
use std::io::Write;
use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

const END: &str =
    "END T0002 - ordinary shell prompt/input below; conceal does not redact copied text.";
const PAYLOAD: &str = "abcdefghijklmnopqrstuvwxyzabcdefghi"; // 35 + "echo " = 40.
const QUIET: Duration = Duration::from_millis(80);

struct LiveCmd {
    terminal: Terminal,
    wake: Receiver<()>,
    prompt: String,
}

fn windows_path(path: &Path) -> String {
    path.to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned()
}

fn retained(terminal: &Terminal) -> String {
    let s = terminal.screen();
    s.selected_text(SelectionBounds {
        start_row: s.history_start(),
        start_col: 0,
        end_row: s.history_start() + (s.scroll_count() + s.rows()) as u64 - 1,
        end_col: s.cols() - 1,
    })
}

impl LiveCmd {
    fn new(rows: usize) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("repository root");
        let root = windows_path(&root);
        let command = ShellCommand::new(
            Some(format!(
                r"{}\System32\cmd.exe",
                std::env::var("SystemRoot").expect("Windows SystemRoot")
            )),
            vec!["/d".into(), "/q".into()],
        );
        let endpoints =
            PtyEndpoints::spawn_shell(harbor_pty::TerminalSize { rows, cols: 103 }, &command)
                .expect("live bundled ConPTY must start (not an ignored infrastructure failure)");
        let (tx, wake) = mpsc::channel();
        let terminal = Terminal::new_headless(rows, 103)
            .start_session_from_endpoints(endpoints, move || tx.send(()).is_ok())
            .expect("attach real PTY endpoints without a GPU");
        let mut live = Self {
            terminal,
            wake,
            prompt: format!("{root}>"),
        };
        live.wait_for("initial cmd prompt", |t| {
            retained(t).trim_end().ends_with('>')
        });
        live.send(&format!("cd /d \"{root}\"\rprompt $P$G\r"));
        let prompt = live.prompt.clone();
        live.wait_for("repository cmd prompt", |t| {
            retained(t).trim_end().ends_with(&prompt)
        });
        live.quiesce("cmd initialization");
        live
    }

    fn send(&mut self, text: &str) {
        self.terminal
            .write_pty(text.as_bytes())
            .expect("write live PTY input");
    }

    // Every wait has an observable predicate and a deadline; wakeups, not long
    // sleeps, drive draining. A missing marker or closed shell is always a failure.
    fn wait_for(&mut self, label: &str, predicate: impl Fn(&Terminal) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            self.terminal.drain_pty();
            if predicate(&self.terminal) {
                return;
            }
            assert!(
                !self.terminal.is_session_closed() && Instant::now() < deadline,
                "{label}: cursor=({}, {}), retained={:?}",
                self.terminal.screen().cursor_y(),
                self.terminal.screen().cursor_x(),
                retained(&self.terminal)
            );
            let _ = self.wake.recv_timeout(Duration::from_millis(20));
        }
    }

    // Resize can repaint an already matching prompt. Observe a short quiet
    // window before asserting; this is bounded, not a fixed success delay.
    fn quiesce(&mut self, label: &str) {
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut last_output = Instant::now();
        loop {
            if self.terminal.drain_pty() {
                last_output = Instant::now();
            }
            assert!(!self.terminal.is_session_closed(), "{label}: shell closed");
            if last_output.elapsed() >= QUIET {
                return;
            }
            assert!(Instant::now() < deadline, "{label}: output never quiesced");
            let _ = self.wake.recv_timeout(Duration::from_millis(10));
        }
    }

    fn run_powershell(&mut self, command: &str, end: &str) {
        self.send(command);
        let suffix = format!("{end}\n\n{}", self.prompt);
        let prompt = self.prompt.clone();
        self.wait_for("PowerShell END and returned cmd prompt", |t| {
            let text = retained(t);
            // Only the producer's complete output line completes the handshake.
            text.lines().any(|line| line.trim_end() == end) && text.trim_end().ends_with(&prompt)
        });
        self.quiesce("PowerShell exit");
        let text = retained(&self.terminal);
        assert!(
            text.lines().any(|line| line.trim_end() == end),
            "END must be complete, not just its marker: {text:?}"
        );
        assert!(
            text.trim_end().ends_with(&self.prompt),
            "returned prompt: {suffix:?}, actual={text:?}"
        );
    }

    fn run_powershell_source(&mut self, source: &str, end: &str) {
        let mut script = tempfile::Builder::new()
            .prefix("harbor-styled-tail-")
            .suffix(".ps1")
            .tempfile()
            .expect("create PowerShell producer fixture");
        script
            .write_all(source.as_bytes())
            .expect("write PowerShell producer fixture");
        let script = script.into_temp_path();
        self.run_powershell(
            &format!(
                "powershell.exe -NoProfile -ExecutionPolicy Bypass -File \"{}\"\r",
                script.display()
            ),
            end,
        );
    }

    fn assert_input(&self, input: &str, label: &str) {
        let expected = format!("{}{input}", self.prompt);
        let text = retained(&self.terminal);
        assert_eq!(
            text.trim_end().lines().last(),
            Some(expected.as_str()),
            "{label}: selected_text must join soft wraps and retain the COMPLETE prompt/input; text={text:?}"
        );
        // Text alone can pass with a displaced cursor. Select through the cell
        // immediately before the live cursor to verify the insertion boundary.
        let s = self.terminal.screen();
        let cursor_generation = s.history_start() + s.scroll_count() as u64 + s.cursor_y() as u64;
        let (end_row, end_col) = if s.cursor_x() == 0 {
            (
                cursor_generation
                    .checked_sub(1)
                    .expect("input precedes cursor"),
                s.cols() - 1,
            )
        } else {
            (cursor_generation, s.cursor_x() - 1)
        };
        assert!(
            end_row >= s.history_start(),
            "input boundary must remain retained"
        );
        let prefix = s.selected_text(SelectionBounds {
            start_row: s.history_start(),
            start_col: 0,
            end_row,
            end_col,
        });
        assert!(
            prefix.ends_with(&expected),
            "{label}: cursor must follow full input, prefix={prefix:?}"
        );
    }

    fn pending_input(&mut self) -> String {
        let input = format!("echo {PAYLOAD}");
        assert_eq!(input.len(), 40);
        self.send(&input);
        let expected = format!("{}{input}", self.prompt);
        self.wait_for("40-character pending command", |t| {
            retained(t).trim_end().ends_with(&expected)
        });
        self.quiesce("pending command echo");
        self.assert_input(&input, "before resize");
        input
    }

    fn resize(&mut self, rows: usize, cols: usize) {
        self.terminal
            .try_resize_if_changed(TerminalSize { rows, cols })
            .expect("nonblocking live resize request");
        self.wait_for("asynchronous resize commit", |terminal| {
            terminal.screen().rows() == rows && terminal.screen().cols() == cols
        });
        self.quiesce("ConPTY resize repaint");
    }

    fn edit_and_execute(&mut self, input: &str) {
        self.send("x");
        let expected = format!("{}{input}x", self.prompt);
        self.wait_for("x echoed after pending command", |t| {
            retained(t).trim_end().ends_with(&expected)
        });
        self.assert_input(&format!("{input}x"), "x insertion after resize");
        self.send("\x7f"); // ConPTY VT DEL maps to the cooked-input Backspace key.
        let expected = format!("{}{input}", self.prompt);
        self.wait_for("backspace removes only x", |t| {
            retained(t).trim_end().ends_with(&expected)
        });
        self.quiesce("backspace echo");
        self.assert_input(input, "backspace insertion boundary");
        self.send("\r");
        // A standalone response line proves Enter executed the entire command;
        // matching payload inside the echoed input is deliberately insufficient.
        let response = format!("\n{}\n", input.strip_prefix("echo ").expect("echo fixture"));
        let prompt = self.prompt.clone();
        self.wait_for("echo response and next complete prompt", |t| {
            let text = retained(t);
            text.contains(&response) && text.trim_end().ends_with(&prompt)
        });
        self.quiesce("echo command completion");
        self.assert_input("", "next cmd prompt");
    }
}

fn generation_with_prefix(t: &Terminal, prefix: &str) -> u64 {
    let s = t.screen();
    (s.history_start()..s.history_start() + (s.scroll_count() + s.rows()) as u64)
        .find(|&generation| {
            s.selected_text(SelectionBounds {
                start_row: generation,
                start_col: 0,
                end_row: generation,
                end_col: s.cols() - 1,
            })
            .starts_with(prefix)
        })
        .unwrap_or_else(|| panic!("missing physical row {prefix:?}: {:?}", retained(t)))
}

fn assert_fixture_rows(t: &Terminal, printed: &Cell, erased: &Cell, tail_limit: usize) {
    let print = generation_with_prefix(t, "PRINT:A    界    B");
    let erase = generation_with_prefix(t, "ERASE:AB");
    let end = generation_with_prefix(t, "END_MINIMAL");
    assert_eq!(
        erase,
        print + 1,
        "printed styled tail must not add physical rows"
    );
    assert_eq!(end, erase + 1, "styled erase must not add physical rows");
    let s = t.screen();
    for col in 18..tail_limit.min(s.cols()) {
        assert_eq!(
            s.cell_at_generation(print, col),
            Some(printed),
            "printed in-grid style at col={col}"
        );
    }
    for col in 8..tail_limit.min(s.cols()) {
        assert_eq!(
            s.cell_at_generation(erase, col),
            Some(erased),
            "erased in-grid style at col={col}"
        );
    }
}

fn minimal_fixture(sgr: &str, rows: usize) {
    let mut live = LiveCmd::new(rows);
    // Keep source ASCII for PowerShell 5.1's legacy file decoding; the
    // producer constructs the wide glyph as a scalar at runtime.
    let history = if rows == 6 {
        "1..30 | ForEach-Object { [Console]::Out.WriteLine(('HISTORY_' + $_)) }; "
    } else {
        ""
    };
    let source = format!(
        "[Console]::OutputEncoding=[System.Text.UTF8Encoding]::new($false); $e=[char]27; [Console]::Out.Write($e+'[0m'+$e+'[2J'+$e+'[H'); {history}[Console]::Out.WriteLine(('PRINT:A    '+[char]0x754C+'    B'+$e+'[{sgr}m'+(' '*65)+$e+'[0m')); [Console]::Out.WriteLine(('ERASE:AB'+$e+'[{sgr}m'+$e+'[K'+$e+'[0m')); [Console]::Out.WriteLine('END_MINIMAL');"
    );
    live.run_powershell_source(&source, "END_MINIMAL");
    let print = generation_with_prefix(&live.terminal, "PRINT:A    界    B");
    let erase = generation_with_prefix(&live.terminal, "ERASE:AB");
    let printed = live
        .terminal
        .screen()
        .cell_at_generation(print, 18)
        .unwrap()
        .clone();
    let erased = live
        .terminal
        .screen()
        .cell_at_generation(erase, 8)
        .unwrap()
        .clone();
    for cell in [&printed, &erased] {
        assert_eq!(cell.ch, ' ');
        assert!(
            cell.attrs
                .contains(CellAttrs::OVERLINE | CellAttrs::UNDERLINE)
                || cell.bg != Color::Default,
            "fixture must arrive through ConPTY with visible blank style: {cell:?}"
        );
    }
    assert_fixture_rows(&live.terminal, &printed, &erased, 83);
    if rows == 6 {
        assert!(
            live.terminal.screen().scroll_count() > 0,
            "bottom/history fixture must really have scrollback"
        );
    }
    let input = live.pending_input();
    let sizes = if rows == 6 {
        [(5, 51), (8, 103), (6, 51), (6, 103)]
    } else {
        [(rows, 51), (rows, 103), (rows, 51), (rows, 103)]
    };
    for (rows, cols) in sizes {
        live.resize(rows, cols);
        // Overflow styles need not return on widening. Only surviving in-grid
        // cells are asserted, while glyphs and internal spaces remain exact.
        assert_fixture_rows(&live.terminal, &printed, &erased, 51);
        live.assert_input(
            &input,
            &format!("minimal sgr={sgr}, rows={rows}, cols={cols}"),
        );
    }
    live.edit_and_execute(&input);
}

// These producer profiles deliberately begin with wrapForced rows at the OLD
// width. They must agree with the approved tail-clipping geometry, not merely
// preserve a payload somewhere in history after a producer-relative repaint.
fn producer_edge_fixture(body: &str, expression: &str, spaces: usize, sgr: &str) {
    let mut live = LiveCmd::new(29);
    live.run_powershell_source(
        &format!(
            "[Console]::OutputEncoding=[System.Text.UTF8Encoding]::new($false); $e=[char]27; [Console]::Out.Write($e+'[0m'+$e+'[2J'+$e+'[H'); [Console]::Out.WriteLine(({expression})+$e+'[{sgr}m'+(' '*{spaces})+$e+'[0m'); [Console]::Out.WriteLine('END_EDGE');"
        ),
        "END_EDGE",
    );
    let start = generation_with_prefix(
        &live.terminal,
        if body.starts_with('Z') { "ZZZZ" } else { body },
    );
    let end = generation_with_prefix(&live.terminal, "END_EDGE");
    assert!(
        end >= start + 2,
        "fixture must really wrap across old 103-column boundary"
    );
    assert!(
        live.terminal.screen().is_wrapped_at_generation(start + 1),
        "producer must mark the initial physical row soft-wrapped"
    );
    let input = live.pending_input();
    for (round, cols) in [51, 103, 51, 103].into_iter().enumerate() {
        live.resize(29, cols);
        let prefix = if body.starts_with('Z') { "ZZZZ" } else { body };
        let start = generation_with_prefix(&live.terminal, prefix);
        let end = generation_with_prefix(&live.terminal, "END_EDGE");
        let width = if body.ends_with('界') {
            body.len() - '界'.len_utf8() + 2
        } else {
            body.len()
        };
        // The pinned producer preserves the already wrapForced 103-column
        // prefix on the FIRST shrink. Only its last old source row is trimmed.
        // After widening that prefix fits one row and subsequent shrinks can
        // trim the final style-only tail down to the actual body extent.
        let expected_rows = if round == 0 {
            103_usize.div_ceil(cols)
        } else {
            width.div_ceil(cols)
        } as u64;
        eprintln!(
            "producer edge: body_width={width}, tail={spaces}, sgr={sgr}, cols={cols}, physical_rows={}, cursor=({}, {})",
            end - start,
            live.terminal.screen().cursor_y(),
            live.terminal.screen().cursor_x()
        );
        assert_eq!(
            end - start,
            expected_rows,
            "wrapForced style-only overflow must not allocate physical rows; text={:?}",
            retained(&live.terminal)
        );
        assert!(
            retained(&live.terminal).contains(body),
            "exact right-edge glyphs/internal content must survive"
        );
        live.assert_input(&input, &format!("producer edge before input cols={cols}"));
        // Force real producer output at EACH width; a model-only resize result
        // can look correct until ConPTY redraws relative to its own row count.
        live.send("x");
        let expected = format!("{}{input}x", live.prompt);
        live.wait_for("edge-profile x echo", |t| {
            retained(t).trim_end().ends_with(&expected)
        });
        live.assert_input(&format!("{input}x"), "edge-profile x insertion");
        live.send("\x7f");
        let expected = format!("{}{input}", live.prompt);
        live.wait_for("edge-profile Backspace", |t| {
            retained(t).trim_end().ends_with(&expected)
        });
        live.quiesce("edge-profile edit completion");
        live.assert_input(&input, "edge-profile Backspace position");
    }
    live.edit_and_execute(&input);
}
