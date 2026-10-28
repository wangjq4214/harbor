use super::*;

#[test]
fn ich_via_csi_at_shifts_cells_right() {
    let mut terminal = Terminal::new_headless(1, 8);
    terminal.put_str("abcdef");
    terminal.put_bytes(b"\x1b[1;3H"); // CUP: col 3 (0-based col 2)
    terminal.put_bytes(b"\x1b[2@"); // ICH 2
    assert_eq!(terminal.row_text(0), "ab  cdef");
}

#[test]
fn dch_via_csi_p_shifts_cells_left() {
    let mut terminal = Terminal::new_headless(1, 8);
    terminal.put_str("abcdef");
    terminal.put_bytes(b"\x1b[1;3H"); // col 3
    terminal.put_bytes(b"\x1b[2P"); // DCH 2
    assert_eq!(terminal.row_text(0), "abef    ");
}

#[test]
fn il_via_csi_l_inserts_lines() {
    let mut terminal = Terminal::new_headless(5, 4);
    terminal.put_bytes(b"\x1b[1;1Haaaa");
    terminal.put_bytes(b"\x1b[2;1Hbbbb");
    terminal.put_bytes(b"\x1b[3;1Hcccc");
    terminal.put_bytes(b"\x1b[4;1Hdddd");
    terminal.put_bytes(b"\x1b[2;4r"); // CSI r: region rows 2-4 (1-based)
    terminal.put_bytes(b"\x1b[2;1H"); // cursor to row 2
    terminal.put_bytes(b"\x1b[1L"); // IL 1
    assert_eq!(terminal.row_text(0), "aaaa");
    assert_eq!(terminal.row_text(1), "    ");
    assert_eq!(terminal.row_text(2), "bbbb");
    assert_eq!(terminal.row_text(3), "cccc");
    assert_eq!(terminal.row_text(4), "    ");
}

#[test]
fn dl_via_csi_m_deletes_lines() {
    let mut terminal = Terminal::new_headless(5, 4);
    terminal.put_bytes(b"\x1b[1;1Haaaa");
    terminal.put_bytes(b"\x1b[2;1Hbbbb");
    terminal.put_bytes(b"\x1b[3;1Hcccc");
    terminal.put_bytes(b"\x1b[4;1Hdddd");
    terminal.put_bytes(b"\x1b[2;4r"); // CSI r: region rows 2-4
    terminal.put_bytes(b"\x1b[2;1H"); // cursor to row 2
    terminal.put_bytes(b"\x1b[1M"); // DL 1
    assert_eq!(terminal.row_text(0), "aaaa");
    assert_eq!(terminal.row_text(1), "cccc");
    assert_eq!(terminal.row_text(2), "dddd");
    assert_eq!(terminal.row_text(3), "    ");
    assert_eq!(terminal.row_text(4), "    ");
}

#[test]
fn su_via_csi_s_scrolls_up() {
    let mut terminal = Terminal::new_headless(5, 4);
    terminal.put_bytes(b"\x1b[1;1Haaaa");
    terminal.put_bytes(b"\x1b[2;1Hbbbb");
    terminal.put_bytes(b"\x1b[3;1Hcccc");
    terminal.put_bytes(b"\x1b[4;1Hdddd");
    terminal.put_bytes(b"\x1b[2;4r"); // CSI r: region rows 2-4
    terminal.put_bytes(b"\x1b[2S"); // SU 2
    assert_eq!(terminal.row_text(0), "aaaa");
    assert_eq!(terminal.row_text(1), "dddd"); // shifted up by 2
    assert_eq!(terminal.row_text(2), "    ");
    assert_eq!(terminal.row_text(3), "    ");
    assert_eq!(terminal.row_text(4), "    ");
}

