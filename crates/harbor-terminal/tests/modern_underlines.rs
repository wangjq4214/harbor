//! T0001: GPU-independent parser/state/status and resize contracts.
use harbor_terminal::{Cell, CellAttrs, Color, Screen, TerminalParser, UnderlineStyle};

fn feed(parser: &mut TerminalParser, screen: &mut Screen, bytes: &[u8]) {
    let mut offset = 0;
    while offset < bytes.len() {
        let result = parser.put_bytes(screen, &bytes[offset..]);
        offset += result.consumed;
        if let Some(action) = result.alt_request {
            match action {
                harbor_terminal::AltScreenAction::Enter { clear } => screen.enter_alt(clear),
                harbor_terminal::AltScreenAction::Exit => screen.exit_alt(),
            }
        }
    }
}

fn cell_after(sgr: &str) -> Cell {
    let mut screen = Screen::new(2, 20);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, format!("\x1b[{sgr}mX").as_bytes());
    screen.cell(0, 0).clone()
}

#[test]
fn style_compatibility_ordering_and_independent_color() {
    use UnderlineStyle::*;
    for (input, style) in [
        ("4", Single),
        ("4:0", Off),
        ("4:1", Single),
        ("4:2", Double),
        ("4:3", Curly),
        ("4:4", Dotted),
        ("4:5", Dashed),
        ("21", Double),
        ("4:3;24", Off),
        ("4:3;4:99", Curly),
        ("4:3;4:", Curly),
        ("4:3;4:2:1", Curly),
        ("4:3;0;4:5", Dashed),
    ] {
        assert_eq!(cell_after(input).attrs.underline_style(), style, "{input}");
    }
    let cell = cell_after("1;21");
    assert!(cell.attrs.contains(CellAttrs::BOLD));
    assert_eq!(cell.attrs.underline_style(), Double);
    assert!(!cell_after("4:3").attrs.contains(CellAttrs::ITALIC));
    for off in ["24", "4:0"] {
        let cell = cell_after(&format!("4:3;58;5;123;{off}"));
        assert_eq!(cell.attrs.underline_style(), Off);
        assert_eq!(cell.underline_color, Color::Indexed(123));
        assert_eq!(
            cell_after(&format!("58;5;123;{off};4:5")).underline_color,
            Color::Indexed(123)
        );
    }
    assert_eq!(cell_after("58;5;123").attrs.underline_style(), Off);
    assert_eq!(
        cell_after("4:3;58;5;123;59").underline_color,
        Color::Default
    );
    for reset in ["0", ""] {
        let cell = cell_after(&format!("4:5;58;5;123;{reset}"));
        assert_eq!(cell.attrs.underline_style(), Off);
        assert_eq!(cell.underline_color, Color::Default);
    }
}

#[test]
fn selected_color_forms_and_invalid_candidates_do_not_leak() {
    for (form, expected) in [
        ("58;5;0", Color::Indexed(0)),
        ("58:5:0", Color::Indexed(0)),
        ("58;5;255", Color::Indexed(255)),
        ("58:5:255", Color::Indexed(255)),
        ("58;2;0;17;255", Color::Rgb(0, 17, 255)),
        ("58:2:0:17:255", Color::Rgb(0, 17, 255)),
        ("58:2::0:17:255", Color::Rgb(0, 17, 255)),
    ] {
        assert_eq!(cell_after(form).underline_color, expected, "{form}");
        let stream = format!("\x1b[{form}mX");
        for split in 0..=stream.len() {
            let mut screen = Screen::new(2, 20);
            let mut parser = TerminalParser::default();
            feed(&mut parser, &mut screen, &stream.as_bytes()[..split]);
            feed(&mut parser, &mut screen, &stream.as_bytes()[split..]);
            assert_eq!(
                screen.cell(0, 0).underline_color,
                expected,
                "{form} split {split}"
            );
        }
    }
    for invalid in [
        "58;5;256",
        "58:5:256",
        "58;5",
        "58:5",
        "58:5:",
        "58:5:1:2",
        "58;2;1;2",
        "58;2;1;;3",
        "58;2;256;2;3",
        "58;2;1;256;3",
        "58;2;1;2;256",
        "58:2:256:2:3",
        "58:2:1:256:3",
        "58:2:1:2:256",
        "58:2::1:2",
        "58:2::1:2:256",
        "58:2:1:1:2:3",
        "58:2:::1:2:3",
        "58;2;1:4;2;3",
        "58;5;1:4",
    ] {
        let cell = cell_after(&format!("4:3;58;5;42;{invalid}"));
        assert_eq!(cell.underline_color, Color::Indexed(42), "{invalid}");
        assert_eq!(
            cell.attrs.underline_style(),
            UnderlineStyle::Curly,
            "{invalid}"
        );
        for bit in [
            CellAttrs::BOLD,
            CellAttrs::DIM,
            CellAttrs::ITALIC,
            CellAttrs::BLINK,
        ] {
            assert!(!cell.attrs.contains(bit), "{invalid}: leaked attribute");
        }
    }
    let mixed = cell_after("4:3;58:2::1:2:3;1;24;4:5;58;5;256;31");
    assert_eq!(mixed.underline_color, Color::Rgb(1, 2, 3));
    assert_eq!(mixed.attrs.underline_style(), UnderlineStyle::Dashed);
    assert!(mixed.attrs.contains(CellAttrs::BOLD));
    assert_eq!(mixed.fg, Color::Named(1));
}

