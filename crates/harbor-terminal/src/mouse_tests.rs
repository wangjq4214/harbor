//! Byte-exact mouse protocol tests through the encoder and recording PTY path.
use crate::*;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

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
fn terminal(rows: usize, cols: usize) -> (Terminal, Arc<Mutex<Vec<u8>>>) {
    let written = Arc::new(Mutex::new(Vec::new()));
    let terminal = Terminal::new_headless_with_io(
        rows,
        cols,
        std::io::empty(),
        RecordingWriter(written.clone()),
        || true,
    );
    (terminal, written)
}
fn take(written: &Arc<Mutex<Vec<u8>>>) -> Vec<u8> {
    std::mem::take(&mut *written.lock().unwrap())
}
fn event(
    position: (f32, f32),
    phase: TerminalPointerPhase,
    button: TerminalPointerButton,
) -> TerminalEvent {
    TerminalEvent::Pointer(TerminalPointerEvent::new(position, phase, button, 7))
}
fn modes(encoding: MouseEncoding) -> InputModes {
    InputModes {
        mouse_tracking: MouseTrackingMode::AnyMotion,
        mouse_utf8: encoding == MouseEncoding::Utf8,
        mouse_urxvt: encoding == MouseEncoding::Urxvt,
        mouse_sgr: encoding == MouseEncoding::Sgr,
        mouse_sgr_pixel: encoding == MouseEncoding::SgrPixel,
        ..InputModes::default()
    }
}
const ENCODINGS: [MouseEncoding; 5] = [
    MouseEncoding::Legacy,
    MouseEncoding::Utf8,
    MouseEncoding::Urxvt,
    MouseEncoding::Sgr,
    MouseEncoding::SgrPixel,
];
const MODE_NUMBERS: [usize; 5] = [0, 1005, 1015, 1006, 1016];

#[test]
fn mouse_all_encodings_press_release_reach_pty() {
    let fixtures: [(&[u8], &[u8]); 5] = [
        (b"\x1b[M\x20\x23\x22", b"\x1b[M\x23\x23\x22"),
        (b"\x1b[M\x20\x23\x22", b"\x1b[M\x23\x23\x22"),
        (b"\x1b[32;3;2M", b"\x1b[35;3;2M"),
        (b"\x1b[<0;3;2M", b"\x1b[<0;3;2m"),
        (b"\x1b[<0;26;22M", b"\x1b[<0;26;22m"),
    ];
    for (mode, (down_bytes, up_bytes)) in MODE_NUMBERS.into_iter().zip(fixtures) {
        let (mut t, written) = terminal(10, 20);
        t.pointer
            .set_viewport(RenderViewport::with_padding(10.0, 20.0, 2.0));
        t.put_bytes(format!("\x1b[?1000;{mode}h").as_bytes());
        let down = t
            .handle_event_with_outcome(event(
                (27.0, 23.0),
                TerminalPointerPhase::Down,
                TerminalPointerButton::Left,
            ))
            .unwrap();
        assert_eq!(down.capture_pointer, Some(7));
        assert!(!t.pointer.has_selection_state());
        assert_eq!(take(&written), down_bytes, "mode {mode}");
        let up = t
            .handle_event_with_outcome(event(
                (27.0, 23.0),
                TerminalPointerPhase::Up,
                TerminalPointerButton::Left,
            ))
            .unwrap();
        assert_eq!(up.release_pointer, Some(7));
        assert_eq!(take(&written), up_bytes, "mode {mode}");
    }
}

