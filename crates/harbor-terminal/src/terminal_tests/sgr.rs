use super::*;

#[test]
fn sgr_bold_sets_attr() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[1ma");
    assert!(terminal.screen().cell(0, 0).attrs.contains(CellAttrs::BOLD));
}

#[test]
fn sgr_dim_sets_attr() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[2ma");
    assert!(terminal.screen().cell(0, 0).attrs.contains(CellAttrs::DIM));
}

#[test]
fn sgr_italic_sets_attr() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[3ma");
    assert!(
        terminal
            .screen()
            .cell(0, 0)
            .attrs
            .contains(CellAttrs::ITALIC)
    );
}

#[test]
fn sgr_underline_sets_attr() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[4ma");
    assert!(
        terminal
            .screen()
            .cell(0, 0)
            .attrs
            .contains(CellAttrs::UNDERLINE)
    );
}

#[test]
fn sgr_blink_sets_attr() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[5ma");
    assert!(
        terminal
            .screen()
            .cell(0, 0)
            .attrs
            .contains(CellAttrs::BLINK)
    );
}

#[test]
fn sgr_inverse_sets_attr() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[7ma");
    assert!(
        terminal
            .screen()
            .cell(0, 0)
            .attrs
            .contains(CellAttrs::INVERSE)
    );
}

#[test]
fn sgr_strikethrough_sets_attr() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[9ma");
    assert!(
        terminal
            .screen()
            .cell(0, 0)
            .attrs
            .contains(CellAttrs::STRIKETHROUGH)
    );
}

#[test]
fn sgr_reset_clears_all() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[1;31;42ma");
    terminal.put_bytes(b"\x1b[0mb");
    let cell = terminal.screen().cell(0, 1);
    assert_eq!(cell.fg, Color::Default);
    assert_eq!(cell.bg, Color::Default);
    assert!(cell.attrs.is_empty());
}

#[test]
fn sgr_8color_fg_sets_named() {
    for code in 30u8..=37u8 {
        let mut terminal = Terminal::new_headless(1, 2);
        let seq = format!("\x1b[{}mX", code);
        terminal.put_bytes(seq.as_bytes());
        assert_eq!(
            terminal.screen().cell(0, 0).fg,
            Color::Named(code - 30),
            "SGR {} should set fg Named({})",
            code,
            code - 30
        );
    }
}

#[test]
fn sgr_8color_bg_sets_named() {
    for code in 40u8..=47u8 {
        let mut terminal = Terminal::new_headless(1, 2);
        let seq = format!("\x1b[{}mX", code);
        terminal.put_bytes(seq.as_bytes());
        assert_eq!(
            terminal.screen().cell(0, 0).bg,
            Color::Named(code - 40),
            "SGR {} should set bg Named({})",
            code,
            code - 40
        );
    }
}

#[test]
fn sgr_bright_fg_sets_bright() {
    for code in 90u8..=97u8 {
        let mut terminal = Terminal::new_headless(1, 2);
        let seq = format!("\x1b[{}mX", code);
        terminal.put_bytes(seq.as_bytes());
        assert_eq!(
            terminal.screen().cell(0, 0).fg,
            Color::Bright(code - 90),
            "SGR {} should set fg Bright({})",
            code,
            code - 90
        );
    }
}

#[test]
fn sgr_bright_bg_sets_bright() {
    for code in 100u8..=107u8 {
        let mut terminal = Terminal::new_headless(1, 2);
        let seq = format!("\x1b[{}mX", code);
        terminal.put_bytes(seq.as_bytes());
        assert_eq!(
            terminal.screen().cell(0, 0).bg,
            Color::Bright(code - 100),
            "SGR {} should set bg Bright({})",
            code,
            code - 100
        );
    }
}

#[test]
fn sgr_256color_fg_sets_indexed() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[38;5;200mb");
    assert_eq!(terminal.screen().cell(0, 0).fg, Color::Indexed(200));
}

#[test]
fn sgr_256color_bg_sets_indexed() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[48;5;100mb");
    assert_eq!(terminal.screen().cell(0, 0).bg, Color::Indexed(100));
}