#[test]
fn fragmentation_cancellation_bounds_and_recovery() {
    let streams = [
        "\x1b[1;4:3;58:2::12:34:56m界 X\x1b[24;4:5;58;5;123mY",
        "\x1b[4:2;58;2;1;2;256;31mA\x1b[59;4:0mB",
        "\x1b[4:3;58;5;42m\x1b[4:5;58:2::1\x18X\x1b[4:4mY",
        "\x1b[4:3;58;5;42m\x1b[4:5;58;2\x1aX\x1b[4mY",
        "\x1b[4:3;58;5;42m\x1b[58:2::1:2:3:4:5:6:7mX\x1b[4:5mY",
        "\x1b[4:3;58;5;42m\x1b[58:2::1?2:3mX\x1b[4:5mY",
        "\x1b[58:5:99999999999999999mX\x1b[4:5mY",
        "\x1b[4:3;58;5;42m\x1bP$qm\x1b\\",
    ];
    for stream in streams {
        let bytes = stream.as_bytes();
        let mut baseline = Screen::new(4, 20);
        feed(&mut TerminalParser::default(), &mut baseline, bytes);
        let expected = baseline.terminal_snapshot().cells;
        let replies = baseline.drain_replies();
        for split in 0..=bytes.len() {
            let mut parser = TerminalParser::default();
            let mut screen = Screen::new(4, 20);
            feed(&mut parser, &mut screen, &bytes[..split]);
            feed(&mut parser, &mut screen, &bytes[split..]);
            assert_eq!(
                screen.terminal_snapshot().cells,
                expected,
                "split {split} in {stream:?}"
            );
            assert_eq!(screen.drain_replies(), replies);
        }
        let mut parser = TerminalParser::default();
        let mut screen = Screen::new(4, 20);
        for byte in bytes {
            feed(&mut parser, &mut screen, &[*byte]);
        }
        assert_eq!(screen.terminal_snapshot().cells, expected);
    }
    // Cancelled and over-subparameter CSI must not commit partially collected state.
    for invalid in [
        "\x1b[4:5;58;5;1\x18",
        "\x1b[4:5;58;5;1\x1a",
        "\x1b[58:2::1:2:3:4:5:6:7m",
    ] {
        let mut parser = TerminalParser::default();
        let mut screen = Screen::new(2, 20);
        feed(
            &mut parser,
            &mut screen,
            format!("\x1b[4:3;58;5;42m{invalid}X").as_bytes(),
        );
        assert_eq!(
            screen.cell(0, 0).attrs.underline_style(),
            UnderlineStyle::Curly
        );
        assert_eq!(screen.cell(0, 0).underline_color, Color::Indexed(42));
    }
}