#[test]
fn mouse_encoding_numeric_boundaries_are_exact_and_safe() {
    for (encoding, x, expected) in [
        (MouseEncoding::Legacy, 223.0, &b"\x1b[M\x20\xff\x22"[..]),
        (MouseEncoding::Utf8, 95.0, &b"\x1b[M\x20\x7f\x22"[..]),
        (MouseEncoding::Utf8, 96.0, &b"\x1b[M\x20\xc2\x80\x22"[..]),
        (MouseEncoding::Utf8, 128.0, &b"\x1b[M\x20\xc2\xa0\x22"[..]),
        (MouseEncoding::Utf8, 2015.0, &b"\x1b[M\x20\xdf\xbf\x22"[..]),
        (MouseEncoding::Urxvt, 224.0, &b"\x1b[32;224;2M"[..]),
        (MouseEncoding::Sgr, 224.0, &b"\x1b[<0;224;2M"[..]),
        (MouseEncoding::SgrPixel, 224.0, &b"\x1b[<0;224;2M"[..]),
        (
            MouseEncoding::Sgr,
            4294967040.0,
            &b"\x1b[<0;4294967041;2M"[..],
        ),
    ] {
        let encoded = input::encode(
            &event(
                (x - 1.0, 1.0),
                TerminalPointerPhase::Down,
                TerminalPointerButton::Left,
            ),
            modes(encoding),
        );
        assert_eq!(encoded.as_deref(), Some(expected), "{encoding:?}, x={x}");
        // Exercise the same limit on y independently.
        if matches!(encoding, MouseEncoding::Legacy | MouseEncoding::Utf8) {
            let encoded = input::encode(
                &event(
                    (1.0, x - 1.0),
                    TerminalPointerPhase::Down,
                    TerminalPointerButton::Left,
                ),
                modes(encoding),
            )
            .unwrap();
            let mut y_expected = b"\x1b[M\x20\x22".to_vec();
            y_expected.extend_from_slice(&expected[4..expected.len() - 1]);
            assert_eq!(encoded, y_expected);
        }
    }
    for (encoding, overflow) in [
        (MouseEncoding::Legacy, 223.0),
        (MouseEncoding::Utf8, 2015.0),
    ] {
        for position in [(overflow, 0.0), (0.0, overflow)] {
            assert_eq!(
                input::encode(
                    &event(
                        position,
                        TerminalPointerPhase::Down,
                        TerminalPointerButton::Left
                    ),
                    modes(encoding)
                ),
                None
            );
        }
    }
    for encoding in ENCODINGS {
        for extreme in [
            f32::MAX,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
            u32::MAX as f32,
        ] {
            assert_eq!(
                input::encode(
                    &event(
                        (extreme, 0.0),
                        TerminalPointerPhase::Down,
                        TerminalPointerButton::Left
                    ),
                    modes(encoding)
                ),
                None
            );
        }
    }
}

#[test]
fn mouse_all_buttons_modifiers_and_event_codes() {
    for encoding in ENCODINGS {
        for (button, identity) in [
            (TerminalPointerButton::Left, 0u8),
            (TerminalPointerButton::Middle, 1),
            (TerminalPointerButton::Right, 2),
        ] {
            for bits in 0..16 {
                let modifiers = TerminalModifiers {
                    shift: bits & 1 != 0,
                    alt: bits & 2 != 0,
                    meta: bits & 4 != 0,
                    ctrl: bits & 8 != 0,
                };
                let modifier_code = (if modifiers.shift { 4 } else { 0 })
                    | (if modifiers.alt || modifiers.meta {
                        8
                    } else {
                        0
                    })
                    | (if modifiers.ctrl { 16 } else { 0 });
                for (phase, code, release) in [
                    (TerminalPointerPhase::Down, identity, false),
                    (
                        TerminalPointerPhase::Up,
                        if matches!(encoding, MouseEncoding::Sgr | MouseEncoding::SgrPixel) {
                            identity
                        } else {
                            3
                        },
                        true,
                    ),
                    (TerminalPointerPhase::Move, identity + 32, false),
                    (
                        TerminalPointerPhase::WheelLine { dx: 0.0, dy: 1.0 },
                        64,
                        false,
                    ),
                    (
                        TerminalPointerPhase::WheelPixel { dx: 0.0, dy: -2.5 },
                        65,
                        false,
                    ),
                ] {
                    let pointer = TerminalPointerEvent::new((2.0, 1.0), phase, button, 7)
                        .with_modifiers(modifiers);
                    let code = code | modifier_code;
                    let expected = match encoding {
                        MouseEncoding::Legacy => vec![0x1b, b'[', b'M', code + 32, 35, 34],
                        MouseEncoding::Utf8 => {
                            format!("\x1b[M{}#\"", char::from(code + 32)).into_bytes()
                        }
                        MouseEncoding::Urxvt => format!("\x1b[{};3;2M", code + 32).into_bytes(),
                        MouseEncoding::Sgr | MouseEncoding::SgrPixel => {
                            format!("\x1b[<{code};3;2{}", if release { 'm' } else { 'M' })
                                .into_bytes()
                        }
                    };
                    assert_eq!(
                        input::encode(&TerminalEvent::Pointer(pointer), modes(encoding)),
                        Some(expected),
                        "{encoding:?}, {phase:?}, bits={bits}"
                    );
                }
            }
        }
    }
}

