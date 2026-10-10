//! T0003: capability honesty and combined modern SGR state through the public core.
use harbor_terminal::{
    AltScreenAction, Cell, CellAttrs, Color, Screen, SelectionBounds, TerminalParser,
    UnderlineStyle,
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

#[test]
fn su_exact_mixed_unknown_fragmented_cancelled_and_identity_replies() {
    for (request, expected) in [
        ("\x1bP+q5375\x1b\\", "\x1bP1+r5375\x1b\\"),
        (
            "\x1bP+q7878;5375;7538;524742;544E\x1b\\",
            "\x1bP1+r5375;7538;524742=382F382F38;544E=787465726D2D323536636F6C6F72\x1b\\",
        ),
        ("\x1bP+q7375;7878\x1b\\", "\x1bP0+r\x1b\\"),
        ("\x1bP+q5375\x18\x1bP+q5375\x1a", ""),
        ("\x1b[c\x1b[>c", "\x1b[?62;6;17;22;28c\x1b[>1;1;0c"),
    ] {
        for split in 0..=request.len() {
            let mut screen = Screen::new(2, 20);
            let mut parser = TerminalParser::default();
            feed(&mut parser, &mut screen, &request.as_bytes()[..split]);
            feed(&mut parser, &mut screen, &request.as_bytes()[split..]);
            assert_eq!(screen.drain_replies(), expected.as_bytes(), "split {split}");
            feed(&mut parser, &mut screen, b"X");
            assert_eq!(
                screen.cell(0, 0),
                &Cell {
                    ch: 'X',
                    ..Cell::default()
                }
            );
        }
    }
}

#[test]
fn su_request_reply_bounds_and_recovery_use_existing_transport() {
    for count in [50, 51, 52] {
        let request = format!("\x1bP+q{}\x1b\\", vec!["5375"; count].join(";"));
        let mut screen = Screen::new(2, 20);
        let mut parser = TerminalParser::default();
        feed(&mut parser, &mut screen, request.as_bytes());
        let reply = screen.drain_replies();
        // 50 names fit the 256-byte reply; 51 fit the request but exceed the reply.
        if count == 50 {
            assert_eq!(
                reply,
                format!("\x1bP1+r{}\x1b\\", vec!["5375"; count].join(";")).as_bytes()
            );
            assert!(reply.len() <= 256);
        } else {
            assert_eq!(reply, b"\x1bP0+r\x1b\\");
        }
        feed(&mut parser, &mut screen, b"\x1bP+q5375\x1b\\X");
        assert_eq!(screen.drain_replies(), b"\x1bP1+r5375\x1b\\");
        assert_eq!(screen.cell(0, 0).ch, 'X');
    }
}

#[test]
fn combined_status_exact_bytes_reconstructs_the_pen_from_reset() {
    for style in 0..=5 {
        for (color, canonical) in [
            ("58:5:123", "58;5;123"),
            ("58:2::10:20:30", "58;2;10;20;30"),
        ] {
            let sgr = format!("1;2;3;4:{style};5;7;8;9;53;38:2:1:2:3;48:5:9;{color}");
            let underline = match style {
                0 => String::new(),
                1 => ";4".into(),
                _ => format!(";4:{style}"),
            };
            let canonical = canonical.replace(';', ":");
            let status = format!("0;1;2;3{underline};5;7;8;9;53;38:2:1:2:3;48:5:9;{canonical}m");
            let stream = format!("\x1b[{sgr}m\x1bP$qm\x1b\\");
            for split in 0..=stream.len() {
                let mut screen = Screen::new(2, 20);
                let mut parser = TerminalParser::default();
                feed(&mut parser, &mut screen, &stream.as_bytes()[..split]);
                feed(&mut parser, &mut screen, &stream.as_bytes()[split..]);
                let reply = screen.drain_replies();
                assert_eq!(reply, format!("\x1bP1$r{status}\x1b\\").as_bytes());
                assert!(reply.len() <= 128);
                feed(&mut parser, &mut screen, b"X");
                let mut restored = Screen::new(2, 20);
                // Apply the actual serialized payload as CSI, not the original SGR.
                let mut replay = b"\x1b[".to_vec();
                replay.extend_from_slice(&reply[5..reply.len() - 2]);
                replay.push(b'X');
                feed(&mut TerminalParser::default(), &mut restored, &replay);
                assert_eq!(restored.cell(0, 0), screen.cell(0, 0));
            }
        }
    }
}

#[test]
fn modern_status_compacts_only_past_the_top_level_parameter_limit() {
    for (dim, status) in [
        ("", "0;1;3;4:3;5;7;8;9;53;31;48;5;9;58;5;123m"),
        ("2;", "0;1;2;3;4:3;5;7;8;9;53;31;48:5:9;58:5:123m"),
    ] {
        let mut screen = Screen::new(2, 20);
        let mut parser = TerminalParser::default();
        feed(
            &mut parser,
            &mut screen,
            format!("\x1b[1;{dim}3;4:3;5;7;8;9;53;31;48:5:9;58:5:123m\x1bP$qm\x1b\\X").as_bytes(),
        );
        let reply = screen.drain_replies();
        assert_eq!(reply, format!("\x1bP1$r{status}\x1b\\").as_bytes());
        let mut restored = Screen::new(2, 20);
        feed(
            &mut TerminalParser::default(),
            &mut restored,
            format!("\x1b[{status}X").as_bytes(),
        );
        assert_eq!(restored.cell(0, 0), screen.cell(0, 0));
    }
}

#[test]
fn combined_decorated_blank_reflow_copy_and_erase_preserve_source() {
    for style in 1..=5 {
        for operation in ["    ", "\x1b[4X", "\x1b[K", "\x1b[32;1;3;1;6$x"] {
            let mut screen = Screen::new(4, 6);
            let mut parser = TerminalParser::default();
            feed(
                &mut parser,
                &mut screen,
                format!("AB\x1b[4:{style};58;2;10;20;30;8;53m{operation}\x1b[0m\x1b[1;1H")
                    .as_bytes(),
            );
            let expected = screen.cell(0, 2).clone();
            assert!(expected.attrs.contains(CellAttrs::CONCEAL));
            assert!(expected.attrs.contains(CellAttrs::OVERLINE));
            assert_eq!(expected.underline_color, Color::Rgb(10, 20, 30));
            screen.resize(4, 3);
            for (row, col) in [(0, 2), (1, 0), (1, 1), (1, 2)] {
                assert_eq!(screen.cell(row, col), &expected);
            }
            screen.resize(4, 6);
            assert_eq!(
                screen.selected_text(SelectionBounds {
                    start_row: screen.history_start(),
                    start_col: 0,
                    end_row: screen.history_start(),
                    end_col: 5
                }),
                "AB    "
            );
        }
    }
}

#[test]
fn combined_save_alternate_reset_and_invalid_candidates_recover() {
    for reset in ["\x1b[0m", "\x1b[!p", "\x1bc"] {
        let mut screen = Screen::new(4, 20);
        let mut parser = TerminalParser::default();
        feed(
            &mut parser,
            &mut screen,
            "\x1b[4:3;58;5;123;8;53m\x1b7\x1b[0m\x1b8界".as_bytes(),
        );
        let cell = screen.cell(0, 0).clone();
        assert_eq!(cell.attrs.underline_style(), UnderlineStyle::Curly);
        assert_eq!(screen.cell(0, 1).attrs, cell.attrs);
        assert_eq!(screen.cell(0, 1).underline_color, cell.underline_color);
        feed(
            &mut parser,
            &mut screen,
            b"\x1b[?1049h\x1b[0mALT\x1b[?1049lX",
        );
        assert_eq!(screen.cell(0, 2).attrs, cell.attrs);
        assert_eq!(screen.cell(0, 2).underline_color, cell.underline_color);
        feed(&mut parser, &mut screen, b"\x1b[4:99;58:2::999:20:30mY");
        assert_eq!(screen.cell(0, 3).attrs, cell.attrs);
        assert_eq!(screen.cell(0, 3).underline_color, cell.underline_color);
        feed(
            &mut parser,
            &mut screen,
            format!("{reset}\x1b[1;1HZ").as_bytes(),
        );
        assert_eq!(
            screen.cell(0, 0),
            &Cell {
                ch: 'Z',
                ..Cell::default()
            }
        );
    }
}
