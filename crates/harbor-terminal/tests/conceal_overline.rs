//! T0002: conceal is presentation, not redaction; overline is independent styling.
use harbor_terminal::{
    AltScreenAction, Cell, CellAttrs, Color, Screen, SelectionBounds, TerminalParser,
};

fn feed(parser: &mut TerminalParser, screen: &mut Screen, bytes: &[u8]) {
    let mut offset = 0;
    while offset < bytes.len() {
        let result = parser.put_bytes(screen, &bytes[offset..]);
        assert!(result.consumed > 0);
        offset += result.consumed;
        if let Some(action) = result.alt_request {
            match action {
                AltScreenAction::Enter { clear } => screen.enter_alt(clear),
                AltScreenAction::Exit => screen.exit_alt(),
            }
        }
    }
}

fn cell_after(sgr: &str) -> Cell {
    let mut screen = Screen::new(2, 20);
    feed(
        &mut TerminalParser::default(),
        &mut screen,
        format!("\x1b[{sgr}mX").as_bytes(),
    );
    screen.cell(0, 0).clone()
}

fn assert_attrs(cell: &Cell, conceal: bool, overline: bool) {
    assert_eq!(cell.attrs.contains(CellAttrs::CONCEAL), conceal);
    assert_eq!(cell.attrs.contains(CellAttrs::OVERLINE), overline);
}

#[test]
fn independent_ordered_attributes_and_resets_do_not_restyle_existing_cells() {
    for (sgr, conceal, overline) in [
        ("8;53", true, true),
        ("8;53;28", false, true),
        ("8;53;55", true, false),
        ("8;53;28;55", false, false),
        ("8;53;0", false, false),
        ("0;53;8", true, true),
    ] {
        assert_attrs(&cell_after(sgr), conceal, overline);
    }
    let cell = cell_after("1;2;3;4;5;7;9;31;44;8;53;28;55");
    for attr in [
        CellAttrs::BOLD,
        CellAttrs::DIM,
        CellAttrs::ITALIC,
        CellAttrs::UNDERLINE,
        CellAttrs::BLINK,
        CellAttrs::INVERSE,
        CellAttrs::STRIKETHROUGH,
    ] {
        assert!(cell.attrs.contains(attr));
    }
    assert_eq!((cell.fg, cell.bg), (Color::Named(1), Color::Named(4)));
    let mut screen = Screen::new(2, 20);
    feed(
        &mut TerminalParser::default(),
        &mut screen,
        b"\x1b[8;53mA\x1b[28mB\x1b[55mC",
    );
    assert_attrs(screen.cell(0, 0), true, true);
    assert_attrs(screen.cell(0, 1), false, true);
    assert_attrs(screen.cell(0, 2), false, false);
}

#[test]
fn concealed_source_suffix_width_background_cursor_and_copy_are_intact() {
    let mut screen = Screen::new(4, 12);
    let mut parser = TerminalParser::default();
    let source = "Ae\u{301}界\u{301} ";
    feed(
        &mut parser,
        &mut screen,
        format!("\x1b[8;53;4;9;7;31;44m{source}").as_bytes(),
    );
    assert_eq!(screen.cursor_x(), 5);
    assert_eq!(screen.cell(0, 1).raw_text(), "e\u{301}");
    assert_eq!(screen.cell(0, 2).raw_text(), "界\u{301}");
    assert_eq!(screen.cell(0, 2).grid_width(), 2);
    assert!(screen.cell(0, 3).wide_continuation);
    // A + e + wide CJK + space occupies five cells.
    assert_eq!(screen.cell(0, 5).ch, ' ');
    for col in 0..5 {
        assert_attrs(screen.cell(0, col), true, true);
        assert_eq!(screen.cell(0, col).bg, Color::Named(4));
    }
    let generation = screen.history_start();
    let copied = screen.selected_text(SelectionBounds {
        start_row: generation,
        start_col: 0,
        end_row: generation,
        end_col: 4,
    });
    assert_eq!(copied, source);
    feed(&mut parser, &mut screen, b"\x1b[0m\x1b[1;1H");
    screen.resize(4, 3);
    screen.resize(4, 12);
    assert_eq!(screen.cell(0, 1).raw_text(), "e\u{301}");
    assert_eq!(screen.cell(0, 2).raw_text(), "界\u{301}");
    assert_attrs(screen.cell(0, 3), true, true);
    assert_eq!(
        screen.selected_text(SelectionBounds {
            start_row: screen.history_start(),
            start_col: 0,
            end_row: screen.history_start(),
            end_col: 4,
        }),
        source
    );
}