#[test]
fn mouse_pixel_geometry_precision_switching_and_safe_boundaries() {
    let (mut t, written) = terminal(10, 20);
    t.pointer
        .set_viewport(RenderViewport::with_padding(10.0, 20.0, 2.0));
    t.put_bytes(b"\x1b[?1000;1016h");
    for command in [&b""[..], &b"\x1b[?1006h"[..], &b"\x1b[?1016l"[..]] {
        t.put_bytes(command);
        t.handle_event(event(
            (27.0, 23.0),
            TerminalPointerPhase::Down,
            TerminalPointerButton::Left,
        ))
        .unwrap();
    }
    assert_eq!(take(&written), b"\x1b[<0;26;22M\x1b[<0;26;22M\x1b[<0;3;2M");
    let mut viewport = RenderViewport::with_padding(10.0, 20.0, 4.0);
    viewport.allocation_origin = (100.0, 50.0);
    t.pointer.set_viewport(viewport);
    t.pointer.set_input_scale(2.0);
    t.put_bytes(b"\x1b[?1016h");
    for position in [
        (52.0, 27.0),
        (64.0, 39.0),
        (64.5, 39.0),
        (64.99, 39.25),
        (-100.0, -100.0),
        (500.0, 500.0),
    ] {
        t.handle_event(event(
            position,
            TerminalPointerPhase::Down,
            TerminalPointerButton::Left,
        ))
        .unwrap();
    }
    assert_eq!(
        take(&written),
        b"\x1b[<0;1;1M\x1b[<0;25;25M\x1b[<0;26;25M\x1b[<0;26;25M\x1b[<0;1;1M\x1b[<0;897;947M"
    );
    for extreme in [f32::MAX, f32::NAN, f32::INFINITY] {
        let outcome = t
            .handle_event_with_outcome(event(
                (extreme, 39.0),
                TerminalPointerPhase::Down,
                TerminalPointerButton::Left,
            ))
            .unwrap();
        assert_eq!(outcome.capture_pointer, Some(7));
        assert!(!t.pointer.has_selection_state());
    }
    assert!(take(&written).is_empty());
    viewport.allocation_origin = (0.0, 0.0);
    t.pointer.set_viewport(viewport);
    t.pointer.set_input_scale(1.0);
    t.handle_event(event(
        (4.75, 4.25),
        TerminalPointerPhase::Down,
        TerminalPointerButton::Left,
    ))
    .unwrap();
    assert_eq!(take(&written), b"\x1b[<0;1;1M");
    t.put_bytes(b"\x1b[?1016l");
    t.handle_event(event(
        (f32::MAX, -100.0),
        TerminalPointerPhase::Down,
        TerminalPointerButton::Left,
    ))
    .unwrap();
    assert_eq!(take(&written), b"\x1b[<0;20;1M"); // Existing cell viewport clamp.
}

#[test]
fn mouse_wheels_both_units_signs_order_modifiers_and_multiplicity() {
    for mode in [1006, 1016] {
        let (mut t, written) = terminal(10, 20);
        t.pointer
            .set_viewport(RenderViewport::with_padding(10.0, 20.0, 2.0));
        t.put_bytes(format!("\x1b[?1000;{mode}h").as_bytes());
        let coordinates = if mode == 1006 { "3;2" } else { "26;22" };
        for pixel in [false, true] {
            for (dx, dy, codes) in [
                (1.0, 0.0, &[66][..]),
                (-1.0, 0.0, &[67][..]),
                (2.5, -9.5, &[65, 66][..]),
                (-0.25, 3.0, &[64, 67][..]),
                (0.0, 0.0, &[][..]),
            ] {
                let phase = if pixel {
                    TerminalPointerPhase::WheelPixel { dx, dy }
                } else {
                    TerminalPointerPhase::WheelLine { dx, dy }
                };
                let pointer =
                    TerminalPointerEvent::new((27.0, 23.0), phase, TerminalPointerButton::None, 7)
                        .with_modifiers(TerminalModifiers {
                            shift: true,
                            alt: true,
                            meta: true,
                            ctrl: true,
                        });
                t.handle_event(TerminalEvent::Pointer(pointer)).unwrap();
                let expected = codes
                    .iter()
                    .map(|code| format!("\x1b[<{};{coordinates}M", code | 28))
                    .collect::<String>();
                assert_eq!(take(&written), expected.as_bytes());
            }
        }
    }
}