#[test]
fn sgr_truecolor_fg_sets_rgb() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[38;2;10;20;30mb");
    assert_eq!(terminal.screen().cell(0, 0).fg, Color::Rgb(10, 20, 30));
}

#[test]
fn sgr_truecolor_bg_sets_rgb() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[48;2;100;150;200mb");
    assert_eq!(terminal.screen().cell(0, 0).bg, Color::Rgb(100, 150, 200));
}

#[test]
fn sgr_multi_param_sets_all() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[1;31;44ma");
    let cell = terminal.screen().cell(0, 0);
    assert!(cell.attrs.contains(CellAttrs::BOLD));
    assert_eq!(cell.fg, Color::Named(1));
    assert_eq!(cell.bg, Color::Named(4));
}

#[test]
fn sgr_default_fg_bg_resets_colors() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[31;42m\x1b[39;49mb");
    let cell = terminal.screen().cell(0, 0);
    assert_eq!(cell.fg, Color::Default);
    assert_eq!(cell.bg, Color::Default);
}

#[test]
fn sgr_compound_clear_removes_attrs() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[1;3ma");
    let cell = terminal.screen().cell(0, 0);
    assert!(cell.attrs.contains(CellAttrs::BOLD));
    assert!(cell.attrs.contains(CellAttrs::ITALIC));
    terminal.put_bytes(b"\x1b[23mb");
    let cell = terminal.screen().cell(0, 1);
    assert!(!cell.attrs.contains(CellAttrs::ITALIC));
    assert!(cell.attrs.contains(CellAttrs::BOLD));
}

#[test]
fn sgr_22_clears_bold_and_dim() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[1;2ma\x1b[22mb");
    let cell = terminal.screen().cell(0, 1);
    assert!(!cell.attrs.contains(CellAttrs::BOLD));
    assert!(!cell.attrs.contains(CellAttrs::DIM));
}

#[test]
fn sgr_24_clears_underline() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[4ma\x1b[24mb");
    let cell = terminal.screen().cell(0, 1);
    assert!(!cell.attrs.contains(CellAttrs::UNDERLINE));
}

#[test]
fn sgr_25_clears_blink() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[5ma\x1b[25mb");
    let cell = terminal.screen().cell(0, 1);
    assert!(!cell.attrs.contains(CellAttrs::BLINK));
}

#[test]
fn sgr_27_clears_inverse() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[7ma\x1b[27mb");
    let cell = terminal.screen().cell(0, 1);
    assert!(!cell.attrs.contains(CellAttrs::INVERSE));
}

#[test]
fn sgr_29_clears_strikethrough() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[9ma\x1b[29mb");
    let cell = terminal.screen().cell(0, 1);
    assert!(!cell.attrs.contains(CellAttrs::STRIKETHROUGH));
}

#[test]
fn sgr_bare_csi_m_is_reset() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[1;31;42ma\x1b[mb");
    let cell = terminal.screen().cell(0, 1);
    assert_eq!(cell.fg, Color::Default);
    assert_eq!(cell.bg, Color::Default);
    assert!(cell.attrs.is_empty());
}

#[test]
fn sgr_indexed_out_of_range_ignored() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[31ma");
    terminal.put_bytes(b"\x1b[38;5;300mb");
    // 300 > 255 so fg should still be Named(1) from the 31 sequence
    assert_eq!(terminal.screen().cell(0, 1).fg, Color::Named(1));
}

#[test]
fn sgr_empty_extended_color_fields_preserve_colors_and_apply_following_sgr() {
    for params in ["38;5;", "48;5;", "38;", "48;"] {
        let mut terminal = Terminal::new_headless(1, 4);
        terminal.put_bytes(b"\x1b[1;31;42m");
        terminal.put_bytes(format!("\x1b[{params};22mX").as_bytes());

        let cell = terminal.screen().cell(0, 0);
        assert_eq!(cell.ch, 'X', "params: {params}");
        assert_eq!(cell.fg, Color::Named(1), "params: {params}");
        assert_eq!(cell.bg, Color::Named(2), "params: {params}");
        assert!(!cell.attrs.contains(CellAttrs::BOLD), "params: {params}");
    }
}

#[test]
fn sgr_incomplete_extended_colors_preserve_colors_and_attributes() {
    for params in ["38", "48", "38;5", "48;5", "38;2;128;64", "48;2;128;64"] {
        let mut terminal = Terminal::new_headless(1, 4);
        terminal.put_bytes(b"\x1b[1;31;42m");
        terminal.put_bytes(format!("\x1b[{params}mX").as_bytes());

        let cell = terminal.screen().cell(0, 0);
        assert_eq!(cell.ch, 'X', "params: {params}");
        assert_eq!(cell.fg, Color::Named(1), "params: {params}");
        assert_eq!(cell.bg, Color::Named(2), "params: {params}");
        assert!(cell.attrs.contains(CellAttrs::BOLD), "params: {params}");
    }
}

#[test]
fn sgr_truecolor_missing_params_ignored() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[31ma");
    terminal.put_bytes(b"\x1b[38;2;128;64mx");
    // Incomplete truecolor seq — fg stays red, 'x' still renders
    assert_eq!(terminal.row_text(0), "ax  ");
    assert_eq!(terminal.screen().cell(0, 1).fg, Color::Named(1));
}

#[test]
fn sgr_truecolor_component_out_of_range_ignored() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[31ma");
    terminal.put_bytes(b"\x1b[38;2;300;0;0mb");
    // 300 > 255 — fg stays red
    assert_eq!(terminal.screen().cell(0, 1).fg, Color::Named(1));
}

#[test]
fn csi_cursor_position_overwrites_target_cell() {
    let mut terminal = Terminal::new_headless(2, 4);

    terminal.put_bytes(b"abcd\x1b[1;2HZ");

    assert_eq!(terminal.row_text(0), "aZcd");
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (2, 0)
    );
}

#[test]
fn sgr_bg_preserved_after_erase_line() {
    // Vim's pattern: set bg → write text → CSI K (erase to end of line)
    let mut terminal = Terminal::new_headless(1, 6);
    terminal.put_bytes(b"\x1b[44mHi\x1b[K");
    // "Hi" should have blue bg; erased remainder should also have blue bg
    let cell = terminal.screen();
    assert_eq!(cell.cell(0, 0).ch, 'H');
    assert_eq!(cell.cell(0, 0).bg, Color::Named(4));
    assert_eq!(cell.cell(0, 1).ch, 'i');
    assert_eq!(cell.cell(0, 1).bg, Color::Named(4));
    // erased cells (cols 2-5) should have the same bg, not default
    for col in 2..6 {
        assert_eq!(cell.cell(0, col).bg, Color::Named(4));
    }
}

#[test]
fn sgr_bg_preserved_after_erase_display() {
    let mut terminal = Terminal::new_headless(2, 4);
    // Set bg green, write, erase entire display
    terminal.put_bytes(b"\x1b[42mab\x1b[2J");
    for row in 0..2 {
        for col in 0..4 {
            assert_eq!(
                terminal.screen().cell(row, col).bg,
                Color::Named(2),
                "erase_display(2) should preserve current_bg in all cells"
            );
        }
    }
}

#[test]
fn default_bg_after_sgr_reset_and_erase() {
    // After SGR reset (ESC [ m), erasing should produce default-bg cells
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[44mHi\x1b[0m\x1b[K");
    // "Hi" was written before reset, so still has blue bg
    assert_eq!(terminal.screen().cell(0, 0).ch, 'H');
    assert_eq!(terminal.screen().cell(0, 0).bg, Color::Named(4));
    assert_eq!(terminal.screen().cell(0, 1).ch, 'i');
    assert_eq!(terminal.screen().cell(0, 1).bg, Color::Named(4));
    // erased cells (cols 2-3) were erased after SGR reset → default bg
    for col in 2..4 {
        assert_eq!(terminal.screen().cell(0, col).bg, Color::Default);
    }
}

#[test]
fn bracketed_paste_mode_tracks_decset_and_decrst() {
    let mut terminal = Terminal::new_headless(1, 1);

    terminal.put_bytes(b"\x1b[?2004h");
    assert!(terminal.screen().input_modes().bracketed_paste);

    terminal.put_bytes(b"\x1b[?2004l");
    assert!(!terminal.screen().input_modes().bracketed_paste);
}