#[test]
fn saved_pen_edit_scroll_snapshots_and_reset_lifetimes() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(4, 12);
    feed(&mut parser, &mut screen, "\x1b[8;53m界".as_bytes());
    for save_restore in ["\x1b7\x1b[0m\x1b8", "\x1b[s\x1b[0m\x1b[u"] {
        feed(
            &mut parser,
            &mut screen,
            format!("{save_restore}X").as_bytes(),
        );
        assert_attrs(screen.cell(0, screen.cursor_x() - 1), true, true);
    }
    feed(&mut parser, &mut screen, b"\x1b[1;1H\x1b[@");
    assert_eq!(screen.cell(0, 1).ch, '界');
    assert_attrs(screen.cell(0, 2), true, true);
    feed(&mut parser, &mut screen, b"\x1b[P");
    assert_eq!(screen.cell(0, 0).ch, '界');
    assert_attrs(screen.cell(0, 1), true, true);
    feed(&mut parser, &mut screen, b"\x1b[0m\x1b[4;1H\n");
    screen.scroll_up(1);
    let snap = screen.terminal_snapshot();
    assert_attrs(snap.cell(0, 0), true, true);
    assert_attrs(snap.cell(0, 1), true, true);
    for reset in ["\x1b[0m", "\x1b[m", "\x1b[!p", "\x1bc"] {
        let mut screen = Screen::new(2, 20);
        feed(
            &mut parser,
            &mut screen,
            format!("\x1b[8;53m{reset}X").as_bytes(),
        );
        assert_attrs(screen.cell(0, 0), false, false);
    }
}

#[test]
fn decorated_print_erase_fill_tails_survive_reflow_even_when_concealed() {
    for attrs in ["53", "53;8", "4;8"] {
        for operation in ["    ", "\x1b[4X", "\x1b[K", "\x1b[32;1;3;1;6$x"] {
            let mut screen = Screen::new(4, 6);
            feed(
                &mut TerminalParser::default(),
                &mut screen,
                format!("AB\x1b[{attrs}m{operation}\x1b[0m\x1b[1;1H").as_bytes(),
            );
            screen.resize(4, 3);
            for (row, col) in [(0, 2), (1, 0), (1, 1), (1, 2)] {
                assert_eq!(
                    screen.cell(row, col).attrs,
                    cell_after(attrs).attrs,
                    "{attrs}/{operation:?}"
                );
            }
            screen.resize(4, 6);
            assert_eq!(screen.row_text(0), "AB    ");
            for col in 2..6 {
                assert_eq!(screen.cell(0, col).attrs, cell_after(attrs).attrs);
            }
            assert_eq!(
                screen.cell(2, 0),
                &Cell::default(),
                "fresh capacity is not decorated"
            );
        }
    }
    let mut ordinary = Screen::new(4, 6);
    feed(
        &mut TerminalParser::default(),
        &mut ordinary,
        b"AB    \x1b[1;1H",
    );
    ordinary.resize(4, 3);
    assert_eq!(ordinary.row_text(0), "AB ");
    assert_eq!(ordinary.cell(1, 0), &Cell::default());
}

#[test]
fn alternate_rectangular_resize_and_primary_pen_are_isolated() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(4, 6);
    feed(
        &mut parser,
        &mut screen,
        b"AB\x1b[8;53m    \x1b[1;1H\x1b[?1049h",
    );
    assert_eq!(screen.cell(0, 0), &Cell::default());
    feed(&mut parser, &mut screen, b"\x1b[53mXYZ123\x1b[1;1H");
    screen.resize(4, 3);
    assert_eq!(screen.row_text(0), "XYZ");
    assert_attrs(screen.cell(0, 2), false, true);
    assert_eq!(screen.cell(1, 0), &Cell::default());
    screen.resize(4, 6);
    feed(&mut parser, &mut screen, b"\x1b[?1049l");
    for col in 2..6 {
        assert_attrs(screen.cell(0, col), true, true);
    }
    feed(&mut parser, &mut screen, b"X");
    assert_attrs(screen.cell(0, 0), true, true);
}