#[test]
fn mouse_tracking_fallback_cancel_and_encoding_overflow_preserve_vt_ownership() {
    for mode in MODE_NUMBERS {
        let (mut t, written) = terminal(5, 230);
        t.pointer
            .set_viewport(RenderViewport::with_padding(1.0, 1.0, 0.0));
        t.put_bytes(format!("\x1b[?1000;1002;1003;{mode}h").as_bytes());
        let encoding = t.screen.input_modes().mouse_encoding();
        let assert_report = |t: &mut Terminal, phase, button, code: u8, release| {
            t.handle_event(event((2.0, 1.0), phase, button)).unwrap();
            let expected = match encoding {
                MouseEncoding::Legacy | MouseEncoding::Utf8 => {
                    vec![0x1b, b'[', b'M', code + 32, 35, 34]
                }
                MouseEncoding::Urxvt => format!("\x1b[{};3;2M", code + 32).into_bytes(),
                _ => format!("\x1b[<{code};3;2{}", if release { 'm' } else { 'M' }).into_bytes(),
            };
            assert_eq!(take(&written), expected);
        };
        assert_report(
            &mut t,
            TerminalPointerPhase::Move,
            TerminalPointerButton::None,
            35,
            false,
        );
        t.put_bytes(b"\x1b[?1003l");
        t.handle_event(event(
            (2.0, 1.0),
            TerminalPointerPhase::Move,
            TerminalPointerButton::None,
        ))
        .unwrap();
        assert!(take(&written).is_empty());
        assert_report(
            &mut t,
            TerminalPointerPhase::Down,
            TerminalPointerButton::Middle,
            1,
            false,
        );
        assert_report(
            &mut t,
            TerminalPointerPhase::Move,
            TerminalPointerButton::None,
            33,
            false,
        );
        let sgr = matches!(encoding, MouseEncoding::Sgr | MouseEncoding::SgrPixel);
        assert_report(
            &mut t,
            TerminalPointerPhase::Up,
            TerminalPointerButton::Middle,
            if sgr { 1 } else { 3 },
            true,
        );
        t.put_bytes(b"\x1b[?1002l");
        t.handle_event(event(
            (2.0, 1.0),
            TerminalPointerPhase::Move,
            TerminalPointerButton::None,
        ))
        .unwrap();
        assert!(take(&written).is_empty());
        t.put_bytes(b"\x1b[?1002h");
        t.handle_event(event(
            (2.0, 1.0),
            TerminalPointerPhase::Down,
            TerminalPointerButton::Left,
        ))
        .unwrap();
        take(&written);
        let cancelled = t
            .handle_event_with_outcome(event(
                (2.0, 1.0),
                TerminalPointerPhase::Cancel,
                TerminalPointerButton::None,
            ))
            .unwrap();
        assert_eq!(cancelled.release_pointer, Some(7));
        t.handle_event(event(
            (2.0, 1.0),
            TerminalPointerPhase::Move,
            TerminalPointerButton::None,
        ))
        .unwrap();
        assert!(take(&written).is_empty());
        assert!(!t.pointer.has_active_pointer());
        assert!(!t.pointer.has_selection_state());
    }
    for (mode, limit) in [(0, 223.0), (1005, 2015.0)] {
        let (mut t, written) = terminal(3, 2020);
        t.pointer
            .set_viewport(RenderViewport::with_padding(1.0, 1.0, 0.0));
        t.put_bytes(format!("\x1b[?1000;{mode}h").as_bytes());
        let outcome = t
            .handle_event_with_outcome(event(
                (limit, 1.0),
                TerminalPointerPhase::Down,
                TerminalPointerButton::Left,
            ))
            .unwrap();
        assert_eq!(outcome.capture_pointer, Some(7));
        assert!(!t.pointer.has_selection_state());
        t.handle_event(event(
            (limit, 1.0),
            TerminalPointerPhase::WheelLine { dx: 0.0, dy: 1.0 },
            TerminalPointerButton::None,
        ))
        .unwrap();
        assert!(take(&written).is_empty());
        assert_eq!(t.screen.view_offset(), 0);
        let up = t
            .handle_event_with_outcome(event(
                (limit, 1.0),
                TerminalPointerPhase::Up,
                TerminalPointerButton::Left,
            ))
            .unwrap();
        assert_eq!(up.release_pointer, Some(7));
        assert!(take(&written).is_empty());
    }
}