#[test]
fn decrqss_exact_round_trip_and_cancelled_requests() {
    for (sgr, status) in [
        ("", "0m"),
        ("1;4;7;31", "0;1;4;7;31m"),
        ("1;21;58;5;123", "0;1;4:2;58;5;123m"),
        ("4:3;58:2::1:2:3", "0;4:3;58;2;1;2;3m"),
        ("4:4;58:5:255;24", "0;58;5;255m"),
        ("4:5;58;2;255;255;255;59", "0;4:5m"),
    ] {
        let mut screen = Screen::new(2, 20);
        let mut parser = TerminalParser::default();
        feed(
            &mut parser,
            &mut screen,
            format!("\x1b[{sgr}m\x1bP$qm\x1b\\").as_bytes(),
        );
        assert_eq!(
            screen.drain_replies(),
            format!("\x1bP1$r{status}\x1b\\").as_bytes()
        );
        feed(&mut parser, &mut screen, b"X");
        let original = screen.cell(0, 0);
        let rebuilt = cell_after(status.trim_end_matches('m'));
        assert_eq!(rebuilt.attrs, original.attrs);
        assert_eq!(rebuilt.underline_color, original.underline_color);
        assert_eq!((rebuilt.fg, rebuilt.bg), (original.fg, original.bg));
    }
    let mut screen = Screen::new(2, 20);
    let mut parser = TerminalParser::default();
    feed(
        &mut parser,
        &mut screen,
        b"\x1b[4:3;58;5;42m\x1bP$qm\x18\x1bP$qm\x1a",
    );
    assert!(screen.drain_replies().is_empty());
    let maximal = "\x1b[1;2;3;4:5;5;7;9;38:2:255:255:255;48:2:255:255:255;58:2::255:255:255m";
    feed(
        &mut parser,
        &mut screen,
        format!("{maximal}\x1bP$qm\x1b\\").as_bytes(),
    );
    assert!(screen.drain_replies().len() <= 128);
}

#[test]
fn saved_pen_wide_edits_history_and_reset_lifetimes() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(4, 12);
    feed(
        &mut parser,
        &mut screen,
        "\x1b[4:3;58:2::1:2:3m界".as_bytes(),
    );
    for col in 0..2 {
        assert_eq!(
            screen.cell(0, col).attrs.underline_style(),
            UnderlineStyle::Curly
        );
        assert_eq!(screen.cell(0, col).underline_color, Color::Rgb(1, 2, 3));
    }
    for save_restore in ["\x1b7\x1b[0m\x1b8", "\x1b[s\x1b[0m\x1b[u"] {
        feed(
            &mut parser,
            &mut screen,
            format!("{save_restore}X").as_bytes(),
        );
        assert_eq!(
            screen.cell(0, screen.cursor_x() - 1).underline_color,
            Color::Rgb(1, 2, 3)
        );
    }
    // Move a complete wide unit right, then delete the inserted prefix.
    feed(&mut parser, &mut screen, b"\x1b[1;1H\x1b[@");
    assert_eq!(screen.cell(0, 1).ch, '界');
    assert_eq!(screen.cell(0, 2).underline_color, Color::Rgb(1, 2, 3));
    feed(&mut parser, &mut screen, b"\x1b[P");
    assert_eq!(screen.cell(0, 0).ch, '界');
    assert_eq!(
        screen.cell(0, 1).attrs.underline_style(),
        UnderlineStyle::Curly
    );
    feed(&mut parser, &mut screen, b"\x1b[0m\x1b[4;1H\n");
    screen.scroll_up(1);
    assert_eq!(screen.cell(0, 0).underline_color, Color::Rgb(1, 2, 3));
    screen.scroll_to_bottom();
    for reset in ["\x1b[0m", "\x1b[m", "\x1b[!p", "\x1bc"] {
        let mut screen = Screen::new(2, 20);
        feed(
            &mut parser,
            &mut screen,
            format!("\x1b[4:5;58;5;42m{reset}X").as_bytes(),
        );
        assert_eq!(
            screen.cell(0, 0).attrs.underline_style(),
            UnderlineStyle::Off
        );
        assert_eq!(screen.cell(0, 0).underline_color, Color::Default);
    }
}

