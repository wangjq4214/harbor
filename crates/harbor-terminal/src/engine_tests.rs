use crate::{
    Preedit, Terminal, TerminalEvent, TerminalFocusEvent, TerminalKey, TerminalKeyboardEvent,
    TerminalModifiers,
};
use std::{
    io::Write,
    sync::{Arc, Mutex},
    time::Instant,
};

#[test]
fn polling_and_updates_share_scheduling_without_consuming_redraw() {
    let mut terminal = Terminal::new_headless(3, 8);
    let now = Instant::now();
    for bytes in [
        &b""[..],
        &b"\x1b[2;2H"[..],
        &b"\x1b[?2026h"[..],
        &b"\x1b[?2026l"[..],
        &b"\x1b[2 q"[..],
        &b"\x1b[?25l"[..],
        &b"\x1b[?25h\x1b[1 q"[..],
    ] {
        terminal.put_bytes(bytes);
        let update = terminal.read_update(now);
        assert_eq!(terminal.frame_demand(now), update.frame_demand);
        assert_eq!(terminal.read_update(now).frame_demand, update.frame_demand);
        assert!(terminal.acknowledge_update(&update));
        assert!(!terminal.frame_demand(now).redraw_now);
    }

    terminal
        .handle_event(TerminalEvent::Preedit(Preedit::new("compose", None)))
        .unwrap();
    let update = terminal.read_update(now);
    assert!(update.frame_demand.redraw_now);
    assert_eq!(terminal.frame_demand(now), update.frame_demand);
    assert!(terminal.acknowledge_update(&update));
    terminal.clear_preedit();
    assert_eq!(
        terminal.frame_demand(now),
        terminal.read_update(now).frame_demand
    );
}

#[test]
fn overdue_autoscroll_remains_scheduled_while_ordinary_present_is_deferred() {
    use crate::{
        RenderViewport, TerminalPointerButton, TerminalPointerEvent, TerminalPointerPhase,
    };
    let mut terminal = Terminal::new_headless(4, 8);
    terminal.put_str(&"line\r\n".repeat(20));
    terminal.put_bytes(b"\x1b[2 q"); // Isolate the pointer deadline from cursor blink.
    terminal
        .pointer
        .set_viewport(RenderViewport::with_padding(10.0, 20.0, 0.0));
    let now = Instant::now();
    for (phase, y) in [
        (TerminalPointerPhase::Down, 25.0),
        (TerminalPointerPhase::Move, 1.0),
    ] {
        terminal.pointer.handle_pointer(
            &mut terminal.screen,
            TerminalPointerEvent::new((1.0, y), phase, TerminalPointerButton::Left, 7),
            now,
        );
    }
    let deadline = terminal
        .pointer
        .auto_scroll_deadline()
        .expect("active drag timer");
    assert!(deadline > now);
    assert!(terminal.acknowledge_update(&terminal.read_update(now)));
    terminal.put_bytes(b"\x1b[?2026h");
    for (at, redraw) in [(now, false), (deadline, true)] {
        let update = terminal.read_update(at);
        assert!(!update.frame_demand.ordinary_present_eligible);
        assert_eq!(update.frame_demand.deadline, Some(deadline));
        assert_eq!(update.frame_demand.redraw_now, redraw);
        assert_eq!(terminal.frame_demand(at), update.frame_demand);
    }
}

#[derive(Clone)]
struct RecordingWriter(Arc<Mutex<Vec<u8>>>);

impl Write for RecordingWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn escape_without_local_selection_reaches_primary_and_alternate_pty() {
    for alternate in [false, true] {
        let written = Arc::new(Mutex::new(Vec::new()));
        let mut terminal = Terminal::new_headless_with_io(
            3,
            8,
            std::io::empty(),
            RecordingWriter(Arc::clone(&written)),
            || true,
        );
        if alternate {
            terminal.put_bytes(b"\x1b[?1049h");
        }
        assert_eq!(terminal.is_alt_screen(), alternate);
        for _ in 0..2 {
            terminal
                .handle_event(TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown {
                    key: TerminalKey::Escape,
                    modifiers: TerminalModifiers::default(),
                }))
                .unwrap();
        }
        terminal
            .handle_event(TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyUp {
                key: TerminalKey::Escape,
                modifiers: TerminalModifiers::default(),
            }))
            .unwrap();
        assert_eq!(
            *written.lock().unwrap(),
            b"\x1b\x1b",
            "alternate={alternate}"
        );
    }
}