#[test]
fn mouse_encoding_flags_queries_fallback_ris_and_tracking_off() {
    for order in [
        "1005;1015;1006;1016",
        "1016;1006;1015;1005",
        "1006;1005;1016;1015",
    ] {
        let (mut t, written) = terminal(10, 20);
        t.pointer
            .set_viewport(RenderViewport::with_padding(10.0, 20.0, 2.0));
        for mode in [1005, 1006, 1015, 1016, 9] {
            t.put_bytes(format!("\x1b[?{mode}$p").as_bytes());
        }
        assert_eq!(
            t.screen.drain_replies(),
            b"\x1b[?1005;2$y\x1b[?1006;2$y\x1b[?1015;2$y\x1b[?1016;2$y\x1b[?9;0$y"
        );
        t.put_bytes(format!("\x1b[?{order}h\x1b[?{order}h").as_bytes());
        let down = t
            .handle_event_with_outcome(event(
                (27.0, 23.0),
                TerminalPointerPhase::Down,
                TerminalPointerButton::Left,
            ))
            .unwrap();
        assert_eq!(down.capture_pointer, Some(7)); // Local selection, not VT.
        assert!(t.pointer.has_selection_state());
        assert!(take(&written).is_empty());
        t.handle_event(event(
            (27.0, 23.0),
            TerminalPointerPhase::Cancel,
            TerminalPointerButton::None,
        ))
        .unwrap();
        t.put_bytes(b"\x1b[?1000h");
        for (clear, encoding, expected) in [
            (0, MouseEncoding::SgrPixel, &b"\x1b[<0;26;22M"[..]),
            (1016, MouseEncoding::Sgr, &b"\x1b[<0;3;2M"[..]),
            (1006, MouseEncoding::Urxvt, &b"\x1b[32;3;2M"[..]),
            (1015, MouseEncoding::Utf8, &b"\x1b[M\x20\x23\x22"[..]),
            (1005, MouseEncoding::Legacy, &b"\x1b[M\x20\x23\x22"[..]),
        ] {
            t.put_bytes(format!("\x1b[?{clear}l\x1b[?{clear}l").as_bytes());
            assert_eq!(t.screen.input_modes().mouse_encoding(), encoding);
            for mode in [1005, 1006, 1015, 1016] {
                t.put_bytes(format!("\x1b[?{mode}$p").as_bytes());
                let enabled = match mode {
                    1005 => t.screen.input_modes().mouse_utf8,
                    1006 => t.screen.input_modes().mouse_sgr,
                    1015 => t.screen.input_modes().mouse_urxvt,
                    _ => t.screen.input_modes().mouse_sgr_pixel,
                };
                assert_eq!(
                    t.screen.drain_replies(),
                    format!("\x1b[?{mode};{}$y", if enabled { 1 } else { 2 }).as_bytes()
                );
            }
            t.handle_event(event(
                (27.0, 23.0),
                TerminalPointerPhase::Down,
                TerminalPointerButton::Left,
            ))
            .unwrap();
            assert_eq!(take(&written), expected);
        }
        t.put_bytes(b"\x1b[?1002;1003;1005;1006;1015;1016h\x1bc");
        assert_eq!(t.screen.input_modes(), InputModes::default());
        let cancel = t
            .handle_event_with_outcome(event(
                (27.0, 23.0),
                TerminalPointerPhase::Cancel,
                TerminalPointerButton::None,
            ))
            .unwrap();
        assert_eq!(cancel.release_pointer, Some(7));
        for mode in [1000, 1002, 1003, 1005, 1006, 1015, 1016, 9] {
            t.put_bytes(format!("\x1b[?{mode}$p").as_bytes());
            assert_eq!(
                t.screen.drain_replies(),
                format!("\x1b[?{mode};{}$y", if mode == 9 { 0 } else { 2 }).as_bytes()
            );
        }
        assert!(take(&written).is_empty());
    }
}

