use super::*;

#[test]
fn should_default_backdrop_availability_to_false_when_headless() {
    let terminal = Terminal::new_headless(2, 4);

    assert!(!terminal.backdrop_available);
}

#[test]
fn osc_background_updates_live_tint_and_reset_restores_startup_rgba() {
    let startup = Palette {
        background: Rgba::from_rgba8(1, 2, 3, 77),
        ..Palette::default()
    };
    let mut terminal =
        Terminal::new_headless_with_appearance(2, 4, TerminalAppearance::from_palette(startup));

    terminal.put_bytes(b"\x1b]11;#445566\x07");
    assert_eq!(
        terminal.clear_rgba(true),
        Rgba::from_rgba8(0x44, 0x55, 0x66, 77).components()
    );
    assert_eq!(
        terminal.clear_rgba(false),
        Rgba::from_rgb8(0x44, 0x55, 0x66).components()
    );

    terminal.put_bytes(b"\x1b]111;\x1b\\");
    assert_eq!(terminal.clear_rgba(true), startup.background.components());
}

#[test]
fn should_record_backdrop_availability_through_the_setter() {
    let mut terminal = Terminal::new_headless(2, 4);

    terminal.set_backdrop_available(true);
    assert!(terminal.backdrop_available);

    terminal.set_backdrop_available(false);
    assert!(!terminal.backdrop_available);
}

#[test]
fn width_round_trip_discards_ordinary_tail_but_preserves_cjk_and_hard_breaks() {
    fn retained_text(terminal: &Terminal) -> String {
        let screen = terminal.screen();
        let retained_rows = screen.scroll_count() + screen.rows();
        screen.selected_text(crate::SelectionBounds {
            start_row: screen.history_start(),
            start_col: 0,
            end_row: screen.history_start() + retained_rows as u64 - 1,
            end_col: screen.cols() - 1,
        })
    }

    let mut terminal = Terminal::new_headless(4, 8);
    terminal.put_str("A界  \r\n\r\nB");
    let expected = retained_text(&terminal);
    assert!(expected.contains("A界  \n\nB"));

    for cols in [6, 10, 8] {
        assert!(terminal.resize_if_changed(TerminalSize { rows: 4, cols }));
        assert_eq!(retained_text(&terminal), "A界\n\nB\n");
    }
}