#[test]
fn escape_consumes_local_selection_or_drag_then_next_escape_reaches_pty() {
    use crate::{
        RenderViewport, TerminalPointerButton, TerminalPointerEvent, TerminalPointerPhase,
    };
    for (dragged, released) in [(false, false), (true, false), (true, true)] {
        let written = Arc::new(Mutex::new(Vec::new()));
        let mut terminal = Terminal::new_headless_with_io(
            3,
            8,
            std::io::empty(),
            RecordingWriter(Arc::clone(&written)),
            || true,
        );
        terminal.put_str("abcdefgh");
        terminal
            .pointer
            .set_viewport(RenderViewport::with_padding(10.0, 20.0, 0.0));
        let pointer = |phase, x| {
            TerminalEvent::Pointer(TerminalPointerEvent::new(
                (x, 1.0),
                phase,
                TerminalPointerButton::Left,
                7,
            ))
        };
        terminal
            .handle_event(pointer(TerminalPointerPhase::Down, 11.0))
            .unwrap();
        if dragged {
            terminal
                .handle_event(pointer(TerminalPointerPhase::Move, 41.0))
                .unwrap();
        }
        if released {
            terminal
                .handle_event(pointer(TerminalPointerPhase::Up, 41.0))
                .unwrap();
        }
        let escape = || {
            TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown {
                key: TerminalKey::Escape,
                modifiers: TerminalModifiers::default(),
            })
        };
        let canceled = terminal.handle_event_with_outcome(escape()).unwrap();
        assert!(canceled.redraw);
        assert_eq!(
            canceled.release_pointer,
            if released { None } else { Some(7) }
        );
        assert!(!terminal.pointer.has_selection_state());
        assert!(!terminal.pointer.has_active_pointer());
        assert!(
            written.lock().unwrap().is_empty(),
            "local cancellation must not reach child"
        );
        terminal.handle_event(escape()).unwrap();
        assert_eq!(
            *written.lock().unwrap(),
            b"\x1b",
            "next Esc must reach child"
        );
    }
}
#[test]
fn event_dispatch_preserves_escape_passthrough_ime_focus_and_keyup_policy() {
    let written = Arc::new(Mutex::new(Vec::new()));
    let mut terminal = Terminal::new_headless_with_io(
        3,
        8,
        std::io::empty(),
        RecordingWriter(Arc::clone(&written)),
        || true,
    );
    let now = Instant::now();
    let key = |key| {
        TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown {
            key,
            modifiers: TerminalModifiers::default(),
        })
    };

    terminal.handle_event(key(TerminalKey::Escape)).unwrap();
    terminal
        .handle_event(TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyUp {
            key: TerminalKey::Character('x'),
            modifiers: TerminalModifiers::default(),
        }))
        .unwrap();
    assert_eq!(*written.lock().unwrap(), b"\x1b");
    assert!(terminal.frame_demand(now).redraw_now);

    terminal
        .handle_event(key(TerminalKey::Character('x')))
        .unwrap();
    assert!(terminal.frame_demand(now).redraw_now);
    terminal
        .handle_event(TerminalEvent::Preedit(Preedit::new("compose", None)))
        .unwrap();
    let committed = terminal
        .handle_event_with_outcome(TerminalEvent::Keyboard(TerminalKeyboardEvent::Ime(
            "界".into(),
        )))
        .unwrap();
    assert!(committed.redraw);
    assert!(terminal.preedit().is_none());

    terminal.put_bytes(b"\x1b[?1004h");
    terminal
        .handle_event(TerminalEvent::Focus(TerminalFocusEvent::Gained))
        .unwrap();
    terminal
        .handle_event(TerminalEvent::Focus(TerminalFocusEvent::Gained))
        .unwrap();
    terminal
        .handle_event(TerminalEvent::Preedit(Preedit::new("pending", None)))
        .unwrap();
    let lost = terminal
        .handle_event_with_outcome(TerminalEvent::Focus(TerminalFocusEvent::Lost))
        .unwrap();
    assert!(lost.redraw);
    assert!(terminal.preedit().is_none());
    assert_eq!(*written.lock().unwrap(), "\x1bx界\x1b[I\x1b[O".as_bytes());
}