#[test]
fn mouse_fragmented_mode_query_reset_and_cancellation_streams() {
    let stream = b"\x1b[?1000;1005;1015;1016h\x1b[?1005$p\x1b[?1015$p\x1b[?1016$p\x1b[?1016l\x1b[?1006h\x1b[?1015$p\x1b[?1005l\x1b[?1016;9h\x1b[?9$p\x1bc\x1b[?1016$p\x1b[?101\x18\x1b[?1016h";
    let run = |chunks: Vec<&[u8]>| {
        let mut t = Terminal::new_headless(10, 20);
        for chunk in chunks {
            t.put_bytes(chunk);
        }
        (t.screen.input_modes(), t.screen.drain_replies())
    };
    let expected = run(vec![stream]);
    assert_eq!(expected.0.mouse_encoding(), MouseEncoding::SgrPixel);
    assert_eq!(expected.0.mouse_tracking, MouseTrackingMode::Disabled);
    assert_eq!(
        expected.1,
        b"\x1b[?1005;1$y\x1b[?1015;1$y\x1b[?1016;1$y\x1b[?1015;1$y\x1b[?9;0$y\x1b[?1016;2$y"
    );
    for split in 0..=stream.len() {
        assert_eq!(
            run(vec![&stream[..split], &stream[split..]]),
            expected,
            "split {split}"
        );
    }
    assert_eq!(run(stream.chunks(1).collect()), expected);
    for command in [
        &b"\x1b[?1000;1005h"[..],
        &b"\x1b[?1000;1015h"[..],
        &b"\x1b[?1000;1016h"[..],
    ] {
        let (mut t, written) = terminal(10, 20);
        t.pointer
            .set_viewport(RenderViewport::with_padding(10.0, 20.0, 2.0));
        t.put_bytes(&command[..command.len() - 1]);
        t.handle_event(event(
            (27.0, 23.0),
            TerminalPointerPhase::Down,
            TerminalPointerButton::Left,
        ))
        .unwrap();
        assert!(take(&written).is_empty());
        assert_eq!(t.screen.input_modes(), InputModes::default());
        t.put_bytes(&command[command.len() - 1..]);
        t.handle_event(event(
            (27.0, 23.0),
            TerminalPointerPhase::Down,
            TerminalPointerButton::Left,
        ))
        .unwrap();
        assert!(!take(&written).is_empty());
    }
}
#[test]
fn mouse_large_viewport_limits_are_not_hidden_by_cell_clamping() {
    for (mode, limit, bytes) in [
        (0, 223.0, &b"\x1b[M\x20\xff\xff"[..]),
        (1005, 2015.0, &b"\x1b[M\x20\xdf\xbf\xdf\xbf"[..]),
    ] {
        // A large square grid keeps both the boundary and overflow in content.
        // Avoid allocating a 2016² grid: test each large axis separately for UTF-8.
        let dimensions = if mode == 0 {
            vec![(225, 225)]
        } else {
            vec![(3, 2020), (2020, 3)]
        };
        for (rows, cols) in dimensions {
            let (mut t, written) = terminal(rows, cols);
            t.pointer
                .set_viewport(RenderViewport::with_padding(1.0, 1.0, 0.0));
            t.put_bytes(format!("\x1b[?1000;{mode}h").as_bytes());
            let position = (
                if cols > 3 { limit - 1.0 } else { 1.0 },
                if rows > 3 { limit - 1.0 } else { 1.0 },
            );
            t.handle_event(event(
                position,
                TerminalPointerPhase::Down,
                TerminalPointerButton::Left,
            ))
            .unwrap();
            let expected = if mode == 0 {
                bytes.to_vec()
            } else if cols > 3 {
                b"\x1b[M\x20\xdf\xbf\x22".to_vec()
            } else {
                b"\x1b[M\x20\x22\xdf\xbf".to_vec()
            };
            assert_eq!(take(&written), expected);
            let overflow = (
                if cols > 3 { limit } else { 1.0 },
                if rows > 3 { limit } else { 1.0 },
            );
            t.handle_event(event(
                overflow,
                TerminalPointerPhase::Down,
                TerminalPointerButton::Left,
            ))
            .unwrap();
            assert!(take(&written).is_empty());
            assert!(!t.pointer.has_selection_state());
        }
    }
}