#[test]
fn fragmented_malformed_cancelled_and_bounded_input_recovers() {
    let over_limit = format!("\x1b[{}8;53mX\x1b[0mY", "1;".repeat(40));
    for stream in [
        "\x1b[8;53;31m界e\u{301}\x1b[28;55mY",
        "\x1b[8;53m\x1b[28;55\x18X\x1b[28;55mY",
        "\x1b[8;53m\x1b[28;55\x1aX\x1b[0mY",
        "\x1b[8;53m\x1b[28?55mX\x1b[0mY",
        "\x1b[99999999999999999999999999mX\x1b[8;53mY",
        "\x1b[8;53m\x1b[999:28:55mX\x1b[0mY",
        &over_limit,
    ] {
        let mut baseline = Screen::new(4, 20);
        feed(
            &mut TerminalParser::default(),
            &mut baseline,
            stream.as_bytes(),
        );
        let expected = baseline.terminal_snapshot().cells;
        for split in 0..=stream.len() {
            let mut parser = TerminalParser::default();
            let mut screen = Screen::new(4, 20);
            feed(&mut parser, &mut screen, &stream.as_bytes()[..split]);
            feed(&mut parser, &mut screen, &stream.as_bytes()[split..]);
            assert_eq!(
                screen.terminal_snapshot().cells,
                expected,
                "{stream:?} split {split}"
            );
        }
    }
    for cancelled in ["\x1b[28;55\x18", "\x1b[28;55\x1a", "\x1b[28?55m"] {
        let mut screen = Screen::new(2, 20);
        feed(
            &mut TerminalParser::default(),
            &mut screen,
            format!("\x1b[8;53m{cancelled}X\x1b[28;55mY").as_bytes(),
        );
        assert_attrs(screen.cell(0, 0), true, true);
        assert_attrs(screen.cell(0, 1), false, false);
    }
}

#[test]
fn decrqss_exact_bytes_roundtrip_fragmentation_cancellation_and_bounds() {
    for (sgr, status) in [
        ("", "0m"),
        ("8", "0;8m"),
        ("53", "0;53m"),
        ("8;53", "0;8;53m"),
        ("8;53;28", "0;53m"),
        ("8;53;55", "0;8m"),
        ("8;53;0", "0m"),
        ("1;4;7;9;31", "0;1;4;7;9;31m"),
        ("1;4;7;8;9;53;31;44", "0;1;4;7;8;9;53;31;44m"),
    ] {
        let stream = format!("\x1b[{sgr}m\x1bP$qm\x1b\\");
        for split in 0..=stream.len() {
            let mut screen = Screen::new(2, 20);
            let mut parser = TerminalParser::default();
            feed(&mut parser, &mut screen, &stream.as_bytes()[..split]);
            feed(&mut parser, &mut screen, &stream.as_bytes()[split..]);
            assert_eq!(
                screen.drain_replies(),
                format!("\x1bP1$r{status}\x1b\\").as_bytes()
            );
            feed(&mut parser, &mut screen, b"X");
            assert_eq!(screen.cell(0, 0), &cell_after(status.trim_end_matches('m')));
        }
    }
    let mut screen = Screen::new(2, 20);
    let mut parser = TerminalParser::default();
    feed(
        &mut parser,
        &mut screen,
        b"\x1b[8;53m\x1bP$qm\x18\x1bP$qm\x1a",
    );
    assert!(screen.drain_replies().is_empty());
    feed(&mut parser, &mut screen, b"\x1b[1;2;3;4;5;7;8;9;53;38;2;255;255;255;48;2;255;255;255;58;2;255;255;255m\x1bP$qm\x1b\\");
    let reply = screen.drain_replies();
    assert!(reply.starts_with(b"\x1bP1$r0;1;2;3;4;5;7;8;9;53;"));
    assert!(reply.ends_with(b"m\x1b\\"));
    assert!(reply.len() <= 128);
}

#[test]
fn cell_attribute_api_preserves_underlines_and_accepts_overline_without_bit_aliasing() {
    let mut cell = Cell::default();
    cell.apply_sgr(4);
    cell.apply_sgr(8);
    cell.apply_sgr(53);
    assert_attrs(&cell, true, true);
    assert!(cell.attrs.contains(CellAttrs::UNDERLINE));
    cell.apply_sgr(28);
    cell.apply_sgr(55);
    assert_attrs(&cell, false, false);
    assert!(cell.attrs.contains(CellAttrs::UNDERLINE));
    // Arbitrary public masks cannot corrupt private underline-style bits.
    cell.attrs.set(u16::MAX);
    cell.attrs.toggle(u16::MAX);
    cell.attrs.clear(u16::MAX);
    assert!(cell.attrs.is_empty());
}