#[test]
fn decorated_written_and_erase_tails_survive_reflow_without_retaining_capacity() {
    for style in 1..=5 {
        for operation in ["    ", "\x1b[4X", "\x1b[K", "\x1b[32;1;3;1;6$x"] {
            let mut screen = Screen::new(4, 6);
            let mut parser = TerminalParser::default();
            feed(
                &mut parser,
                &mut screen,
                format!("AB\x1b[4:{style};58;5;42m{operation}\x1b[0m\x1b[1;1H").as_bytes(),
            );
            screen.resize(4, 3);
            for (row, col) in [(0, 2), (1, 0), (1, 1), (1, 2)] {
                assert_eq!(screen.cell(row, col).attrs.underline_style() as u8, style);
                assert_eq!(screen.cell(row, col).underline_color, Color::Indexed(42));
            }
            screen.resize(4, 6);
            assert_eq!(screen.row_text(0), "AB    ");
            for col in 2..6 {
                assert_eq!(screen.cell(0, col).underline_color, Color::Indexed(42));
            }
            // Fresh capacity never inherits the erase pen.
            assert_eq!(screen.cell(2, 0), &Cell::default());
        }
    }
}

#[test]
fn alternate_resize_is_rectangular_and_saved_primary_reflows_without_leaks() {
    let mut screen = Screen::new(4, 6);
    let mut parser = TerminalParser::default();
    feed(
        &mut parser,
        &mut screen,
        b"AB\x1b[4:3;58;5;42m    \x1b[1;1H\x1b[?1049h",
    );
    assert_eq!(
        cell_after("0").underline_color,
        screen.cell(0, 0).underline_color
    );
    feed(
        &mut parser,
        &mut screen,
        b"\x1b[4:5;58;5;123mXYZ123\x1b[1;1H",
    );
    screen.resize(4, 3);
    assert_eq!(screen.row_text(0), "XYZ");
    assert_eq!(screen.cell(0, 2).underline_color, Color::Indexed(123));
    assert_eq!(screen.cell(1, 0), &Cell::default());
    screen.resize(4, 6);
    feed(&mut parser, &mut screen, b"\x1b[?1049l");
    assert_eq!(screen.row_text(0), "AB    ");
    for col in 2..6 {
        assert_eq!(
            screen.cell(0, col).attrs.underline_style(),
            UnderlineStyle::Curly
        );
        assert_eq!(screen.cell(0, col).underline_color, Color::Indexed(42));
    }
    feed(&mut parser, &mut screen, b"X");
    assert_eq!(screen.cell(0, 0).underline_color, Color::Indexed(42));
}

#[test]
fn wide_reflow_preserves_style_color_and_original_copy_text() {
    let mut screen = Screen::new(4, 6);
    let mut parser = TerminalParser::default();
    feed(
        &mut parser,
        &mut screen,
        "\x1b[4:5;58:2::1:2:3m界界  \x1b[0m\x1b[1;1H".as_bytes(),
    );
    screen.resize(4, 2);
    for row in 0..2 {
        for col in 0..2 {
            assert_eq!(
                screen.cell(row, col).attrs.underline_style(),
                UnderlineStyle::Dashed
            );
            assert_eq!(screen.cell(row, col).underline_color, Color::Rgb(1, 2, 3));
        }
        assert!(screen.cell(row, 1).wide_continuation);
    }
    screen.resize(4, 6);
    let generation = screen.history_start();
    assert_eq!(
        screen.selected_text(harbor_terminal::SelectionBounds {
            start_row: generation,
            start_col: 0,
            end_row: generation,
            end_col: 5,
        }),
        "界界  "
    );
    for col in 0..6 {
        assert_eq!(screen.cell(0, col).underline_color, Color::Rgb(1, 2, 3));
    }
}

#[test]
fn legacy_attribute_api_has_only_one_authoritative_underline() {
    for style in [
        UnderlineStyle::Double,
        UnderlineStyle::Curly,
        UnderlineStyle::Dotted,
        UnderlineStyle::Dashed,
    ] {
        let mut cell = Cell::default();
        cell.attrs.set_underline_style(style);
        assert!(cell.attrs.contains(CellAttrs::UNDERLINE));
        cell.toggle_sgr(4);
        assert_eq!(cell.attrs.underline_style(), UnderlineStyle::Off);
        cell.apply_sgr(21);
        assert_eq!(cell.attrs.underline_style(), UnderlineStyle::Double);
        cell.apply_sgr(24);
        assert!(!cell.attrs.contains(CellAttrs::UNDERLINE));
        cell.apply_sgr(4);
        assert_eq!(cell.attrs.underline_style(), UnderlineStyle::Single);
    }
}