#[test]
fn mouse_partial_query_and_disable_do_not_complete_prematurely() {
    for mode in [1005, 1015, 1016] {
        let query = format!("\x1b[?{mode}$p");
        let disable = format!("\x1b[?{mode}l");
        for split in 1..query.len() {
            let mut t = Terminal::new_headless(3, 8);
            t.put_bytes(format!("\x1b[?{mode}h").as_bytes());
            t.put_bytes(&query.as_bytes()[..split]);
            assert!(t.screen.drain_replies().is_empty());
            t.put_bytes(&query.as_bytes()[split..]);
            assert_eq!(
                t.screen.drain_replies(),
                format!("\x1b[?{mode};1$y").as_bytes()
            );
        }
        for split in 1..disable.len() {
            let mut t = Terminal::new_headless(3, 8);
            t.put_bytes(format!("\x1b[?{mode}h").as_bytes());
            let enabled = t.screen.input_modes();
            t.put_bytes(&disable.as_bytes()[..split]);
            assert_eq!(t.screen.input_modes(), enabled);
            t.put_bytes(&disable.as_bytes()[split..]);
            assert_eq!(t.screen.input_modes(), InputModes::default());
        }
    }
}

#[test]
fn mouse_encoding_flags_keep_existing_buffer_local_lifetimes() {
    let mut t = Terminal::new_headless(3, 8);
    t.put_bytes(b"\x1b[?1003;1005;1015;1016h");
    let primary = t.screen.input_modes();
    t.put_bytes(b"\x1b[?1049h");
    assert_eq!(t.screen.input_modes(), InputModes::default());
    t.put_bytes(b"\x1b[?1000;1006h");
    let alternate = t.screen.input_modes();
    t.put_bytes(b"\x1b[?1049l");
    assert_eq!(t.screen.input_modes(), primary);
    t.put_bytes(b"\x1b[?47h");
    assert_eq!(t.screen.input_modes(), alternate);
    // Clear-on-entry mode 1049 still resets the parked alternate state.
    t.put_bytes(b"\x1b[?47l\x1b[?1049h");
    assert_eq!(t.screen.input_modes(), InputModes::default());
}

struct ChannelReader(mpsc::Receiver<Vec<u8>>);
impl Read for ChannelReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        match self.0.recv() {
            Ok(bytes) => {
                buffer[..bytes.len()].copy_from_slice(&bytes);
                Ok(bytes.len())
            }
            Err(_) => Ok(0),
        }
    }
}
#[test]
fn mouse_queued_output_resolves_tracking_and_coordinate_units_before_projection() {
    let (tx, rx) = mpsc::channel();
    let (wake_tx, wake_rx) = mpsc::channel();
    let written = Arc::new(Mutex::new(Vec::new()));
    let mut t = Terminal::new_headless_with_io(
        10,
        20,
        ChannelReader(rx),
        RecordingWriter(written.clone()),
        move || wake_tx.send(()).is_ok(),
    );
    t.pointer
        .set_viewport(RenderViewport::with_padding(10.0, 20.0, 2.0));
    for (command, expected) in [
        (&b"\x1b[?1000;1016h"[..], &b"\x1b[<0;26;22M"[..]),
        (&b"\x1b[?1016l\x1b[?1006h"[..], &b"\x1b[<0;3;2M"[..]),
        (&b"\x1b[?1016h"[..], &b"\x1b[<0;26;22M"[..]),
        (&b"\x1b[?1000l"[..], &b""[..]),
    ] {
        tx.send(command.to_vec()).unwrap();
        wake_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        t.handle_event(event(
            (27.0, 23.0),
            TerminalPointerPhase::Down,
            TerminalPointerButton::Left,
        ))
        .unwrap();
        assert_eq!(take(&written), expected);
    }
    assert!(t.pointer.has_selection_state());
    drop(tx);
}