#[test]
fn sd_via_csi_t_scrolls_down() {
    let mut terminal = Terminal::new_headless(5, 4);
    terminal.put_bytes(b"\x1b[1;1Haaaa");
    terminal.put_bytes(b"\x1b[2;1Hbbbb");
    terminal.put_bytes(b"\x1b[3;1Hcccc");
    terminal.put_bytes(b"\x1b[4;1Hdddd");
    terminal.put_bytes(b"\x1b[2;4r"); // CSI r: region rows 2-4
    terminal.put_bytes(b"\x1b[2T"); // SD 2
    assert_eq!(terminal.row_text(0), "aaaa");
    assert_eq!(terminal.row_text(1), "    ");
    assert_eq!(terminal.row_text(2), "    ");
    assert_eq!(terminal.row_text(3), "bbbb"); // shifted down by 2
    assert_eq!(terminal.row_text(4), "    ");
}

#[test]
fn decstbm_region_respected_by_scroll() {
    let mut terminal = Terminal::new_headless(4, 4);
    // Write content with default (full) scroll region first.
    terminal.put_bytes(b"\x1b[1;1Haaaa");
    terminal.put_bytes(b"\x1b[2;1Hbbbb");
    terminal.put_bytes(b"\x1b[3;1Hcccc");
    // Now set scroll region to [1,2] via CSI r.
    terminal.put_bytes(b"\x1b[2;3r"); // region rows 2-3 (1-based) = [1,2]
    // Newline at scroll_bottom (row 2, 0-based) → only region scrolls.
    terminal.put_bytes(b"\x1b[3;1H"); // cursor to row 3 = scroll_bottom
    terminal.put_str("\n");
    // Region [1,2] scrolled up: row 1 gets old row 2 "cccc", row 2 blanked.
    assert_eq!(terminal.row_text(0), "aaaa");
    assert_eq!(terminal.row_text(1), "cccc");
    assert_eq!(terminal.row_text(2), "    ");
    assert_eq!(terminal.row_text(3), "    ");
}

#[test]
fn decstbm_vim_like_scenario() {
    // Simulate vim setting scroll region, writing lines, and scrolling within region.
    let mut terminal = Terminal::new_headless(5, 10);
    // Write lines with default (full) scroll region first.
    terminal.put_bytes(b"\x1b[1;1Htitle");
    terminal.put_bytes(b"\x1b[2;1Hline1");
    terminal.put_bytes(b"\x1b[3;1Hline2");
    terminal.put_bytes(b"\x1b[4;1Hline3");
    // Set scroll region to [1,3].
    terminal.put_bytes(b"\x1b[2;4r"); // CSI r: region rows 2-4 (1-based)
    // Trigger scroll within region: newline from row 3 (scroll_bottom).
    terminal.put_bytes(b"\x1b[4;1H\n"); // LF → scroll_up region
    // Region [1,3] scrolled up: row 1 gets old row 2 "line2",
    // row 2 gets old row 3 "line3", row 3 blanked.
    assert_eq!(terminal.row_text(0).trim_end(), "title");
    assert_eq!(terminal.row_text(1), "line2     ");
    assert_eq!(terminal.row_text(2), "line3     ");
    assert_eq!(terminal.row_text(3), "          ");
}

#[test]
fn cursor_save_restore_via_esc_7_8() {
    let mut terminal = Terminal::new_headless(4, 10);
    terminal.put_bytes(b"\x1b[2;3H"); // cursor to row 2, col 3
    terminal.put_bytes(b"\x1b7"); // ESC 7 → save cursor (row 1, col 2)
    terminal.put_bytes(b"\x1b[4;8H"); // cursor to row 4, col 8
    terminal.put_str("XX"); // write at row 3, col 7
    terminal.put_bytes(b"\x1b8"); // ESC 8 → restore cursor to (row 1, col 2)
    terminal.put_str("YY"); // write starting at row 1, col 2
    // Row 1 should have spaces with YY at cols 2-3.
    let row1 = terminal.row_text(1);
    assert_eq!(&row1[..2], "  ");
    assert_eq!(&row1[2..4], "YY");
}
